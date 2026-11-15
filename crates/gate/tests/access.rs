//! pi-gate 9차 테스트: 접속 키 · 잠금 · 접속 기록 (T17).
//!
//! 근거: `docs/dev/modules/gate.md` "9차 작업" 1~8, `dev.md` C1 · M6 · E1 · E2 · N3 · V37~V41,
//! 원본 근거 `docs/research/triggers-approval-memory-deep.md` 5.1~5.4,
//! `docs/research/remote-access-security.md` 4.5 · 4.6, 결정 2026-11-16 8 · 9 · 10번.
//!
//! 루프백이 아닌 상대는 `GateBuilder::peer_override`로 흉내 낸다. 테스트가 그 값을 도중에 바꾼다.
//! 시험용 접속 키는 프로그램이 만든 것만 쓰고, 틀린 키는 진짜 키처럼 보이지 않는 글자를 쓴다.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::atomic::{AtomicI64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::Router;
use axum::routing::{get, post};
use futures::StreamExt;
use pi_gate::{
    ACCESS_ENTER, ACCESS_KEY_ALPHABET, ACCESS_KEY_CHANGED, ACCESS_KEY_FIELD, ACCESS_KEY_LEN,
    ACCESS_KEY_WRONG, ACCESS_LOCKED, Clock, Gate, GateBuilder, GateConfig, GateServer,
    LAST_SEQ_PARAM, LOCKED_STATUS, LOCKOUT, LockAlert, LockNotice, MAX_KEY_FAILURES,
    REMOTE_SESSION_MAX_AGE, SECURITY_HEADERS, SESSION_COOKIE, SESSION_PATH, WS_PATH,
};
use pi_store::{AccessRecord, Store, UnixMillis};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tempfile::TempDir;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

// ───────────── 도우미 ─────────────

const STEP: Duration = Duration::from_secs(10);
const T0: i64 = 1_800_000_000_000;
const PUBLIC_PATH: &str = "/";
const PING_PATH: &str = "/api/ping";
/// 루프백이 아닌 상대들(문서용 주소 대역).
const OUTSIDE_A: IpAddr = IpAddr::V4(Ipv4Addr::new(203, 0, 113, 9));
const OUTSIDE_B: IpAddr = IpAddr::V4(Ipv4Addr::new(198, 51, 100, 7));
const LOOPBACK: IpAddr = IpAddr::V4(Ipv4Addr::LOCALHOST);
/// 틀린 키(진짜 키처럼 보이지 않는 글자).
const WRONG_KEY: &str = "TEST-WRONG-KEY-0000";

type Ws = WebSocketStream<MaybeTlsStream<TcpStream>>;

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

/// 받은 잠금 알림을 모아 둔다.
#[derive(Default)]
struct Alerts(Mutex<Vec<LockNotice>>);

impl Alerts {
    fn all(&self) -> Vec<LockNotice> {
        self.0.lock().unwrap().clone()
    }
}

impl LockAlert for Alerts {
    fn locked(&self, notice: LockNotice) {
        self.0.lock().unwrap().push(notice);
    }
}

struct Harness {
    dir: TempDir,
    store: Store,
    clock: Arc<TestClock>,
    peer: Arc<Mutex<IpAddr>>,
    alerts: Arc<Alerts>,
    hits: Arc<AtomicUsize>,
    server: GateServer,
    addr: SocketAddr,
}

impl Harness {
    /// 다음 요청부터 상대 주소를 바꾼다.
    fn as_peer(&self, ip: IpAddr) {
        *self.peer.lock().unwrap() = ip;
    }

    fn set_time(&self, millis: i64) {
        self.clock.set(millis);
    }

    fn log(&self) -> Vec<AccessRecord> {
        self.store.access_log().unwrap()
    }

    fn actions(&self, action: &str) -> Vec<AccessRecord> {
        self.log()
            .into_iter()
            .filter(|record| record.action == action)
            .collect()
    }
}

fn routes(hits: &Arc<AtomicUsize>) -> (Router, Router) {
    let h1 = Arc::clone(hits);
    let public = Router::new().route(
        PUBLIC_PATH,
        get(move || {
            let h = Arc::clone(&h1);
            async move {
                h.fetch_add(1, Ordering::SeqCst);
                "공개 화면"
            }
        }),
    );
    let h2 = Arc::clone(hits);
    let protected = Router::new().route(
        PING_PATH,
        get(move || {
            let h = Arc::clone(&h2);
            async move {
                h.fetch_add(1, Ordering::SeqCst);
                "pong"
            }
        }),
    );
    (
        public,
        protected.route("/api/poke", post(|| async { "poked" })),
    )
}

async fn serve(
    store: &Store,
    clock: &Arc<TestClock>,
    peer: &Arc<Mutex<IpAddr>>,
    alerts: &Arc<Alerts>,
    hits: &Arc<AtomicUsize>,
    config: impl FnOnce(GateConfig) -> GateConfig,
) -> GateServer {
    let (public, protected) = routes(hits);
    let seen = Arc::clone(peer);
    let builder: GateBuilder = Gate::builder(store.clone(), config(GateConfig::local(0)))
        .clock(Arc::clone(clock) as Arc<dyn Clock>)
        .public_routes(public)
        .protected_routes(protected)
        .lock_alert(Arc::clone(alerts) as Arc<dyn LockAlert>)
        .peer_override(Arc::new(move |actual: SocketAddr| {
            SocketAddr::new(*seen.lock().unwrap(), actual.port())
        }));
    tokio::time::timeout(STEP, builder.build().serve())
        .await
        .expect("서버가 시간 안에 떠야 한다")
        .expect("서버가 떠야 한다")
}

async fn start_with(config: impl FnOnce(GateConfig) -> GateConfig) -> Harness {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("gate.sqlite")).unwrap();
    let clock = Arc::new(TestClock(AtomicI64::new(T0)));
    let peer = Arc::new(Mutex::new(OUTSIDE_A));
    let alerts = Arc::new(Alerts::default());
    let hits = Arc::new(AtomicUsize::new(0));
    let server = serve(&store, &clock, &peer, &alerts, &hits, config).await;
    let addr = server.local_addr();
    Harness {
        dir,
        store,
        clock,
        peer,
        alerts,
        hits,
        server,
        addr,
    }
}

async fn start() -> Harness {
    start_with(|c| c).await
}

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

    /// `Cookie` 헤더에 넣을 `pi_session=<값>`.
    fn cookie(&self) -> Option<String> {
        let raw = self.header("set-cookie")?;
        let first = raw.split(';').next()?.trim();
        first
            .starts_with(&format!("{SESSION_COOKIE}="))
            .then(|| first.to_string())
    }

    /// 쿠키 속성들(소문자, 이름=값 그대로).
    fn cookie_attrs(&self) -> Vec<String> {
        self.header("set-cookie")
            .map(|raw| {
                raw.split(';')
                    .skip(1)
                    .map(|part| part.trim().to_ascii_lowercase())
                    .collect()
            })
            .unwrap_or_default()
    }

    fn text(&self) -> String {
        let heads: Vec<String> = self
            .headers
            .iter()
            .map(|(k, v)| format!("{k}: {v}"))
            .collect();
        format!("{}\n{}", heads.join("\n"), self.body)
    }
}

fn host_of(addr: SocketAddr) -> String {
    format!("127.0.0.1:{}", addr.port())
}

fn origin_of(addr: SocketAddr) -> String {
    format!("http://{}", host_of(addr))
}

/// HTTP/1.1 요청 하나를 날것으로 보낸다.
async fn http(
    addr: SocketAddr,
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
    body: &str,
) -> Reply {
    let work = async {
        let mut stream = TcpStream::connect(addr).await.unwrap();
        let mut request = format!("{method} {path} HTTP/1.1\r\n");
        for (name, value) in headers {
            request.push_str(&format!("{name}: {value}\r\n"));
        }
        request.push_str(&format!(
            "Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        ));
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

/// 세션을 달라고 한다. `key`가 있으면 JSON 본문에 싣는다. `extra`는 더 붙일 헤더.
async fn enter_with(addr: SocketAddr, key: Option<&str>, extra: &[(&str, &str)]) -> Reply {
    let (host, origin) = (host_of(addr), origin_of(addr));
    let mut headers = vec![("Host", host.as_str()), ("Origin", origin.as_str())];
    headers.extend_from_slice(extra);
    let body = match key {
        Some(key) => {
            headers.push(("Content-Type", "application/json"));
            json!({ ACCESS_KEY_FIELD: key }).to_string()
        }
        None => String::new(),
    };
    http(addr, "POST", SESSION_PATH, &headers, &body).await
}

async fn enter(addr: SocketAddr, key: Option<&str>) -> Reply {
    enter_with(addr, key, &[]).await
}

/// 맞는 키로 들어오고 쿠키를 돌려준다.
async fn login(addr: SocketAddr, key: &str) -> String {
    let reply = enter(addr, Some(key)).await;
    assert_eq!(reply.status, 204, "맞는 키면 들어와야 한다");
    reply.cookie().expect("세션 쿠키가 있어야 한다")
}

async fn get_with(addr: SocketAddr, path: &str, cookie: Option<&str>) -> Reply {
    let host = host_of(addr);
    let mut headers = vec![("Host", host.as_str())];
    if let Some(cookie) = cookie {
        headers.push(("Cookie", cookie));
    }
    http(addr, "GET", path, &headers, "").await
}

async fn ws_try(addr: SocketAddr, cookie: Option<&str>, last_seq: u64) -> Result<Ws, u16> {
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
    request
        .headers_mut()
        .insert("Origin", origin_of(addr).parse().unwrap());
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

/// 다음 글자 프레임. 닫혔으면 `None`. 시간 안에 아무 일도 없으면 `Err`.
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

async fn next_frame(ws: &mut Ws) -> Value {
    let text = next_text(ws, STEP)
        .await
        .expect("프레임이 시간 안에 와야 한다")
        .expect("연결이 열려 있어야 한다");
    serde_json::from_str(&text).unwrap()
}

/// 키를 `n`번 틀린다(각 응답은 거부여야 한다).
async fn fail_times(addr: SocketAddr, n: u32, extra: &[(&str, &str)]) {
    for i in 0..n {
        let reply = enter_with(addr, Some(WRONG_KEY), extra).await;
        assert!(
            reply.status == 401 || reply.status == LOCKED_STATUS,
            "{i}번째 틀린 키는 거부된다: {}",
            reply.status
        );
        assert!(reply.cookie().is_none(), "틀린 키로 세션을 주지 않는다");
    }
}

/// 알림이 `n`개가 될 때까지 잠깐 기다린다.
async fn wait_alerts(h: &Harness, n: usize) -> Vec<LockNotice> {
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        let all = h.alerts.all();
        if all.len() >= n || Instant::now() > deadline {
            return all;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

fn millis(duration: Duration) -> i64 {
    i64::try_from(duration.as_millis()).unwrap()
}

// ───────────── 1. 루프백 밖은 접속 키로 (C1 · V37 · V38) ─────────────

// 검증: V37, C1, E1
#[tokio::test]
async fn 키를_넣지_않은_다른_기기에서는_화면도_세션도_실시간_연결도_열리지_않는다() {
    let h = start().await;
    h.server.access_keys().issue().await.unwrap();

    let page = get_with(h.addr, PUBLIC_PATH, None).await;
    assert_eq!(page.status, 401, "화면이 열리지 않는다");
    let entered = enter(h.addr, None).await;
    assert_eq!(entered.status, 401, "키가 없으면 세션이 없다");
    assert!(entered.cookie().is_none());
    assert_eq!(get_with(h.addr, PING_PATH, None).await.status, 401);
    assert_eq!(ws_try(h.addr, None, 0).await.err(), Some(401));
    assert_eq!(h.hits.load(Ordering::SeqCst), 0, "핸들러가 불리지 않는다");
    assert!(h.actions(ACCESS_ENTER).is_empty());
}

// 검증: V38, C1, M6, C2
#[tokio::test]
async fn 맞는_키를_넣으면_다른_기기가_들어오고_30일_쿠키를_받는다() {
    let h = start().await;
    let issued = h.server.access_keys().issue().await.unwrap();
    assert_eq!(issued.generation, 1, "처음 둔 키는 1세대다");

    let reply = enter(h.addr, Some(&issued.key)).await;
    assert_eq!(reply.status, 204);
    let cookie = reply.cookie().expect("세션 쿠키");
    let attrs = reply.cookie_attrs();
    for want in ["httponly", "samesite=strict", "path=/"] {
        assert!(attrs.iter().any(|a| a == want), "{want} 속성: {attrs:?}");
    }
    let max_age = format!("max-age={}", REMOTE_SESSION_MAX_AGE.as_secs());
    assert_eq!(REMOTE_SESSION_MAX_AGE.as_secs(), 30 * 24 * 60 * 60);
    assert!(attrs.contains(&max_age), "원격 세션은 30일: {attrs:?}");
    assert!(
        !reply.text().contains(&issued.key),
        "응답에 키 글자를 싣지 않는다"
    );

    // 그 세션으로 화면 · 보호된 경로 · 실시간 연결이 열린다.
    assert_eq!(
        get_with(h.addr, PUBLIC_PATH, Some(&cookie)).await.status,
        200
    );
    let ping = get_with(h.addr, PING_PATH, Some(&cookie)).await;
    assert_eq!(ping.status, 200);
    assert_eq!(ping.body, "pong");
    let mut ws = ws_try(h.addr, Some(&cookie), 0)
        .await
        .expect("원격 세션으로 실시간 연결이 열린다");
    let seq = h
        .server
        .publisher()
        .publish("chat", json!({ "n": 1 }))
        .await
        .unwrap();
    assert_eq!(next_frame(&mut ws).await["seq"], json!(seq));
}

// 검증: E1, V38, M6
#[tokio::test]
async fn 틀린_키는_401이고_세션이_없고_틀림이_기록된다() {
    let h = start().await;
    // 아직 둔 키가 없을 때의 거부.
    let none_set = enter(h.addr, Some(WRONG_KEY)).await;
    assert_eq!(none_set.status, 401, "둔 키가 없으면 어떤 키도 맞지 않는다");
    assert!(none_set.cookie().is_none());

    let issued = h.server.access_keys().issue().await.unwrap();
    h.set_time(T0 + 777);
    let wrong = enter(h.addr, Some(WRONG_KEY)).await;
    assert_eq!(wrong.status, 401);
    assert!(wrong.cookie().is_none());
    let missing = enter(h.addr, None).await;
    assert_eq!(missing.status, 401);
    assert_eq!(
        wrong.body, none_set.body,
        "어느 경우인지 알려 주지 않는다(같은 본문)"
    );
    assert_eq!(wrong.body, missing.body);
    for reply in [&wrong, &none_set, &missing] {
        assert!(
            !reply.text().contains(WRONG_KEY),
            "넣은 키를 되돌려 싣지 않는다"
        );
        assert!(!reply.text().contains(&issued.key));
    }

    let wrongs = h.actions(ACCESS_KEY_WRONG);
    assert!(!wrongs.is_empty(), "틀림이 기록된다");
    let last = wrongs.last().unwrap();
    assert_eq!(last.origin, "203.0.113.9", "어디서는 상대 IP");
    assert_eq!(last.at, UnixMillis(T0 + 777), "시각은 관문의 시계");
    for record in h.log() {
        assert!(!record.origin.contains(WRONG_KEY) && !record.action.contains(WRONG_KEY));
        assert!(!record.origin.contains(&issued.key) && !record.action.contains(&issued.key));
    }
    assert!(h.actions(ACCESS_ENTER).is_empty());
}

// 검증: V37, E1, C1
#[tokio::test]
async fn 키를_둔_적이_없으면_다른_기기는_들어오지_못한다() {
    let h = start().await;
    for key in [None, Some(""), Some(WRONG_KEY)] {
        let reply = enter(h.addr, key).await;
        assert_eq!(reply.status, 401, "{key:?}");
        assert!(reply.cookie().is_none());
    }
    assert!(h.store.access_key().unwrap().is_none());
}

// 검증: C1, M6, V38
#[tokio::test]
async fn 접속_키는_100비트_글자로_만들고_소금_친_해시만_저장한다() {
    let h = start().await;
    assert_eq!(ACCESS_KEY_LEN, 20);
    assert_eq!(ACCESS_KEY_ALPHABET.len(), 32, "글자마다 5비트 → 100비트");
    let keys = h.server.access_keys();
    let mut seen = Vec::new();
    let mut salts = Vec::new();
    for round in 1..=6_u64 {
        let issued = keys.issue().await.unwrap();
        assert_eq!(issued.generation, round, "바꿀 때마다 세대가 1씩 오른다");
        assert_eq!(issued.key.chars().count(), ACCESS_KEY_LEN);
        assert!(
            issued.key.chars().all(|c| ACCESS_KEY_ALPHABET.contains(c)),
            "정한 글자만 쓴다"
        );
        assert!(!seen.contains(&issued.key), "매번 다른 키다");

        let record = h.store.access_key().unwrap().expect("해시가 저장된다");
        assert_eq!(record.generation, round);
        assert!(record.salt.len() >= pi_gate::ACCESS_KEY_SALT_BYTES);
        let mut hasher = Sha256::new();
        hasher.update(&record.salt);
        hasher.update(issued.key.as_bytes());
        assert_eq!(
            hasher.finalize().as_slice(),
            record.sha256.as_slice(),
            "SHA-256(소금 ‖ 키)"
        );
        assert!(!salts.contains(&record.salt), "소금은 키마다 새로 만든다");
        salts.push(record.salt);
        seen.push(issued.key);
    }

    // 키 원문은 저장소 파일 어디에도 없다.
    let mut bytes = Vec::new();
    for entry in std::fs::read_dir(h.dir.path()).unwrap() {
        bytes.extend(std::fs::read(entry.unwrap().path()).unwrap());
    }
    let haystack = String::from_utf8_lossy(&bytes);
    for key in &seen {
        assert!(!haystack.contains(key.as_str()), "원문을 저장하지 않는다");
    }
    for record in h.log() {
        for key in &seen {
            assert!(!record.origin.contains(key.as_str()));
        }
    }
    assert_eq!(
        h.actions(ACCESS_KEY_CHANGED).len(),
        6,
        "키를 바꾼 것이 기록된다"
    );
}

// 검증: M6, C1
#[tokio::test]
async fn 원격_세션은_30일이_지나면_끝난다() {
    let h = start().await;
    let issued = h.server.access_keys().issue().await.unwrap();
    let cookie = login(h.addr, &issued.key).await;
    let life = millis(REMOTE_SESSION_MAX_AGE);

    h.set_time(T0 + life - 1_000);
    assert_eq!(
        get_with(h.addr, PING_PATH, Some(&cookie)).await.status,
        200,
        "30일 안에는 살아 있다"
    );
    h.set_time(T0 + life + 1_000);
    assert_eq!(
        get_with(h.addr, PING_PATH, Some(&cookie)).await.status,
        401,
        "30일이 지나면 끝난다"
    );
    assert_eq!(ws_try(h.addr, Some(&cookie), 0).await.err(), Some(401));
}

// ───────────── 1. 전달 헤더 (프록시를 거친 요청) ─────────────

// 검증: C1, V37, V38, C15
#[tokio::test]
async fn 전달_헤더가_하나라도_있는_루프백_요청은_키가_있어야_한다() {
    let h = start().await;
    let issued = h.server.access_keys().issue().await.unwrap();
    h.as_peer(LOOPBACK);
    for (name, value) in [
        ("X-Forwarded-For", "127.0.0.1"),
        ("Forwarded", "for=127.0.0.1"),
        ("X-Real-IP", "127.0.0.1"),
        ("x-forwarded-for", "203.0.113.9"),
    ] {
        let reply = enter_with(h.addr, None, &[(name, value)]).await;
        assert_eq!(reply.status, 401, "{name}가 있으면 루프백으로 치지 않는다");
        assert!(reply.cookie().is_none());
        let page = http(
            h.addr,
            "GET",
            PUBLIC_PATH,
            &[("Host", &host_of(h.addr)), (name, value)],
            "",
        )
        .await;
        assert_eq!(page.status, 401, "{name}: 화면도 열리지 않는다");

        let keyed = enter_with(h.addr, Some(&issued.key), &[(name, value)]).await;
        assert_eq!(keyed.status, 204, "{name}: 키가 맞으면 들어온다");
        assert!(
            keyed
                .cookie_attrs()
                .contains(&format!("max-age={}", REMOTE_SESSION_MAX_AGE.as_secs())),
            "{name}: 원격 세션으로 준다"
        );
    }
    // 전달 헤더가 없는 루프백은 지금처럼 키 없이 들어온다.
    let plain = enter(h.addr, None).await;
    assert_eq!(plain.status, 204);
    for record in h.actions(ACCESS_ENTER) {
        assert_eq!(record.origin, "127.0.0.1", "기록은 실제 상대 주소");
    }
}

// 검증: N3, V39, E2, C1
#[tokio::test]
async fn 전달_헤더가_있는_루프백은_틀리면_잠기지만_그_컴퓨터는_막지_않는다() {
    let h = start().await;
    let issued = h.server.access_keys().issue().await.unwrap();
    h.as_peer(LOOPBACK);
    let proxied = [("X-Forwarded-For", "203.0.113.9")];
    fail_times(h.addr, MAX_KEY_FAILURES, &proxied).await;

    let locked = enter_with(h.addr, Some(&issued.key), &proxied).await;
    assert_eq!(locked.status, LOCKED_STATUS, "프록시를 거친 시도는 잠긴다");
    assert!(locked.cookie().is_none());
    let alerts = wait_alerts(&h, 1).await;
    assert_eq!(alerts.len(), 1);
    assert_eq!(alerts[0].device, LOOPBACK, "기기는 실제 상대 주소");

    let plain = enter(h.addr, None).await;
    assert_eq!(plain.status, 204, "그 컴퓨터의 요청은 막지 않는다");
}

// ───────────── 2 · 3. 잠금과 알림 (N3 · V39 · E2) ─────────────

// 검증: N3, V39, E2, E1, M6
#[tokio::test]
async fn 다섯_번_틀리면_15분_동안_그_기기가_막히고_알림이_한_번_온다() {
    assert_eq!(MAX_KEY_FAILURES, 5);
    assert_eq!(LOCKOUT, Duration::from_secs(15 * 60));
    let h = start().await;
    let issued = h.server.access_keys().issue().await.unwrap();
    h.set_time(T0 + 5_000);
    fail_times(h.addr, MAX_KEY_FAILURES, &[]).await;

    let alerts = wait_alerts(&h, 1).await;
    let until = UnixMillis(T0 + 5_000 + millis(LOCKOUT));
    assert_eq!(
        alerts,
        vec![LockNotice {
            device: OUTSIDE_A,
            failures: MAX_KEY_FAILURES,
            until,
        }],
        "잠기는 순간 알림 하나"
    );
    let locked = h.server.access_keys().locked();
    assert_eq!(locked.len(), 1);
    assert_eq!(locked[0].device, OUTSIDE_A);
    assert_eq!(locked[0].until, until);

    // 잠긴 동안은 맞는 키도 막는다.
    let blocked = enter(h.addr, Some(&issued.key)).await;
    assert_eq!(blocked.status, LOCKED_STATUS);
    assert!(blocked.cookie().is_none());
    assert!(!blocked.text().contains(&issued.key));
    for (name, value) in SECURITY_HEADERS {
        assert_eq!(blocked.header(name), Some(value), "잠금 응답에도 {name}");
    }
    fail_times(h.addr, 3, &[]).await;
    h.set_time(T0 + 5_000 + millis(LOCKOUT) - 1);
    assert_eq!(
        enter(h.addr, Some(&issued.key)).await.status,
        LOCKED_STATUS,
        "15분이 되기 전에는 막혀 있다"
    );
    assert_eq!(
        wait_alerts(&h, 2).await.len(),
        1,
        "잠긴 동안의 시도로 다시 알리지 않는다"
    );
    assert_eq!(h.actions(ACCESS_LOCKED).len(), 1, "잠김이 한 번 기록된다");
    assert_eq!(h.actions(ACCESS_LOCKED)[0].origin, "203.0.113.9");
    assert!(h.actions(ACCESS_ENTER).is_empty());

    // 15분이 지나면 다시 시도할 수 있다.
    h.set_time(T0 + 5_000 + millis(LOCKOUT));
    let reply = enter(h.addr, Some(&issued.key)).await;
    assert_eq!(reply.status, 204, "잠금이 풀린다");
    assert!(h.server.access_keys().locked().is_empty());
}

// 검증: N3, V38, E1
#[tokio::test]
async fn 네_번_틀린_뒤의_맞는_키는_들어온다() {
    let h = start().await;
    let issued = h.server.access_keys().issue().await.unwrap();
    fail_times(h.addr, MAX_KEY_FAILURES - 1, &[]).await;
    let reply = enter(h.addr, Some(&issued.key)).await;
    assert_eq!(reply.status, 204);
    assert!(h.alerts.all().is_empty(), "잠기지 않았으면 알림이 없다");
    assert!(h.server.access_keys().locked().is_empty());
}

// 검증: N3, V39, E2
#[tokio::test]
async fn 잠금은_그_기기만_막고_이미_들어온_세션은_유지된다() {
    let h = start().await;
    let issued = h.server.access_keys().issue().await.unwrap();
    let cookie = login(h.addr, &issued.key).await;
    let mut ws = ws_try(h.addr, Some(&cookie), 0).await.unwrap();

    fail_times(h.addr, MAX_KEY_FAILURES, &[]).await;
    assert_eq!(enter(h.addr, Some(&issued.key)).await.status, LOCKED_STATUS);

    assert_eq!(
        get_with(h.addr, PING_PATH, Some(&cookie)).await.status,
        200,
        "이미 들어온 세션은 그대로다"
    );
    let seq = h
        .server
        .publisher()
        .publish("chat", json!({ "n": 2 }))
        .await
        .unwrap();
    assert_eq!(
        next_frame(&mut ws).await["seq"],
        json!(seq),
        "열린 실시간 연결도 그대로다"
    );

    h.as_peer(OUTSIDE_B);
    assert_eq!(
        enter(h.addr, Some(&issued.key)).await.status,
        204,
        "다른 기기는 잠기지 않는다"
    );
    h.as_peer(LOOPBACK);
    assert_eq!(
        enter(h.addr, None).await.status,
        204,
        "그 컴퓨터는 막지 않는다"
    );
}

// 검증: N3, E2
#[tokio::test]
async fn 그_컴퓨터에서_잠금을_풀_수_있다() {
    let h = start().await;
    let issued = h.server.access_keys().issue().await.unwrap();
    fail_times(h.addr, MAX_KEY_FAILURES, &[]).await;
    assert_eq!(enter(h.addr, Some(&issued.key)).await.status, LOCKED_STATUS);

    let keys = h.server.access_keys();
    assert!(!keys.unlock(OUTSIDE_B), "잠기지 않은 기기는 거짓");
    assert!(keys.unlock(OUTSIDE_A), "잠긴 기기는 참");
    assert!(keys.locked().is_empty());
    assert_eq!(enter(h.addr, Some(&issued.key)).await.status, 204);
}

// 검증: N3, V39
#[tokio::test]
async fn 잠금은_메모리에만_있어_다시_켜면_풀리고_키는_남는다() {
    let h = start().await;
    let issued = h.server.access_keys().issue().await.unwrap();
    fail_times(h.addr, MAX_KEY_FAILURES, &[]).await;
    assert_eq!(enter(h.addr, Some(&issued.key)).await.status, LOCKED_STATUS);
    let Harness {
        dir,
        store,
        clock,
        peer,
        alerts,
        hits,
        server,
        ..
    } = h;
    server.shutdown().await.unwrap();

    let again = serve(&store, &clock, &peer, &alerts, &hits, |c| c).await;
    let reply = enter(again.local_addr(), Some(&issued.key)).await;
    assert_eq!(
        reply.status, 204,
        "잠금은 사라지고 저장된 키 해시로 들어온다"
    );
    assert!(again.access_keys().locked().is_empty());
    drop(dir);
}

// 검증: N3, E2
#[tokio::test]
async fn 그_컴퓨터는_키를_틀려도_잠기지_않고_늦춰진다() {
    let start_delay = Duration::from_millis(100);
    let h = start_with(|c| GateConfig {
        loopback_delay_start: start_delay,
        loopback_delay_max: Duration::from_millis(400),
        ..c
    })
    .await;
    let issued = h.server.access_keys().issue().await.unwrap();
    h.as_peer(LOOPBACK);
    let began = Instant::now();
    let mut each = Vec::new();
    for _ in 0..(MAX_KEY_FAILURES + 1) {
        let t = Instant::now();
        let reply = enter(h.addr, Some(WRONG_KEY)).await;
        each.push(t.elapsed());
        assert_eq!(reply.status, 401, "그 컴퓨터는 잠그지 않는다");
        assert!(reply.cookie().is_none());
    }
    // 100 → 200 → 400 → 400 → 400 → 400ms
    assert!(each[0] >= start_delay, "첫 실패부터 늦춘다: {each:?}");
    assert!(
        each[2] >= Duration::from_millis(400),
        "두 배씩 늘어난다: {each:?}"
    );
    assert!(
        each.iter()
            .all(|d| *d < Duration::from_millis(400) + STEP / 4),
        "상한에서 멈춘다: {each:?}"
    );
    assert!(began.elapsed() >= Duration::from_millis(1_900));
    assert!(wait_alerts(&h, 1).await.is_empty(), "알림이 없다");
    assert!(h.server.access_keys().locked().is_empty());

    assert_eq!(enter(h.addr, Some(&issued.key)).await.status, 204);
    assert_eq!(enter(h.addr, None).await.status, 204);
}

// ───────────── 4. 키 바꾸기 (V41) ─────────────

// 검증: V41, M6, C1, C2
#[tokio::test]
async fn 키를_바꾸면_다른_기기의_세션이_끊기고_새_키를_넣어야_들어온다() {
    let h = start().await;
    let keys = h.server.access_keys();
    let first = keys.issue().await.unwrap();
    let remote = login(h.addr, &first.key).await;
    let mut ws = ws_try(h.addr, Some(&remote), 0).await.unwrap();
    h.as_peer(LOOPBACK);
    let local = enter(h.addr, None).await.cookie().unwrap();

    let second = keys.issue().await.unwrap();
    assert_eq!(second.generation, first.generation + 1, "세대가 오른다");
    assert_ne!(second.key, first.key);

    assert_eq!(
        get_with(h.addr, PING_PATH, Some(&local)).await.status,
        200,
        "그 컴퓨터의 세션은 그대로다"
    );
    h.as_peer(OUTSIDE_A);
    assert_eq!(
        get_with(h.addr, PING_PATH, Some(&remote)).await.status,
        401,
        "앞 세대의 원격 세션은 끊긴다"
    );
    assert_eq!(
        next_text(&mut ws, STEP).await,
        Ok(None),
        "열린 실시간 연결도 닫힌다"
    );
    assert_eq!(ws_try(h.addr, Some(&remote), 0).await.err(), Some(401));

    let old = enter(h.addr, Some(&first.key)).await;
    assert_eq!(old.status, 401, "앞 키는 맞지 않는다");
    assert!(old.cookie().is_none());
    let fresh = login(h.addr, &second.key).await;
    assert_eq!(get_with(h.addr, PING_PATH, Some(&fresh)).await.status, 200);
    assert_eq!(h.actions(ACCESS_KEY_CHANGED).len(), 2);
}

// ───────────── 5. 접속 기록 (V40) ─────────────

// 검증: V40, M6, C1
#[tokio::test]
async fn 접속_기록에_방금_들어온_기기와_시각이_남는다() {
    let h = start().await;
    let issued = h.server.access_keys().issue().await.unwrap();
    h.set_time(T0 + 12_345);
    login(h.addr, &issued.key).await;
    h.as_peer(OUTSIDE_B);
    h.set_time(T0 + 67_890);
    login(h.addr, &issued.key).await;

    let enters = h.actions(ACCESS_ENTER);
    let got: Vec<(String, UnixMillis)> = enters
        .iter()
        .map(|record| (record.origin.clone(), record.at))
        .collect();
    assert_eq!(
        got,
        vec![
            ("203.0.113.9".to_string(), UnixMillis(T0 + 12_345)),
            ("198.51.100.7".to_string(), UnixMillis(T0 + 67_890)),
        ]
    );
    assert!(enters.windows(2).all(|w| w[0].seq < w[1].seq), "쌓인 순서");
}

// ───────────── 7. 원격 연결의 살아 있음 신호 · 보안 헤더 (D17 · D21) ─────────────

// 검증: C2, E3, V37
#[tokio::test]
async fn 원격_실시간_연결에도_ping이_가고_거부_응답에도_보안_헤더가_붙는다() {
    let h = start_with(|c| GateConfig {
        keepalive_interval: Duration::from_millis(100),
        pong_timeout: Duration::from_secs(5),
        ..c
    })
    .await;
    let issued = h.server.access_keys().issue().await.unwrap();
    let cookie = login(h.addr, &issued.key).await;
    let mut ws = ws_try(h.addr, Some(&cookie), 0).await.unwrap();
    let got_ping = tokio::time::timeout(STEP, async {
        while let Some(Ok(message)) = ws.next().await {
            if matches!(message, Message::Ping(_)) {
                return true;
            }
        }
        false
    })
    .await
    .unwrap_or(false);
    assert!(got_ping, "원격 연결에도 ping을 보낸다");

    let wrong = enter(h.addr, Some(WRONG_KEY)).await;
    let page = get_with(h.addr, PUBLIC_PATH, None).await;
    for reply in [&wrong, &page] {
        assert_eq!(reply.status, 401);
        for (name, value) in SECURITY_HEADERS {
            assert_eq!(reply.header(name), Some(value), "{name}");
        }
    }
}

// ───────────── 8. 다시 보내기의 옛 봉투 범위 (8차 검토의 참고) ─────────────

const FIXTURE_V3: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../store/tests/fixtures/v3.db");

fn ft(n: i64) -> i64 {
    1_800_000_000_000 + n
}

// 검증: V6, E3, C2
#[tokio::test]
async fn 중간_번호에서_다시_이어도_옛_봉투는_경계_안의_것만_풀린다() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("gate.sqlite");
    std::fs::copy(FIXTURE_V3, &db).unwrap();
    let store = Store::open(&db).unwrap();
    assert_eq!(store.legacy_envelope_until().unwrap(), Some(10));
    let clock = Arc::new(TestClock(AtomicI64::new(T0)));
    let peer = Arc::new(Mutex::new(LOOPBACK));
    let alerts = Arc::new(Alerts::default());
    let hits = Arc::new(AtomicUsize::new(0));
    let server = serve(&store, &clock, &peer, &alerts, &hits, |c| c).await;
    let addr = server.local_addr();
    let cookie = enter(addr, None).await.cookie().unwrap();

    let expected: Vec<(u64, &str, Value, Value)> = vec![
        (1, "chat", Value::Null, json!({ "n": 1, "text": "장 시작" })),
        (2, "feed_status", json!(ft(1000)), json!({ "ok": true })),
        (3, "Old.Kind", json!(ft(1010)), Value::Null),
        (4, "quotes stale", Value::Null, json!("글자 하나만")),
        (
            5,
            "chat",
            Value::Null,
            json!({ "at": "어제", "payload": 1 }),
        ),
        (6, "chat", Value::Null, json!({ "at": ft(1020) })),
        (
            7,
            "trade",
            json!(ft(990)),
            json!({ "amount": 9_007_199_254_740_993_i64 }),
        ),
        (8, "chat", Value::Null, json!({ "n": 8, "text": "올린 뒤" })),
        (9, "feed_status", json!(ft(2000)), json!({ "ok": false })),
        (10, "feed_status", json!(ft(2010)), json!({ "ok": true })),
        (
            11,
            "chat",
            json!(ft(2020)),
            json!({ "n": 11, "text": "마지막" }),
        ),
    ];
    for last_seq in [1_u64, 2, 3, 6, 7, 9, 10] {
        let mut ws = ws_try(addr, Some(&cookie), last_seq).await.unwrap();
        for (seq, kind, at, payload) in expected.iter().filter(|row| row.0 > last_seq) {
            let frame = next_frame(&mut ws).await;
            let what = format!("last_seq {last_seq}의 {seq}번");
            assert_eq!(frame["seq"], json!(seq), "{what}: 번호");
            assert_eq!(frame["kind"], json!(kind), "{what}: 종류");
            assert_eq!(&frame["at"], at, "{what}: 시각");
            assert_eq!(&frame["payload"], payload, "{what}: 내용");
        }
    }
    server.shutdown().await.unwrap();
    drop(dir);
}
