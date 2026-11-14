//! pi-gate 통합 테스트. 서버를 `127.0.0.1:0`에 띄워 실제 HTTP · 웹소켓으로 확인한다.
//!
//! 근거: `docs/dev/modules/gate.md`의 "pi-gate가 만드는 것" 1~8.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, AtomicUsize, Ordering};
use std::time::Duration;

use axum::Router;
use axum::routing::{get, post};
use futures::StreamExt;
use pi_gate::{
    ACCESS_ENTER, Clock, DEFAULT_ALLOWED_HOSTS, DEFAULT_PORT, DEFAULT_SEND_BUFFER, Gate,
    GateBuilder, GateConfig, GateError, GateServer, LAST_SEQ_PARAM, Redactor, SESSION_COOKIE,
    SESSION_PATH, WS_PATH,
};
use pi_store::{Store, UnixMillis};
use serde_json::{Value, json};
use tempfile::TempDir;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

// ───────────── 도우미 ─────────────

/// 한 걸음(요청 하나 · 프레임 하나)을 기다리는 가장 긴 시간.
const STEP: Duration = Duration::from_secs(10);
/// "오지 않는다"를 확인할 때 기다리는 시간.
const QUIET: Duration = Duration::from_millis(300);
/// 테스트 시계의 기준 시각.
const T0: i64 = 1_800_000_000_000;
/// 공개 경로(세션 없이 열린다)와 그 본문.
const PUBLIC_PATH: &str = "/";
const PUBLIC_BODY: &str = "공개 화면";
/// 보호된 경로(세션이 있어야 한다).
const PING_PATH: &str = "/api/ping";
const POKE_PATH: &str = "/api/poke";
/// 루프백이 아닌 상대(문서용 주소 대역).
const OUTSIDE_PEER: &str = "203.0.113.9:40000";

type Ws = WebSocketStream<MaybeTlsStream<TcpStream>>;

/// 테스트가 시각을 정하는 시계.
struct TestClock(AtomicI64);

impl TestClock {
    fn set(&self, millis: i64) {
        self.0.store(millis, Ordering::SeqCst);
    }
}

impl Clock for TestClock {
    fn now(&self) -> UnixMillis {
        UnixMillis(self.0.load(Ordering::SeqCst))
    }
}

/// 띄운 서버 하나와 그 둘레.
struct Harness {
    dir: TempDir,
    db: PathBuf,
    store: Store,
    clock: Arc<TestClock>,
    server: GateServer,
    addr: SocketAddr,
    /// 공개 · 보호 핸들러가 불린 횟수.
    hits: Arc<AtomicUsize>,
}

/// 핸들러가 불린 횟수를 세는 공개 · 보호 경로.
fn counted_routes(hits: &Arc<AtomicUsize>) -> (Router, Router) {
    let h = Arc::clone(hits);
    let public = Router::new().route(
        PUBLIC_PATH,
        get(move || {
            let h = Arc::clone(&h);
            async move {
                h.fetch_add(1, Ordering::SeqCst);
                PUBLIC_BODY
            }
        }),
    );
    let h1 = Arc::clone(hits);
    let h2 = Arc::clone(hits);
    let protected = Router::new()
        .route(
            PING_PATH,
            get(move || {
                let h = Arc::clone(&h1);
                async move {
                    h.fetch_add(1, Ordering::SeqCst);
                    "pong"
                }
            }),
        )
        .route(
            POKE_PATH,
            post(move || {
                let h = Arc::clone(&h2);
                async move {
                    h.fetch_add(1, Ordering::SeqCst);
                    "poked"
                }
            }),
        );
    (public, protected)
}

/// 임시 폴더의 저장소로 서버를 띄운다. 설정과 조립을 테스트가 바꿀 수 있다.
async fn start_with(
    config: impl FnOnce(GateConfig) -> GateConfig,
    build: impl FnOnce(GateBuilder) -> GateBuilder,
) -> Harness {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("gate.sqlite");
    let store = Store::open(&db).unwrap();
    let clock = Arc::new(TestClock(AtomicI64::new(T0)));
    let hits = Arc::new(AtomicUsize::new(0));
    let server = serve_on(&store, &clock, &hits, config, build).await;
    let addr = server.local_addr();
    Harness {
        dir,
        db,
        store,
        clock,
        server,
        addr,
        hits,
    }
}

/// 이미 있는 저장소로 서버를 띄운다(다시 켜기).
async fn serve_on(
    store: &Store,
    clock: &Arc<TestClock>,
    hits: &Arc<AtomicUsize>,
    config: impl FnOnce(GateConfig) -> GateConfig,
    build: impl FnOnce(GateBuilder) -> GateBuilder,
) -> GateServer {
    let (public, protected) = counted_routes(hits);
    let builder = Gate::builder(store.clone(), config(GateConfig::local(0)))
        .clock(Arc::clone(clock) as Arc<dyn Clock>)
        .public_routes(public)
        .protected_routes(protected);
    tokio::time::timeout(STEP, build(builder).build().serve())
        .await
        .expect("서버가 시간 안에 떠야 한다")
        .expect("서버가 떠야 한다")
}

async fn start() -> Harness {
    start_with(|c| c, |b| b).await
}

/// 루프백이 아닌 상대로 보이게 하는 조립.
fn outside(builder: GateBuilder) -> GateBuilder {
    let peer: SocketAddr = OUTSIDE_PEER.parse().unwrap();
    builder.peer_override(Arc::new(move |_actual| peer))
}

/// HTTP 응답.
struct Reply {
    status: u16,
    headers: Vec<(String, String)>,
    body: String,
}

impl Reply {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    fn is_rejected(&self) -> bool {
        (400..500).contains(&self.status)
    }

    /// `Set-Cookie`의 세션 값과 속성들(소문자).
    fn session_cookie(&self) -> Option<(String, Vec<String>)> {
        let raw = self.header("set-cookie")?;
        let mut parts = raw.split(';').map(str::trim);
        let (name, value) = parts.next()?.split_once('=')?;
        if name != SESSION_COOKIE {
            return None;
        }
        let attrs = parts.map(str::to_ascii_lowercase).collect();
        Some((value.to_string(), attrs))
    }
}

/// HTTP/1.1 요청 하나를 날것으로 보낸다. 헤더는 준 것만 나간다(Host도 직접 준다).
async fn http(addr: SocketAddr, method: &str, path: &str, headers: &[(&str, &str)]) -> Reply {
    let work = async {
        let mut stream = TcpStream::connect(addr).await.unwrap();
        let mut request = format!("{method} {path} HTTP/1.1\r\n");
        for (name, value) in headers {
            request.push_str(&format!("{name}: {value}\r\n"));
        }
        request.push_str("Content-Length: 0\r\nConnection: close\r\n\r\n");
        stream.write_all(request.as_bytes()).await.unwrap();
        let mut raw = Vec::new();
        stream.read_to_end(&mut raw).await.unwrap();
        String::from_utf8_lossy(&raw).into_owned()
    };
    let text = tokio::time::timeout(STEP, work)
        .await
        .expect("응답이 시간 안에 와야 한다");
    let (head, body) = text.split_once("\r\n\r\n").unwrap_or((text.as_str(), ""));
    let mut lines = head.lines();
    let status = lines
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|code| code.parse().ok())
        .expect("상태 줄이 있어야 한다");
    let headers = lines
        .filter_map(|line| line.split_once(':'))
        .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
        .collect();
    Reply {
        status,
        headers,
        body: body.to_string(),
    }
}

fn host_of(addr: SocketAddr) -> String {
    format!("127.0.0.1:{}", addr.port())
}

fn origin_of(addr: SocketAddr) -> String {
    format!("http://{}", host_of(addr))
}

/// 같은 출처에서 세션을 달라고 한다.
async fn enter(addr: SocketAddr) -> Reply {
    let (host, origin) = (host_of(addr), origin_of(addr));
    http(
        addr,
        "POST",
        SESSION_PATH,
        &[("Host", &host), ("Origin", &origin)],
    )
    .await
}

/// 세션을 만들고 `Cookie` 헤더에 넣을 글자를 돌려준다.
async fn login(addr: SocketAddr) -> String {
    let reply = enter(addr).await;
    assert_eq!(reply.status, 204, "루프백에서는 세션을 받아야 한다");
    let (value, _) = reply.session_cookie().expect("세션 쿠키가 있어야 한다");
    format!("{SESSION_COOKIE}={value}")
}

/// 웹소켓을 연다. HTTP 단계에서 거부되면 그 상태 코드를 돌려준다.
async fn ws_try(
    addr: SocketAddr,
    cookie: Option<&str>,
    origin: Option<&str>,
    last_seq: &str,
) -> Result<Ws, u16> {
    let url = format!(
        "ws://{}{WS_PATH}?{LAST_SEQ_PARAM}={last_seq}",
        host_of(addr)
    );
    let mut request = url.into_client_request().unwrap();
    if let Some(cookie) = cookie {
        request
            .headers_mut()
            .insert("Cookie", cookie.parse().unwrap());
    }
    if let Some(origin) = origin {
        request
            .headers_mut()
            .insert("Origin", origin.parse().unwrap());
    }
    let result = tokio::time::timeout(STEP, tokio_tungstenite::connect_async(request))
        .await
        .expect("웹소켓 응답이 시간 안에 와야 한다");
    match result {
        Ok((ws, _)) => Ok(ws),
        Err(tokio_tungstenite::tungstenite::Error::Http(response)) => {
            Err(response.status().as_u16())
        }
        Err(other) => panic!("웹소켓 연결이 뜻밖의 모양으로 실패했다: {other}"),
    }
}

/// 세션과 같은 출처로 웹소켓을 연다.
async fn ws_open(addr: SocketAddr, cookie: &str, last_seq: u64) -> Ws {
    ws_try(
        addr,
        Some(cookie),
        Some(&origin_of(addr)),
        &last_seq.to_string(),
    )
    .await
    .expect("세션이 있으면 웹소켓이 열려야 한다")
}

/// 다음 글자 프레임. 연결이 닫혔으면 `None`. 시간 안에 아무 일도 없으면 `Err`.
async fn next_text(ws: &mut Ws, wait: Duration) -> Result<Option<String>, ()> {
    let deadline = tokio::time::Instant::now() + wait;
    loop {
        match tokio::time::timeout_at(deadline, ws.next()).await {
            Err(_) => return Err(()),
            Ok(None | Some(Err(_)) | Some(Ok(Message::Close(_)))) => return Ok(None),
            Ok(Some(Ok(Message::Text(text)))) => return Ok(Some(text.as_str().to_string())),
            Ok(Some(Ok(_))) => {}
        }
    }
}

/// 다음 프레임을 JSON으로 읽는다. 닫혔거나 늦으면 테스트가 실패한다.
async fn next_frame(ws: &mut Ws) -> Value {
    let text = next_text(ws, STEP)
        .await
        .expect("프레임이 시간 안에 와야 한다")
        .expect("연결이 열려 있어야 한다");
    serde_json::from_str(&text).expect("프레임은 JSON이어야 한다")
}

/// 저장된 이벤트 프레임의 번호. 지나가는 이벤트(번호 없음)면 `None`.
fn seq_of(frame: &Value) -> Option<u64> {
    frame.get("seq").and_then(Value::as_u64)
}

/// 번호 있는 프레임을 `count`개 읽어 번호들을 돌려준다(지나가는 이벤트는 건너뛴다).
async fn read_seqs(ws: &mut Ws, count: usize) -> Vec<u64> {
    let mut seqs = Vec::new();
    while seqs.len() < count {
        if let Some(seq) = seq_of(&next_frame(ws).await) {
            seqs.push(seq);
        }
    }
    seqs
}

/// 잠깐 동안 아무 프레임도 오지 않는지 본다.
async fn assert_quiet(ws: &mut Ws) {
    let got = next_text(ws, QUIET).await;
    assert!(got.is_err(), "더 올 프레임이 없어야 한다: {got:?}");
}

/// 접속 기록 가운데 "들어옴"인 것.
fn enters(store: &Store) -> Vec<pi_store::AccessRecord> {
    store
        .access_log()
        .unwrap()
        .into_iter()
        .filter(|record| record.action == ACCESS_ENTER)
        .collect()
}

// ───────────── 1. 그 컴퓨터에서만 열기 (D2) ─────────────

// 검증: D2
#[test]
fn 기본_설정은_그_컴퓨터_주소로만_연다() {
    let config = GateConfig::default();
    assert_eq!(
        config.bind,
        SocketAddr::from(([127, 0, 0, 1], DEFAULT_PORT)),
        "아무것도 주지 않으면 127.0.0.1이다"
    );
    assert_eq!(config.allowed_hosts, DEFAULT_ALLOWED_HOSTS);
    assert_eq!(config.send_buffer, DEFAULT_SEND_BUFFER);

    let local = GateConfig::local(4321);
    assert_eq!(local.bind, SocketAddr::from(([127, 0, 0, 1], 4321)));
    assert_eq!(local.allowed_hosts, config.allowed_hosts);
    assert_eq!(
        DEFAULT_ALLOWED_HOSTS,
        ["localhost", "127.0.0.1", "[::1]"],
        "기본 허용 이름"
    );
}

// 검증: D2
#[tokio::test]
async fn 띄운_서버는_루프백_주소에_열린다() {
    let h = start().await;
    assert!(h.addr.ip().is_loopback(), "연 주소: {}", h.addr);
    assert_ne!(h.addr.port(), 0, "포트 0을 주면 고른 포트를 알려 준다");
    let reply = http(h.addr, "GET", PUBLIC_PATH, &[("Host", &host_of(h.addr))]).await;
    assert_eq!(reply.status, 200);
    assert!(reply.body.contains(PUBLIC_BODY));
}

// 검증: D2
#[tokio::test]
async fn 이미_쓰는_주소는_열지_못하고_오류를_준다() {
    let taken = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = taken.local_addr().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("gate.sqlite")).unwrap();
    let config = GateConfig {
        bind: addr,
        ..GateConfig::local(0)
    };
    let result = tokio::time::timeout(STEP, Gate::builder(store, config).build().serve())
        .await
        .expect("시간 안에 끝나야 한다");
    assert!(
        matches!(result, Err(GateError::Bind { .. })),
        "쓰는 중인 주소는 Bind 오류다"
    );
}

// ───────────── 2. 그 컴퓨터인지 확인 (C15) · 4. 세션 · 5. 접속 기록 (M6) ─────────────

// 검증: C15, M6
#[tokio::test]
async fn 루프백에서는_접속_키_없이_세션을_받고_접속_기록이_남는다() {
    let h = start().await;
    h.clock.set(T0 + 1234);
    let reply = enter(h.addr).await;
    assert_eq!(reply.status, 204);

    let (value, attrs) = reply.session_cookie().expect("세션 쿠키가 있어야 한다");
    assert!(
        value.len() >= 43,
        "256비트를 담으려면 43자 이상이다: {}자",
        value.len()
    );
    assert!(
        value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'),
        "세션 값은 URL에 안전한 글자만 쓴다"
    );
    for wanted in ["httponly", "samesite=strict", "path=/"] {
        assert!(
            attrs.iter().any(|a| a == wanted),
            "쿠키 속성 {wanted}가 있어야 한다: {attrs:?}"
        );
    }

    let records = h.store.access_log().unwrap();
    assert_eq!(records.len(), 1, "들어올 때 한 줄을 남긴다");
    assert_eq!(records[0].at, UnixMillis(T0 + 1234), "언제");
    assert_eq!(records[0].origin, "127.0.0.1", "어디서(상대 IP, 포트 없이)");
    assert_eq!(records[0].action, ACCESS_ENTER, "무엇을");
}

// 검증: M6
#[tokio::test]
async fn 들어올_때마다_접속_기록이_한_줄씩_쌓인다() {
    let h = start().await;
    for i in 0..3 {
        h.clock.set(T0 + i * 1000);
        assert_eq!(enter(h.addr).await.status, 204);
    }
    let records = enters(&h.store);
    let seqs: Vec<u64> = records.iter().map(|r| r.seq).collect();
    let ats: Vec<i64> = records.iter().map(|r| r.at.0).collect();
    assert_eq!(seqs, [1, 2, 3], "지우지 않고 쌓는다");
    assert_eq!(ats, [T0, T0 + 1000, T0 + 2000]);
}

// 검증: C15, M6
#[tokio::test]
async fn 세션_값은_매번_다르고_저장소에_남지_않는다() {
    let h = start().await;
    let first = login(h.addr).await;
    let second = login(h.addr).await;
    assert_ne!(first, second, "세션 값은 추측할 수 없는 무작위 값이다");

    for cookie in [&first, &second] {
        let value = cookie.split_once('=').unwrap().1;
        for record in h.store.access_log().unwrap() {
            assert!(!record.origin.contains(value) && !record.action.contains(value));
        }
        for entry in std::fs::read_dir(h.dir.path()).unwrap() {
            let bytes = std::fs::read(entry.unwrap().path()).unwrap();
            let found = bytes.windows(value.len()).any(|w| w == value.as_bytes());
            assert!(!found, "세션 값 원문이 파일에 있으면 안 된다");
        }
    }
    assert!(h.db.exists());
}

// 검증: C15
#[tokio::test]
async fn 루프백이_아닌_상대는_모두_401이다() {
    let h = start_with(|c| c, outside).await;
    let (host, origin) = (host_of(h.addr), origin_of(h.addr));

    let reply = enter(h.addr).await;
    assert_eq!(
        reply.status, 401,
        "접속 키는 이번 범위가 아니라 모두 거부한다"
    );
    assert!(reply.session_cookie().is_none(), "세션을 주지 않는다");

    let page = http(h.addr, "GET", PUBLIC_PATH, &[("Host", &host)]).await;
    assert_eq!(page.status, 401, "공개 경로도 같다");
    let ping = http(h.addr, "GET", PING_PATH, &[("Host", &host)]).await;
    assert_eq!(ping.status, 401);
    let ws = ws_try(h.addr, None, Some(&origin), "0").await;
    assert_eq!(ws.err(), Some(401), "웹소켓도 같다");

    assert_eq!(h.hits.load(Ordering::SeqCst), 0, "핸들러가 불리지 않는다");
    assert!(enters(&h.store).is_empty(), "들어옴 기록이 없다");
}

// 검증: C15
#[tokio::test]
async fn 전달_헤더로_루프백인_척해도_거부한다() {
    let h = start_with(|c| c, outside).await;
    let (host, origin) = (host_of(h.addr), origin_of(h.addr));
    for (name, value) in [
        ("X-Forwarded-For", "127.0.0.1"),
        ("Forwarded", "for=127.0.0.1"),
        ("X-Real-IP", "127.0.0.1"),
    ] {
        let reply = http(
            h.addr,
            "POST",
            SESSION_PATH,
            &[("Host", &host), ("Origin", &origin), (name, value)],
        )
        .await;
        assert_eq!(reply.status, 401, "{name}를 믿으면 안 된다");
        assert!(reply.session_cookie().is_none());
    }
    assert!(enters(&h.store).is_empty());
}

// 검증: C15, M6
#[tokio::test]
async fn 전달_헤더가_있어도_실제_상대_주소로_판단하고_기록한다() {
    let h = start().await;
    let (host, origin) = (host_of(h.addr), origin_of(h.addr));
    let reply = http(
        h.addr,
        "POST",
        SESSION_PATH,
        &[
            ("Host", &host),
            ("Origin", &origin),
            ("X-Forwarded-For", "203.0.113.9"),
            ("X-Real-IP", "203.0.113.9"),
        ],
    )
    .await;
    assert_eq!(reply.status, 204, "실제 상대가 루프백이면 들어온다");
    let records = enters(&h.store);
    assert_eq!(records.len(), 1);
    assert_eq!(
        records[0].origin, "127.0.0.1",
        "헤더가 아니라 실제 상대 주소를 남긴다"
    );
}

// ───────────── 3. Host · Origin 확인 ─────────────

// 검증: C15
#[tokio::test]
async fn 허용_목록에_없는_host는_거부한다() {
    let h = start().await;
    let port = h.addr.port();
    let bad_hosts = [
        format!("evil.example:{port}"),
        "evil.example".to_string(),
        format!("127.0.0.1.evil.example:{port}"),
        format!("127.0.0.1:{}", port.wrapping_add(1).max(1)),
        "localhost".to_string(),
    ];
    for host in &bad_hosts {
        let page = http(h.addr, "GET", PUBLIC_PATH, &[("Host", host)]).await;
        assert!(page.is_rejected(), "Host {host}: {}", page.status);
        assert!(!page.body.contains(PUBLIC_BODY));
        let session = http(
            h.addr,
            "POST",
            SESSION_PATH,
            &[("Host", host), ("Origin", &format!("http://{host}"))],
        )
        .await;
        assert!(session.is_rejected(), "Host {host}: {}", session.status);
        assert!(session.session_cookie().is_none());
    }
    assert_eq!(h.hits.load(Ordering::SeqCst), 0, "핸들러가 불리지 않는다");
    assert!(enters(&h.store).is_empty());

    for host in [
        format!("localhost:{port}"),
        format!("127.0.0.1:{port}"),
        format!("[::1]:{port}"),
    ] {
        let page = http(h.addr, "GET", PUBLIC_PATH, &[("Host", &host)]).await;
        assert_eq!(page.status, 200, "기본 허용 Host {host}");
    }
    assert_eq!(h.hits.load(Ordering::SeqCst), 3);
}

// 검증: C15
#[tokio::test]
async fn 허용_host_목록은_설정으로_바꾼다() {
    let h = start_with(
        |c| GateConfig {
            allowed_hosts: vec!["pi.test".to_string()],
            ..c
        },
        |b| b,
    )
    .await;
    let port = h.addr.port();
    let allowed = format!("pi.test:{port}");
    let page = http(h.addr, "GET", PUBLIC_PATH, &[("Host", &allowed)]).await;
    assert_eq!(page.status, 200, "설정한 이름은 받는다");
    let session = http(
        h.addr,
        "POST",
        SESSION_PATH,
        &[("Host", &allowed), ("Origin", &format!("http://{allowed}"))],
    )
    .await;
    assert_eq!(session.status, 204, "Origin은 Host 헤더와 같은 출처면 된다");

    let default_host = format!("localhost:{port}");
    let page = http(h.addr, "GET", PUBLIC_PATH, &[("Host", &default_host)]).await;
    assert!(page.is_rejected(), "목록에서 뺀 이름은 거부한다");
}

// 검증: C15
#[tokio::test]
async fn 세션_요청은_같은_출처가_아니면_거부한다() {
    let h = start().await;
    let host = host_of(h.addr);
    let port = h.addr.port();
    let bad_origins = [
        Some("http://evil.example".to_string()),
        Some(format!("http://evil.example:{port}")),
        Some(format!("http://localhost:{port}")),
        Some(format!("https://{host}")),
        Some("null".to_string()),
        None,
    ];
    for origin in &bad_origins {
        let mut headers = vec![("Host", host.as_str())];
        if let Some(origin) = origin {
            headers.push(("Origin", origin.as_str()));
        }
        let reply = http(h.addr, "POST", SESSION_PATH, &headers).await;
        assert!(reply.is_rejected(), "Origin {origin:?}: {}", reply.status);
        assert!(reply.session_cookie().is_none(), "Origin {origin:?}");
    }
    assert!(
        enters(&h.store).is_empty(),
        "거부된 요청은 들어옴으로 남기지 않는다"
    );
    assert_eq!(enter(h.addr).await.status, 204, "같은 출처는 받는다");
}

// 검증: C15
#[tokio::test]
async fn 상태를_바꾸는_요청은_세션이_있어도_같은_출처여야_한다() {
    let h = start().await;
    let cookie = login(h.addr).await;
    let (host, origin) = (host_of(h.addr), origin_of(h.addr));

    let cross = http(
        h.addr,
        "POST",
        POKE_PATH,
        &[
            ("Host", &host),
            ("Cookie", &cookie),
            ("Origin", "http://evil.example"),
        ],
    )
    .await;
    assert!(cross.is_rejected(), "다른 출처: {}", cross.status);
    let missing = http(
        h.addr,
        "POST",
        POKE_PATH,
        &[("Host", &host), ("Cookie", &cookie)],
    )
    .await;
    assert!(missing.is_rejected(), "Origin 없음: {}", missing.status);
    assert_eq!(h.hits.load(Ordering::SeqCst), 0, "핸들러가 불리지 않는다");

    let same = http(
        h.addr,
        "POST",
        POKE_PATH,
        &[("Host", &host), ("Cookie", &cookie), ("Origin", &origin)],
    )
    .await;
    assert_eq!(same.status, 200);
    assert!(same.body.contains("poked"));

    let read = http(
        h.addr,
        "GET",
        PING_PATH,
        &[("Host", &host), ("Cookie", &cookie)],
    )
    .await;
    assert_eq!(read.status, 200, "읽기만 하는 요청은 Origin이 없어도 된다");
    assert_eq!(h.hits.load(Ordering::SeqCst), 2);
}

// ───────────── 4. 세션 ─────────────

// 검증: C15
#[tokio::test]
async fn 세션_없이_보호된_경로를_부르면_401이다() {
    let h = start().await;
    let (host, origin) = (host_of(h.addr), origin_of(h.addr));
    let forged = format!("{SESSION_COOKIE}={}", "a".repeat(43));
    let empty = format!("{SESSION_COOKIE}=");

    let none = http(h.addr, "GET", PING_PATH, &[("Host", &host)]).await;
    assert_eq!(none.status, 401, "세션 없음");
    for cookie in [&forged, &empty] {
        let reply = http(
            h.addr,
            "GET",
            PING_PATH,
            &[("Host", &host), ("Cookie", cookie)],
        )
        .await;
        assert_eq!(reply.status, 401, "모르는 세션 값");
        let poke = http(
            h.addr,
            "POST",
            POKE_PATH,
            &[("Host", &host), ("Origin", &origin), ("Cookie", cookie)],
        )
        .await;
        assert_eq!(poke.status, 401);
    }
    assert_eq!(h.hits.load(Ordering::SeqCst), 0, "핸들러가 불리지 않는다");

    let cookie = login(h.addr).await;
    let reply = http(
        h.addr,
        "GET",
        PING_PATH,
        &[("Host", &host), ("Cookie", &cookie)],
    )
    .await;
    assert_eq!(reply.status, 200, "세션이 있으면 열린다");
    assert!(reply.body.contains("pong"));
    let with_others = format!("theme=dark; {cookie}; lang=ko");
    let reply = http(
        h.addr,
        "GET",
        PING_PATH,
        &[("Host", &host), ("Cookie", &with_others)],
    )
    .await;
    assert_eq!(reply.status, 200, "다른 쿠키와 섞여 와도 찾는다");
}

// ───────────── 6. 실시간 연결 (C2 · D11 · E3) ─────────────

// 검증: C2, C15
#[tokio::test]
async fn 웹소켓은_세션과_같은_출처가_있어야_열린다() {
    let h = start().await;
    let origin = origin_of(h.addr);
    let forged = format!("{SESSION_COOKIE}={}", "b".repeat(43));

    let none = ws_try(h.addr, None, Some(&origin), "0").await;
    assert_eq!(none.err(), Some(401), "세션 없음");
    let unknown = ws_try(h.addr, Some(&forged), Some(&origin), "0").await;
    assert_eq!(unknown.err(), Some(401), "모르는 세션 값");

    let cookie = login(h.addr).await;
    let cross = ws_try(h.addr, Some(&cookie), Some("http://evil.example"), "0").await;
    assert!(
        matches!(cross, Err(status) if (400..500).contains(&status)),
        "다른 출처의 웹소켓은 거부한다"
    );
    let missing = ws_try(h.addr, Some(&cookie), None, "0").await;
    assert!(
        matches!(missing, Err(status) if (400..500).contains(&status)),
        "Origin 없는 웹소켓은 거부한다"
    );
    for bad in ["abc", "-1", "1.5"] {
        let reply = ws_try(h.addr, Some(&cookie), Some(&origin), bad).await;
        assert!(
            matches!(reply, Err(status) if (400..500).contains(&status)),
            "숫자가 아닌 번호 {bad}"
        );
    }

    let mut ws = ws_open(h.addr, &cookie, 0).await;
    let seq = h
        .server
        .publisher()
        .publish("ping", json!({}))
        .await
        .unwrap();
    assert_eq!(seq_of(&next_frame(&mut ws).await), Some(seq));
}

// 검증: C2, D11
#[tokio::test]
async fn 이벤트_프레임은_번호_종류_내용_시각을_가진다() {
    let h = start().await;
    let cookie = login(h.addr).await;
    let mut ws = ws_open(h.addr, &cookie, 0).await;
    let publisher = h.server.publisher();

    h.clock.set(T0 + 5000);
    let payload = json!({ "from": "분석 담당", "text": "삼성전자 거래량이 늘었습니다", "n": 3 });
    let first = publisher.publish("chat", payload.clone()).await.unwrap();
    h.clock.set(T0 + 6000);
    let second = publisher
        .publish("notice", json!({ "level": "info" }))
        .await
        .unwrap();
    assert_eq!((first, second), (1, 2), "번호는 1부터 하나씩 커진다");

    let frame = next_frame(&mut ws).await;
    assert_eq!(frame["seq"], json!(1));
    assert_eq!(frame["kind"], json!("chat"));
    assert_eq!(frame["payload"], payload);
    assert_eq!(frame["at"], json!(T0 + 5000));
    let frame = next_frame(&mut ws).await;
    assert_eq!(frame["seq"], json!(2));
    assert_eq!(frame["kind"], json!("notice"));
    assert_eq!(frame["payload"], json!({ "level": "info" }));
    assert_eq!(frame["at"], json!(T0 + 6000));
}

// 검증: C2, E3
#[tokio::test]
async fn 마지막으로_받은_번호_뒤의_이벤트부터_순서대로_받는다() {
    let h = start().await;
    let publisher = h.server.publisher();
    for i in 1..=5 {
        h.clock.set(T0 + i);
        publisher.publish("step", json!({ "i": i })).await.unwrap();
    }
    let cookie = login(h.addr).await;

    let mut from_two = ws_open(h.addr, &cookie, 2).await;
    for i in 3..=5 {
        let frame = next_frame(&mut from_two).await;
        assert_eq!(frame["seq"], json!(i));
        assert_eq!(frame["payload"], json!({ "i": i }));
        assert_eq!(frame["at"], json!(T0 + i), "저장된 때의 시각 그대로다");
    }
    let mut from_start = ws_open(h.addr, &cookie, 0).await;
    assert_eq!(read_seqs(&mut from_start, 5).await, [1, 2, 3, 4, 5]);
    let mut caught_up = ws_open(h.addr, &cookie, 5).await;
    assert_quiet(&mut caught_up).await;

    let sixth = publisher.publish("step", json!({ "i": 6 })).await.unwrap();
    assert_eq!(sixth, 6);
    for ws in [&mut from_two, &mut from_start, &mut caught_up] {
        assert_eq!(read_seqs(ws, 1).await, [6], "이어서 새 이벤트를 받는다");
        assert_quiet(ws).await;
    }
}

// 검증: C2, E3
#[tokio::test]
async fn 끊겼다가_다시_이어도_번호가_빠지거나_겹치지_않는다() {
    const TOTAL: u64 = 60;
    let h = start().await;
    let cookie = login(h.addr).await;
    let publisher = h.server.publisher();
    let writer = tokio::spawn(async move {
        for i in 1..=TOTAL {
            publisher.publish("tick", json!({ "i": i })).await.unwrap();
            tokio::task::yield_now().await;
        }
    });

    let mut seen = Vec::new();
    let mut ws = ws_open(h.addr, &cookie, 0).await;
    seen.extend(read_seqs(&mut ws, 20).await);
    drop(ws);
    // 끊긴 동안에도 발행은 이어진다(E3: 팀의 운용은 그대로 이어진다).
    let last = *seen.last().unwrap();
    let mut ws = ws_open(h.addr, &cookie, last).await;
    seen.extend(read_seqs(&mut ws, 15).await);
    drop(ws);
    let last = *seen.last().unwrap();
    let mut ws = ws_open(h.addr, &cookie, last).await;
    let rest = usize::try_from(TOTAL).unwrap() - seen.len();
    seen.extend(read_seqs(&mut ws, rest).await);

    tokio::time::timeout(STEP, writer).await.unwrap().unwrap();
    let expected: Vec<u64> = (1..=TOTAL).collect();
    assert_eq!(seen, expected);
    assert_quiet(&mut ws).await;
}

// 검증: C2, E3
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn 여럿이_발행하는_중에_연결해도_번호가_이어진다() {
    const WRITERS: u64 = 4;
    const EACH: u64 = 25;
    let h = start().await;
    let cookie = login(h.addr).await;
    let publisher = h.server.publisher();
    for i in 1..=10 {
        publisher.publish("warm", json!({ "i": i })).await.unwrap();
    }
    let mut writers = Vec::new();
    for w in 0..WRITERS {
        let publisher = h.server.publisher();
        writers.push(tokio::spawn(async move {
            for i in 0..EACH {
                publisher
                    .publish("busy", json!({ "w": w, "i": i }))
                    .await
                    .unwrap();
            }
        }));
    }
    // 다시 보내기(저장소에서 읽기)와 새 이벤트 받기가 겹치는 때에 연결한다.
    let mut early = ws_open(h.addr, &cookie, 0).await;
    tokio::task::yield_now().await;
    let mut late = ws_open(h.addr, &cookie, 7).await;

    let total = 10 + WRITERS * EACH;
    let all: Vec<u64> = (1..=total).collect();
    let from_eight: Vec<u64> = (8..=total).collect();
    assert_eq!(read_seqs(&mut early, all.len()).await, all);
    assert_eq!(read_seqs(&mut late, from_eight.len()).await, from_eight);
    for writer in writers {
        tokio::time::timeout(STEP, writer).await.unwrap().unwrap();
    }
    let stored: Vec<u64> = h
        .store
        .read_after(0, 1000)
        .unwrap()
        .iter()
        .map(|e| e.seq)
        .collect();
    assert_eq!(stored, all, "화면에 간 번호와 저장된 번호가 같다");
}

// 검증: E3, V6
#[tokio::test]
async fn 다시_켜도_번호가_이어지고_놓친_이벤트부터_받는다() {
    let h = start().await;
    let cookie = login(h.addr).await;
    for i in 1..=3 {
        h.clock.set(T0 + i * 10);
        let publisher = h.server.publisher();
        publisher.publish("step", json!({ "i": i })).await.unwrap();
    }
    let mut ws = ws_open(h.addr, &cookie, 0).await;
    assert_eq!(read_seqs(&mut ws, 2).await, [1, 2]);
    drop(ws);

    let Harness {
        dir,
        db,
        store,
        clock,
        server,
        hits,
        ..
    } = h;
    tokio::time::timeout(STEP, server.shutdown())
        .await
        .expect("시간 안에 꺼져야 한다")
        .expect("끄기가 성공해야 한다");
    drop(store);

    // 프로그램을 다시 켠다: 같은 파일을 새로 연다.
    let store = Store::open(&db).unwrap();
    let server = serve_on(&store, &clock, &hits, |c| c, |b| b).await;
    let addr = server.local_addr();
    clock.set(T0 + 999);
    let fourth = server
        .publisher()
        .publish("step", json!({ "i": 4 }))
        .await
        .unwrap();
    assert_eq!(fourth, 4, "다시 켜도 번호가 이어진다");

    let cookie = login(addr).await;
    let mut ws = ws_open(addr, &cookie, 2).await;
    let third = next_frame(&mut ws).await;
    assert_eq!(third["seq"], json!(3));
    assert_eq!(third["payload"], json!({ "i": 3 }));
    assert_eq!(
        third["at"],
        json!(T0 + 30),
        "꺼지기 전에 저장된 시각 그대로다"
    );
    let frame = next_frame(&mut ws).await;
    assert_eq!(frame["seq"], json!(4));
    assert_eq!(frame["at"], json!(T0 + 999));
    assert_quiet(&mut ws).await;
    drop(dir);
}

// 검증: C2, D11
#[tokio::test]
async fn 지나가는_이벤트는_번호가_없고_저장되지_않는다() {
    let h = start().await;
    let cookie = login(h.addr).await;
    let mut ws = ws_open(h.addr, &cookie, 0).await;
    let publisher = h.server.publisher();

    h.clock.set(T0 + 77);
    let quotes = json!({ "quotes": [{ "symbol": "005930", "price": "72000" }] });
    publisher.publish_transient("quotes", quotes.clone());
    let frame = next_frame(&mut ws).await;
    assert!(
        frame.get("seq").is_none(),
        "지나가는 이벤트에는 번호가 없다"
    );
    assert_eq!(frame["kind"], json!("quotes"));
    assert_eq!(frame["payload"], quotes);
    assert_eq!(frame["at"], json!(T0 + 77));

    assert!(
        h.store.read_after(0, 10).unwrap().is_empty(),
        "지나가는 이벤트는 저장하지 않는다"
    );
    let seq = publisher.publish("chat", json!({})).await.unwrap();
    assert_eq!(seq, 1, "지나가는 이벤트는 번호를 쓰지 않는다");
    assert_eq!(seq_of(&next_frame(&mut ws).await), Some(1));
}

// 검증: C2, D11
#[tokio::test]
async fn 연결하면_가장_최근_시세_묶음을_먼저_받는다() {
    let h = start().await;
    let cookie = login(h.addr).await;
    let publisher = h.server.publisher();
    publisher.publish("chat", json!({ "n": 1 })).await.unwrap();
    publisher.publish("chat", json!({ "n": 2 })).await.unwrap();
    publisher.publish_transient("quotes", json!({ "price": "72000" }));
    publisher.publish_transient("feed_status", json!({ "state": "live" }));
    publisher.publish_transient("quotes", json!({ "price": "72100" }));

    let mut ws = ws_open(h.addr, &cookie, 0).await;
    let mut latest = vec![next_frame(&mut ws).await, next_frame(&mut ws).await];
    latest.sort_by_key(|frame| frame["kind"].as_str().map(str::to_string));
    assert!(
        latest.iter().all(|frame| frame.get("seq").is_none()),
        "먼저 오는 것은 지나가는 이벤트다: {latest:?}"
    );
    assert_eq!(latest[0]["kind"], json!("feed_status"));
    assert_eq!(latest[0]["payload"], json!({ "state": "live" }));
    assert_eq!(latest[1]["kind"], json!("quotes"));
    assert_eq!(
        latest[1]["payload"],
        json!({ "price": "72100" }),
        "종류마다 가장 최근 것 하나만 온다"
    );
    assert_eq!(seq_of(&next_frame(&mut ws).await), Some(1));
    assert_eq!(seq_of(&next_frame(&mut ws).await), Some(2));
    assert_quiet(&mut ws).await;

    let mut caught_up = ws_open(h.addr, &cookie, 2).await;
    let kinds = [
        next_frame(&mut caught_up).await["kind"].clone(),
        next_frame(&mut caught_up).await["kind"].clone(),
    ];
    assert!(kinds.contains(&json!("quotes")) && kinds.contains(&json!("feed_status")));
    assert_quiet(&mut caught_up).await;
}

// 검증: C2, E3
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn 느린_연결은_그_연결만_끊기고_다시_이으면_이어_받는다() {
    const TOTAL: u64 = 300;
    let h = start_with(
        |c| GateConfig {
            send_buffer: 8,
            ..c
        },
        |b| b,
    )
    .await;
    let cookie = login(h.addr).await;
    // 느린 화면: 연결만 하고 읽지 않는다.
    let mut slow = ws_open(h.addr, &cookie, 0).await;
    let mut fast = ws_open(h.addr, &cookie, 0).await;
    let publisher = h.server.publisher();
    let blob = "가".repeat(40_000);

    // 빠른 화면이 받은 것을 확인하며 하나씩 발행한다. 느린 연결이 서버를 막으면 여기서 멈춘다.
    let work = async {
        for i in 1..=TOTAL {
            let seq = publisher
                .publish("big", json!({ "i": i, "blob": blob }))
                .await
                .unwrap();
            assert_eq!(seq, i);
            assert_eq!(
                read_seqs(&mut fast, 1).await,
                [i],
                "빠른 화면은 밀리지 않는다"
            );
        }
    };
    tokio::time::timeout(Duration::from_secs(60), work)
        .await
        .expect("느린 연결 하나가 발행과 다른 연결을 막으면 안 된다");

    // 느린 화면이 이제야 읽는다: 쌓여 있던 것까지만 오고 연결은 끊겨 있다.
    let mut got = Vec::new();
    loop {
        match next_text(&mut slow, STEP).await {
            Ok(Some(text)) => {
                let frame: Value = serde_json::from_str(&text).unwrap();
                got.extend(seq_of(&frame));
            }
            Ok(None) => break,
            Err(()) => panic!("밀린 연결은 끊겨야 한다(열린 채 멈춰 있다)"),
        }
    }
    let count = u64::try_from(got.len()).unwrap();
    assert!(count < TOTAL, "밀린 연결은 끝까지 받지 못하고 끊긴다");
    let prefix: Vec<u64> = (1..=count).collect();
    assert_eq!(got, prefix, "끊기기 전까지 받은 것도 순서대로다");

    let mut again = ws_open(h.addr, &cookie, count).await;
    let rest: Vec<u64> = (count + 1..=TOTAL).collect();
    assert_eq!(read_seqs(&mut again, rest.len()).await, rest);
}

// ───────────── 7. 이벤트 내보내기 ─────────────

// 검증: C2
#[tokio::test]
async fn 발행하면_저장되고_연결된_모든_화면에_간다() {
    let h = start().await;
    let cookie = login(h.addr).await;
    let mut first = ws_open(h.addr, &cookie, 0).await;
    let mut second = ws_open(h.addr, &cookie, 0).await;
    // 관문을 열기 전에 만든 손잡이와 복제한 손잡이도 같은 흐름에 쓴다.
    let publisher = h.server.publisher();
    let cloned = publisher.clone();

    let payload = json!({ "trade": 7, "state": "승인대기" });
    let a = publisher
        .publish("approval", payload.clone())
        .await
        .unwrap();
    let b = cloned
        .publish("chat", json!({ "text": "네" }))
        .await
        .unwrap();
    assert_eq!((a, b), (1, 2));

    let stored = h.store.read_after(0, 10).unwrap();
    assert_eq!(stored.len(), 2, "발행한 것은 저장소에 쌓인다");
    assert_eq!((stored[0].seq, stored[0].kind.as_str()), (1, "approval"));
    assert_eq!((stored[1].seq, stored[1].kind.as_str()), (2, "chat"));
    for ws in [&mut first, &mut second] {
        let frame = next_frame(ws).await;
        assert_eq!(frame["seq"], json!(1));
        assert_eq!(frame["kind"], json!("approval"));
        assert_eq!(frame["payload"], payload);
        assert_eq!(seq_of(&next_frame(ws).await), Some(2));
    }
}

// 검증: C2
#[tokio::test]
async fn 주소를_열기_전에_발행한_것도_저장되어_나중에_받는다() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("gate.sqlite")).unwrap();
    let gate = Gate::builder(store.clone(), GateConfig::local(0)).build();
    let publisher = gate.publisher();
    let seq = publisher
        .publish("boot", json!({ "ok": true }))
        .await
        .unwrap();
    assert_eq!(seq, 1);
    assert_eq!(store.read_after(0, 10).unwrap().len(), 1);

    let server = tokio::time::timeout(STEP, gate.serve())
        .await
        .unwrap()
        .unwrap();
    let addr = server.local_addr();
    let cookie = login(addr).await;
    let mut ws = ws_open(addr, &cookie, 0).await;
    let frame = next_frame(&mut ws).await;
    assert_eq!(frame["seq"], json!(1));
    assert_eq!(frame["kind"], json!("boot"));
    let next = publisher.publish("boot", json!({})).await.unwrap();
    assert_eq!(seq_of(&next_frame(&mut ws).await), Some(next));
}

// ───────────── 8. 가리기 자리 ─────────────

/// 정해 둔 글자를 가리는 가짜 가리기 함수.
struct MaskWord(&'static str);

impl Redactor for MaskWord {
    fn redact(&self, text: &str) -> String {
        text.replace(self.0, "[가려짐]")
    }
}

// 검증: C2
#[tokio::test]
async fn 화면으로_나가는_글은_가리기_자리를_거친다() {
    const WORD: &str = "sesame-opens-nothing-0000";
    let h = start_with(
        |c| c,
        |b| b.redactor(Arc::new(MaskWord(WORD)) as Arc<dyn Redactor>),
    )
    .await;
    let cookie = login(h.addr).await;
    let publisher = h.server.publisher();
    let leaky = json!({ "text": format!("오류: key={WORD} 로 실패"), "deep": [{ "v": WORD }] });

    let mut live = ws_open(h.addr, &cookie, 0).await;
    publisher.publish("error", leaky.clone()).await.unwrap();
    publisher.publish_transient("status", leaky.clone());
    let mut texts = vec![
        next_text(&mut live, STEP).await.unwrap().unwrap(),
        next_text(&mut live, STEP).await.unwrap().unwrap(),
    ];
    // 다시 보내기(저장된 것)와 연결할 때 먼저 오는 지나가는 이벤트도 같다.
    let mut replay = ws_open(h.addr, &cookie, 0).await;
    texts.push(next_text(&mut replay, STEP).await.unwrap().unwrap());
    texts.push(next_text(&mut replay, STEP).await.unwrap().unwrap());

    for text in &texts {
        assert!(!text.contains(WORD), "가려야 할 글자가 그대로 나갔다");
        assert!(text.contains("[가려짐]"), "가린 자리가 보여야 한다: {text}");
    }
}

// ───────────── 끄기 ─────────────

// 검증: E3
#[tokio::test]
async fn 끄면_열린_연결이_닫히고_주소가_닫힌다() {
    let h = start().await;
    let cookie = login(h.addr).await;
    let mut ws = ws_open(h.addr, &cookie, 0).await;
    let publisher = h.server.publisher();
    publisher.publish("last", json!({})).await.unwrap();
    assert_eq!(read_seqs(&mut ws, 1).await, [1]);

    let addr = h.addr;
    tokio::time::timeout(STEP, h.server.shutdown())
        .await
        .expect("열린 연결이 있어도 시간 안에 꺼져야 한다")
        .expect("끄기가 성공해야 한다");

    let end = next_text(&mut ws, STEP).await;
    assert_eq!(end, Ok(None), "끄면 화면의 연결이 닫힌다");
    assert!(
        TcpStream::connect(addr).await.is_err(),
        "꺼진 뒤에는 주소가 닫혀 있다"
    );
    assert_eq!(
        h.store.read_after(0, 10).unwrap().len(),
        1,
        "저장소는 그대로다"
    );
}

// ───────────── 2차: 가린 뒤에도 JSON · 저장 전 가리기 · 크기 상한 ─────────────

/// 숫자뿐인 시험용 값(8자). 진짜 키나 계좌가 아니다.
const DIGITS: &str = "12345678";
/// [`DIGITS`]와 같은 값의 JSON 숫자.
const DIGITS_NUMBER: u64 = 12_345_678;
/// 시각을 적은 숫자 안에 [`DIGITS`]가 들어 있는 시각.
const DIGITS_CLOCK: i64 = 1_812_345_678_000;
/// 가려진 자리에 들어가는 글자.
const MASKED: &str = "[가려짐]";

/// 가릴 글자가 나중에 늘어나는 가짜 가리기 함수.
#[derive(Default)]
struct GrowingMask(std::sync::Mutex<Vec<String>>);

impl GrowingMask {
    fn of(words: &[&str]) -> Arc<Self> {
        let mask = Arc::new(Self::default());
        for word in words {
            mask.add(word);
        }
        mask
    }

    fn add(&self, word: &str) {
        self.0.lock().unwrap().push(word.to_string());
    }
}

impl Redactor for GrowingMask {
    fn redact(&self, text: &str) -> String {
        let words = self.0.lock().unwrap();
        words
            .iter()
            .fold(text.to_string(), |acc, word| acc.replace(word, MASKED))
    }
}

/// 저장소에 쌓인 이벤트 전부를 글자로 적은 것(종류와 내용).
fn stored_text(store: &Store) -> String {
    store
        .read_after(0, 100)
        .unwrap()
        .into_iter()
        .map(|event| format!("{} {}\n", event.kind, event.payload))
        .collect()
}

/// 글자 프레임을 JSON으로 읽는다. 읽히지 않으면 테스트가 실패한다.
fn parse_frame(text: &str) -> Value {
    serde_json::from_str(text)
        .unwrap_or_else(|error| panic!("가린 뒤에도 프레임은 JSON이어야 한다({error}): {text}"))
}

// 검증: N8, E38
#[tokio::test]
async fn 숫자뿐인_값이_json_숫자와_겹쳐도_가린_프레임은_json이다() {
    let h = start_with(
        |c| c,
        |b| b.redactor(GrowingMask::of(&[DIGITS]) as Arc<dyn Redactor>),
    )
    .await;
    let cookie = login(h.addr).await;
    let publisher = h.server.publisher();
    let leaky = json!({
        "account": DIGITS_NUMBER,
        "inside": 9_912_345_678_u64,
        "list": [DIGITS_NUMBER, { "deep": DIGITS_NUMBER }],
        "text": format!("id={DIGITS}"),
        "other": 42,
    });

    let mut live = ws_open(h.addr, &cookie, 0).await;
    let seq = publisher.publish("error", leaky.clone()).await.unwrap();
    publisher.publish_transient("status", leaky.clone());
    let mut texts = vec![
        next_text(&mut live, STEP).await.unwrap().unwrap(),
        next_text(&mut live, STEP).await.unwrap().unwrap(),
    ];
    // 다시 보내기(저장된 것)와 연결할 때 먼저 오는 지나가는 이벤트도 같다.
    let mut replay = ws_open(h.addr, &cookie, 0).await;
    texts.push(next_text(&mut replay, STEP).await.unwrap().unwrap());
    texts.push(next_text(&mut replay, STEP).await.unwrap().unwrap());

    let mut stored_frames = 0;
    for text in &texts {
        assert!(!text.contains(DIGITS), "가려야 할 숫자가 나갔다: {text}");
        let frame = parse_frame(text);
        assert!(text.contains(MASKED), "가린 자리가 보여야 한다: {text}");
        let payload = &frame["payload"];
        assert_ne!(payload["account"], json!(DIGITS_NUMBER));
        assert_eq!(payload["other"], json!(42), "겹치지 않는 숫자는 그대로다");
        assert_eq!(payload["text"], json!(format!("id={MASKED}")));
        assert_eq!(payload["list"].as_array().map(Vec::len), Some(2));
        assert_eq!(frame["at"], json!(T0), "프레임의 뼈대는 그대로다");
        if let Some(got) = seq_of(&frame) {
            assert_eq!(got, seq);
            assert_eq!(frame["kind"], json!("error"));
            stored_frames += 1;
        } else {
            assert_eq!(frame["kind"], json!("status"));
        }
    }
    assert_eq!(
        stored_frames, 2,
        "저장된 이벤트는 바로 한 번, 다시 보내기로 한 번 온다"
    );
}

// 검증: N8, E38
#[tokio::test]
async fn 가릴_숫자가_시각과_겹쳐도_프레임의_뼈대는_깨지지_않는다() {
    let h = start_with(
        |c| c,
        |b| b.redactor(GrowingMask::of(&[DIGITS]) as Arc<dyn Redactor>),
    )
    .await;
    // 시각을 적은 숫자 안에 가릴 숫자가 들어 있다.
    h.clock.set(DIGITS_CLOCK);
    let cookie = login(h.addr).await;
    let publisher = h.server.publisher();
    let mut live = ws_open(h.addr, &cookie, 0).await;
    let seq = publisher
        .publish("note", json!({ "account": DIGITS_NUMBER }))
        .await
        .unwrap();
    publisher.publish_transient("status", json!({ "account": DIGITS_NUMBER }));

    let stored = parse_frame(&next_text(&mut live, STEP).await.unwrap().unwrap());
    assert_eq!(seq_of(&stored), Some(seq), "번호 자리는 숫자로 남는다");
    assert_eq!(stored["kind"], json!("note"));
    assert!(!stored["payload"].to_string().contains(DIGITS));
    let passing = parse_frame(&next_text(&mut live, STEP).await.unwrap().unwrap());
    assert_eq!(passing["kind"], json!("status"));
    assert!(passing.get("seq").is_none());
    assert!(!passing["payload"].to_string().contains(DIGITS));

    // 다시 보내도 같다.
    let mut replay = ws_open(h.addr, &cookie, 0).await;
    let first = parse_frame(&next_text(&mut replay, STEP).await.unwrap().unwrap());
    let second = parse_frame(&next_text(&mut replay, STEP).await.unwrap().unwrap());
    for frame in [first, second] {
        assert!(!frame["payload"].to_string().contains(DIGITS));
    }
}

// 검증: N8, E38
#[tokio::test]
async fn 발행한_내용은_저장소에_쓰기_전에_가린다() {
    const WORD: &str = "sesame-opens-nothing-0001";
    let h = start_with(
        |c| c,
        |b| b.redactor(GrowingMask::of(&[WORD, DIGITS]) as Arc<dyn Redactor>),
    )
    .await;
    let publisher = h.server.publisher();
    let mut leaky = json!({
        "text": format!("오류: key={WORD} 로 실패"),
        "deep": [{ "v": WORD }],
        "account": DIGITS_NUMBER,
    });
    leaky[WORD] = json!("이름 자리에 든 값");
    let seq = publisher.publish("error", leaky).await.unwrap();
    publisher
        .publish("note", json!({ "text": "가릴 것이 없는 글" }))
        .await
        .unwrap();

    let stored = stored_text(&h.store);
    assert!(!stored.contains(WORD), "저장소에 값이 그대로 쓰였다");
    assert!(!stored.contains(DIGITS), "저장소에 숫자 값이 그대로 쓰였다");
    assert!(stored.contains(MASKED), "가린 자리가 저장돼 있어야 한다");
    assert!(
        stored.contains("가릴 것이 없는 글"),
        "가릴 것이 없는 내용은 그대로 저장된다"
    );

    // 저장된 것을 다시 보내도 가려져 있고 번호 · 시각은 그대로다.
    let cookie = login(h.addr).await;
    let mut replay = ws_open(h.addr, &cookie, 0).await;
    let text = next_text(&mut replay, STEP).await.unwrap().unwrap();
    assert!(!text.contains(WORD) && !text.contains(DIGITS));
    let frame = parse_frame(&text);
    assert_eq!(seq_of(&frame), Some(seq));
    assert_eq!(frame["at"], json!(T0));
    assert_eq!(
        frame["payload"]["text"],
        json!(format!("오류: key={MASKED} 로 실패"))
    );
}

// 검증: N8, E38
#[tokio::test]
async fn 가리기_대상이_늘면_저장된_것을_다시_보낼_때도_가린다() {
    const EARLY: &str = "sesame-opens-nothing-0002";
    const LATE: &str = "sesame-registered-later-0003";
    let mask = GrowingMask::of(&[EARLY]);
    let h = start_with(
        |c| c,
        |b| b.redactor(Arc::clone(&mask) as Arc<dyn Redactor>),
    )
    .await;
    let cookie = login(h.addr).await;
    let publisher = h.server.publisher();
    // 발행할 때는 LATE가 아직 가리기 대상이 아니다.
    let seq = publisher
        .publish("note", json!({ "text": format!("{EARLY} / {LATE}") }))
        .await
        .unwrap();
    publisher.publish_transient("status", json!({ "text": LATE }));

    mask.add(LATE);
    let mut replay = ws_open(h.addr, &cookie, 0).await;
    let mut seen_stored = false;
    for _ in 0..2 {
        let text = next_text(&mut replay, STEP).await.unwrap().unwrap();
        let frame = parse_frame(&text);
        if seq_of(&frame) == Some(seq) {
            seen_stored = true;
            assert!(!text.contains(EARLY), "처음부터 가리던 값이 나갔다");
            assert!(
                !text.contains(LATE),
                "나중에 등록된 값도 다시 보낼 때 가려야 한다: {text}"
            );
            assert_eq!(
                frame["payload"]["text"],
                json!(format!("{MASKED} / {MASKED}"))
            );
        }
    }
    assert!(seen_stored, "저장된 이벤트가 다시 와야 한다");
}

// 검증: N8, E38
#[tokio::test]
async fn 나중에_등록된_숫자_값도_다시_보낼_때_가려지고_json이다() {
    let mask = GrowingMask::of(&[]);
    let h = start_with(
        |c| c,
        |b| b.redactor(Arc::clone(&mask) as Arc<dyn Redactor>),
    )
    .await;
    let cookie = login(h.addr).await;
    let seq = h
        .server
        .publisher()
        .publish("note", json!({ "account": DIGITS_NUMBER, "other": 7 }))
        .await
        .unwrap();

    mask.add(DIGITS);
    let mut replay = ws_open(h.addr, &cookie, 0).await;
    let text = next_text(&mut replay, STEP).await.unwrap().unwrap();
    assert!(!text.contains(DIGITS), "숫자 값이 나갔다: {text}");
    let frame = parse_frame(&text);
    assert_eq!(seq_of(&frame), Some(seq));
    assert_eq!(frame["payload"]["other"], json!(7));
    assert_eq!(frame["at"], json!(T0));
}

// 검증: C2
#[tokio::test]
async fn 상한을_넘는_내용은_발행을_거부하고_번호를_쓰지_않는다() {
    const LIMIT: usize = 1024;
    let h = start_with(
        |c| GateConfig {
            max_frame_bytes: LIMIT,
            ..c
        },
        |b| b,
    )
    .await;
    let cookie = login(h.addr).await;
    let publisher = h.server.publisher();
    let mut live = ws_open(h.addr, &cookie, 0).await;

    let first = publisher.publish("note", json!({ "n": 1 })).await.unwrap();
    let huge = json!({ "text": "가".repeat(LIMIT * 4) });
    let result = publisher.publish("note", huge).await;
    match &result {
        Err(GateError::FrameTooLarge { limit }) => assert_eq!(*limit, LIMIT),
        other => panic!("상한을 넘으면 FrameTooLarge여야 한다: {other:?}"),
    }
    let shown = result.map(|_| ()).unwrap_err().to_string();
    assert!(!shown.contains("가가가"), "오류에 내용을 싣지 않는다");

    let second = publisher.publish("note", json!({ "n": 2 })).await.unwrap();
    assert_eq!((first, second), (1, 2), "거부된 발행은 번호를 쓰지 않는다");
    let stored = h.store.read_after(0, 10).unwrap();
    assert_eq!(
        stored.iter().map(|event| event.seq).collect::<Vec<_>>(),
        [1, 2],
        "거부된 내용은 저장되지 않는다"
    );
    assert_eq!(read_seqs(&mut live, 2).await, [1, 2]);
    assert_quiet(&mut live).await;
}

// 검증: C2
#[tokio::test]
async fn 상한을_넘는_지나가는_이벤트는_보내지_않는다() {
    const LIMIT: usize = 1024;
    let h = start_with(
        |c| GateConfig {
            max_frame_bytes: LIMIT,
            ..c
        },
        |b| b,
    )
    .await;
    let cookie = login(h.addr).await;
    let publisher = h.server.publisher();
    let mut live = ws_open(h.addr, &cookie, 0).await;

    publisher.publish_transient("quotes", json!({ "text": "가".repeat(LIMIT * 4) }));
    publisher.publish_transient("quotes", json!({ "n": 1 }));
    let frame = next_frame(&mut live).await;
    assert_eq!(
        frame["payload"],
        json!({ "n": 1 }),
        "상한을 넘는 것은 건너뛰고 다음 것이 온다"
    );
    assert_quiet(&mut live).await;

    // 넘는 것은 "가장 최근 것"으로 기억하지도 않는다.
    publisher.publish_transient("status", json!({ "text": "나".repeat(LIMIT * 4) }));
    let mut late = ws_open(h.addr, &cookie, 0).await;
    let first = next_frame(&mut late).await;
    assert_eq!(first["kind"], json!("quotes"));
    assert_eq!(first["payload"], json!({ "n": 1 }));
    assert_quiet(&mut late).await;
}
