//! pi-gate 10차 테스트: 9차 검토의 참고.
//!
//! 근거: `docs/dev/modules/gate.md` "10차 작업" 1~5, `dev.md` C1 · E2 · N3 · V37~V39,
//! 원본 근거 `docs/research/triggers-approval-memory-deep.md` 5.2(추적하는 신원 10,000개).
//!
//! 루프백이 아닌 상대는 `GateBuilder::peer_override`로 흉내 낸다. https 프록시(Tailscale Serve)
//! 뒤의 요청은 "루프백 상대 + 전달 헤더 + 포트 없는 원격 이름 Host + `https://` Origin"으로 흉내 낸다.
//! 원격 이름은 `.test` 이름만 쓴다. 틀린 키는 진짜 키처럼 보이지 않는 글자를 쓴다.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::Router;
use axum::routing::{get, post};
use futures::StreamExt;
use pi_gate::{
    ACCESS_KEY_FIELD, ACCESS_KEY_WRONG, Clock, Gate, GateConfig, GateServer, LAST_SEQ_PARAM,
    LOCKED_STATUS, LOCKOUT, LockAlert, LockNotice, MAX_KEY_FAILURES, MAX_TRACKED_DEVICES,
    REMOTE_SESSION_MAX_AGE, SESSION_COOKIE, SESSION_PATH, WS_PATH,
};
use pi_store::{AccessRecord, Store, UnixMillis};
use serde_json::json;
use tempfile::TempDir;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::HeaderName;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

// ───────────── 도우미 ─────────────

const STEP: Duration = Duration::from_secs(10);
const T0: i64 = 1_800_000_000_000;
const PING_PATH: &str = "/api/ping";
const POKE_PATH: &str = "/api/poke";
/// 설정으로 받는 원격 이름(Tailscale Serve가 보내는 모양: 포트 없음).
const REMOTE_NAME: &str = "pi-box.tailnet.test";
const REMOTE_ORIGIN: &str = "https://pi-box.tailnet.test";
const OUTSIDE_A: IpAddr = IpAddr::V4(Ipv4Addr::new(203, 0, 113, 9));
const LOOPBACK: IpAddr = IpAddr::V4(Ipv4Addr::LOCALHOST);
/// 프록시가 붙이는 전달 헤더(값은 관문이 읽지 않는다).
const PROXIED: (&str, &str) = ("X-Forwarded-For", "100.64.0.7");
const WRONG_KEY: &str = "TEST-WRONG-KEY-0000";

type Ws = WebSocketStream<MaybeTlsStream<TcpStream>>;

struct TestClock(AtomicI64);

impl Clock for TestClock {
    fn now(&self) -> UnixMillis {
        UnixMillis(self.0.load(Ordering::SeqCst))
    }
}

#[derive(Default)]
struct Alerts(Mutex<Vec<LockNotice>>);

impl LockAlert for Alerts {
    fn locked(&self, notice: LockNotice) {
        self.0.lock().unwrap().push(notice);
    }
}

struct Harness {
    _dir: TempDir,
    store: Store,
    clock: Arc<TestClock>,
    peer: Arc<Mutex<IpAddr>>,
    alerts: Arc<Alerts>,
    server: GateServer,
    addr: SocketAddr,
}

impl Harness {
    fn as_peer(&self, ip: IpAddr) {
        *self.peer.lock().unwrap() = ip;
    }

    fn set_time(&self, millis: i64) {
        self.clock.0.store(millis, Ordering::SeqCst);
    }

    fn actions(&self, action: &str) -> Vec<AccessRecord> {
        let log = self.store.access_log().unwrap();
        log.into_iter().filter(|r| r.action == action).collect()
    }

    async fn key(&self) -> String {
        self.server.access_keys().issue().await.unwrap().key
    }

    /// 알림이 `n`개가 될 때까지 잠깐 기다린다.
    async fn alerts(&self, n: usize) -> Vec<LockNotice> {
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            let all = self.alerts.0.lock().unwrap().clone();
            if all.len() >= n || Instant::now() > deadline {
                return all;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }
}

async fn start_with(config: impl FnOnce(GateConfig) -> GateConfig) -> Harness {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("gate.sqlite")).unwrap();
    let clock = Arc::new(TestClock(AtomicI64::new(T0)));
    let peer = Arc::new(Mutex::new(OUTSIDE_A));
    let alerts = Arc::new(Alerts::default());
    let protected = Router::new()
        .route(PING_PATH, get(|| async { "pong" }))
        .route(POKE_PATH, post(|| async { "poked" }));
    let seen = Arc::clone(&peer);
    let builder = Gate::builder(store.clone(), config(GateConfig::local(0)))
        .clock(Arc::clone(&clock) as Arc<dyn Clock>)
        .protected_routes(protected)
        .lock_alert(Arc::clone(&alerts) as Arc<dyn LockAlert>)
        .peer_override(Arc::new(move |actual: SocketAddr| {
            SocketAddr::new(*seen.lock().unwrap(), actual.port())
        }));
    let server = tokio::time::timeout(STEP, builder.build().serve())
        .await
        .expect("서버가 시간 안에 떠야 한다")
        .expect("서버가 떠야 한다");
    let addr = server.local_addr();
    Harness {
        _dir: dir,
        store,
        clock,
        peer,
        alerts,
        server,
        addr,
    }
}

fn with_remote_name(c: GateConfig) -> GateConfig {
    GateConfig {
        remote_hosts: vec![REMOTE_NAME.to_string()],
        ..c
    }
}

struct Reply {
    status: u16,
    headers: Vec<(String, String)>,
}

impl Reply {
    /// `Cookie` 헤더에 넣을 `pi_session=<값>`.
    fn cookie(&self) -> Option<String> {
        let raw = self
            .headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("set-cookie"))?;
        let first = raw.1.split(';').next()?.trim();
        first
            .starts_with(&format!("{SESSION_COOKIE}="))
            .then(|| first.to_string())
    }

    /// 원격 세션 쿠키인가(`Max-Age`가 붙는다).
    fn remote_cookie(&self) -> bool {
        self.headers.iter().any(|(k, v)| {
            k.eq_ignore_ascii_case("set-cookie") && v.to_ascii_lowercase().contains("max-age=")
        })
    }
}

fn local_host(addr: SocketAddr) -> String {
    format!("127.0.0.1:{}", addr.port())
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
    let head = text
        .split_once("\r\n\r\n")
        .map_or(text.as_str(), |(h, _)| h);
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
    Reply { status, headers }
}

/// 주어진 Host · Origin으로 세션을 달라고 한다. `key`가 있으면 JSON 본문에 싣는다.
async fn enter_as(
    addr: SocketAddr,
    host: &str,
    origin: &str,
    key: Option<&str>,
    extra: &[(&str, &str)],
) -> Reply {
    let mut headers = vec![("Host", host), ("Origin", origin)];
    headers.extend_from_slice(extra);
    let body = key.map_or(String::new(), |key| {
        json!({ ACCESS_KEY_FIELD: key }).to_string()
    });
    if key.is_some() {
        headers.push(("Content-Type", "application/json"));
    }
    http(addr, "POST", SESSION_PATH, &headers, &body).await
}

/// 그 컴퓨터의 주소 이름(`127.0.0.1:<포트>`)으로 세션을 달라고 한다.
async fn enter(addr: SocketAddr, key: Option<&str>) -> Reply {
    let host = local_host(addr);
    enter_as(addr, &host, &format!("http://{host}"), key, &[]).await
}

/// 키를 `n`번 틀린다(각 응답은 거부여야 한다).
async fn fail_times(addr: SocketAddr, n: u32) {
    for i in 0..n {
        let reply = enter(addr, Some(WRONG_KEY)).await;
        assert!(
            reply.status == 401 || reply.status == LOCKED_STATUS,
            "{i}번째 틀린 키는 거부된다: {}",
            reply.status
        );
    }
}

async fn ws_try(
    addr: SocketAddr,
    host: &str,
    origin: &str,
    cookie: &str,
    extra: &[(&str, &str)],
) -> Result<Ws, u16> {
    let url = format!("ws://{}{WS_PATH}?{LAST_SEQ_PARAM}=0", local_host(addr));
    let mut request = url.into_client_request().unwrap();
    let headers = request.headers_mut();
    headers.insert("Host", host.parse().unwrap());
    headers.insert("Origin", origin.parse().unwrap());
    headers.insert("Cookie", cookie.parse().unwrap());
    for (name, value) in extra {
        let name = HeaderName::from_bytes(name.as_bytes()).unwrap();
        headers.insert(name, value.parse().unwrap());
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

/// `wait` 안에 연결이 닫히면 참. 글자 프레임 · ping은 읽고 넘긴다(ping에는 답이 나간다).
async fn closed_within(ws: &mut Ws, wait: Duration) -> bool {
    let deadline = tokio::time::Instant::now() + wait;
    loop {
        match tokio::time::timeout_at(deadline, ws.next()).await {
            Err(_) => return false,
            Ok(None | Some(Err(_)) | Some(Ok(Message::Close(_)))) => return true,
            Ok(Some(Ok(_))) => {}
        }
    }
}

fn millis(duration: Duration) -> i64 {
    i64::try_from(duration.as_millis()).unwrap()
}

// ───────────── 1. https 프록시 뒤 원격 로그인 ─────────────

// 검증: C1 V37 V38
#[tokio::test]
async fn https_프록시_뒤의_원격_이름으로_들어오고_실시간_연결도_열린다() {
    let h = start_with(with_remote_name).await;
    let key = h.key().await;
    h.as_peer(LOOPBACK);
    let proxied = [PROXIED];

    let no_key = enter_as(h.addr, REMOTE_NAME, REMOTE_ORIGIN, None, &proxied).await;
    assert_eq!(
        no_key.status, 401,
        "Host · Origin은 지나고 키가 있어야 한다"
    );

    let reply = enter_as(h.addr, REMOTE_NAME, REMOTE_ORIGIN, Some(&key), &proxied).await;
    assert_eq!(reply.status, 204, "원격 이름 + https Origin이면 들어온다");
    assert!(reply.remote_cookie(), "프록시를 거쳤으니 원격 세션이다");
    let cookie = reply.cookie().expect("세션 쿠키가 있어야 한다");

    let read = [("Host", REMOTE_NAME), ("Cookie", cookie.as_str()), PROXIED];
    assert_eq!(http(h.addr, "GET", PING_PATH, &read, "").await.status, 200);
    let change = [
        ("Host", REMOTE_NAME),
        ("Origin", REMOTE_ORIGIN),
        ("Cookie", cookie.as_str()),
        PROXIED,
    ];
    assert_eq!(
        http(h.addr, "POST", POKE_PATH, &change, "").await.status,
        200
    );

    let with_port = format!("{REMOTE_NAME}:{}", h.addr.port());
    let port_origin = format!("https://{with_port}");
    let change = [
        ("Host", with_port.as_str()),
        ("Origin", port_origin.as_str()),
        ("Cookie", cookie.as_str()),
        PROXIED,
    ];
    assert_eq!(
        http(h.addr, "POST", POKE_PATH, &change, "").await.status,
        200,
        "<이름>:<연 포트>도 같은 출처다"
    );

    let ws = ws_try(h.addr, REMOTE_NAME, REMOTE_ORIGIN, &cookie, &proxied).await;
    assert!(
        ws.is_ok(),
        "실시간 연결도 https Origin을 받는다: {:?}",
        ws.err()
    );
}

// 검증: C1 V38
#[tokio::test]
async fn 원격_이름_밖의_https_출처는_그대로_거부한다() {
    assert!(
        GateConfig::local(0).remote_hosts.is_empty(),
        "기본은 비어 있다"
    );
    let h = start_with(with_remote_name).await;
    h.as_peer(LOOPBACK);
    let host = local_host(h.addr);
    let local_https = format!("https://{host}");
    assert_eq!(
        enter_as(h.addr, &host, &local_https, None, &[])
            .await
            .status,
        403,
        "원격 이름이 아닌 Host에는 https Origin을 받지 않는다"
    );
    for origin in [
        "https://other-box.tailnet.test",
        "https://pi-box.tailnet.test.evil.test",
        "http://other-box.tailnet.test",
    ] {
        let reply = enter_as(h.addr, REMOTE_NAME, origin, None, &[PROXIED]).await;
        assert_eq!(reply.status, 403, "Host와 다른 Origin은 거부한다: {origin}");
    }
    let stranger = "other-box.tailnet.test";
    let reply = enter_as(
        h.addr,
        stranger,
        "https://other-box.tailnet.test",
        None,
        &[PROXIED],
    )
    .await;
    assert_eq!(reply.status, 403, "허용 목록 밖의 Host는 그대로 거부한다");
}

// ───────────── 2. 잠금 확인과 실패 기록을 한 번에 ─────────────

// 검증: N3 V39 E2
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn 동시에_틀려도_키를_맞춰_보는_시도는_다섯_번을_넘지_않는다() {
    let h = start_with(|c| c).await;
    let key = h.key().await;
    let attempts = 30;
    let replies =
        futures::future::join_all((0..attempts).map(|_| enter(h.addr, Some(WRONG_KEY)))).await;
    let wrong = replies.iter().filter(|r| r.status == 401).count();
    let locked = replies.iter().filter(|r| r.status == LOCKED_STATUS).count();
    assert_eq!(wrong + locked, attempts, "모두 401이나 잠김이다");
    assert!(
        wrong <= MAX_KEY_FAILURES as usize,
        "키를 맞춰 본 시도는 {MAX_KEY_FAILURES}번까지다: {wrong}번"
    );
    assert!(h.actions(ACCESS_KEY_WRONG).len() <= MAX_KEY_FAILURES as usize);
    assert_eq!(h.alerts(1).await.len(), 1, "잠금 알림은 한 번이다");
    let reply = enter(h.addr, Some(&key)).await;
    assert_eq!(reply.status, LOCKED_STATUS, "잠긴 동안은 맞는 키도 막는다");
}

// ───────────── 3. 잠긴 기기 표와 틀림 기록의 상한 ─────────────

fn device(n: u8) -> IpAddr {
    IpAddr::V4(Ipv4Addr::new(198, 51, 100, n))
}

// 검증: N3 V39
#[tokio::test]
async fn 세어_두는_기기는_상한을_넘지_않는다() {
    assert_eq!(MAX_TRACKED_DEVICES, 10_000, "원본 값");
    assert_eq!(
        GateConfig::local(0).max_tracked_devices,
        MAX_TRACKED_DEVICES
    );
    let h = start_with(|c| GateConfig {
        max_tracked_devices: 2,
        ..c
    })
    .await;
    for n in 1..=3 {
        h.as_peer(device(n));
        fail_times(h.addr, MAX_KEY_FAILURES).await;
    }
    let locked = h.server.access_keys().locked();
    assert!(
        locked.len() <= 2,
        "잠긴 기기도 상한을 넘게 기억하지 않는다: {locked:?}"
    );
}

// 검증: N3 V39
#[tokio::test]
async fn 잠금이_끝난_기기는_상한을_차지하지_않는다() {
    let h = start_with(|c| GateConfig {
        max_tracked_devices: 2,
        ..c
    })
    .await;
    for n in 1..=2 {
        h.as_peer(device(n));
        fail_times(h.addr, MAX_KEY_FAILURES).await;
    }
    h.set_time(T0 + millis(LOCKOUT));
    for n in 3..=4 {
        h.as_peer(device(n));
        fail_times(h.addr, MAX_KEY_FAILURES).await;
        let reply = enter(h.addr, Some(WRONG_KEY)).await;
        assert_eq!(reply.status, LOCKED_STATUS, "새 기기도 잠긴다: {n}");
    }
    let locked: Vec<IpAddr> = h
        .server
        .access_keys()
        .locked()
        .into_iter()
        .map(|l| l.device)
        .collect();
    assert_eq!(locked.len(), 2);
    assert!(locked.contains(&device(3)) && locked.contains(&device(4)));
}

// ───────────── 4. 원격 실시간 연결의 수명 ─────────────

// 검증: C1 V38
#[tokio::test]
async fn 원격_실시간_연결은_세션_수명이_지나면_닫힌다() {
    let h = start_with(|c| GateConfig {
        keepalive_interval: Duration::from_millis(50),
        pong_timeout: Duration::from_secs(60),
        ..c
    })
    .await;
    let key = h.key().await;
    let host = local_host(h.addr);
    let origin = format!("http://{host}");

    let remote = enter(h.addr, Some(&key)).await.cookie().expect("원격 세션");
    let mut remote_ws = ws_try(h.addr, &host, &origin, &remote, &[]).await.unwrap();
    h.as_peer(LOOPBACK);
    let local = enter(h.addr, None).await.cookie().expect("그 컴퓨터 세션");
    let mut local_ws = ws_try(h.addr, &host, &origin, &local, &[]).await.unwrap();

    h.set_time(T0 + millis(REMOTE_SESSION_MAX_AGE) - 1);
    assert!(
        !closed_within(&mut remote_ws, Duration::from_millis(300)).await,
        "수명 안에서는 열려 있다"
    );
    h.set_time(T0 + millis(REMOTE_SESSION_MAX_AGE));
    assert!(
        closed_within(&mut remote_ws, STEP).await,
        "30일이 지나면 원격 실시간 연결을 닫는다"
    );
    assert!(
        !closed_within(&mut local_ws, Duration::from_millis(300)).await,
        "그 컴퓨터의 연결은 이 수명으로 닫지 않는다"
    );
}

// ───────────── 5. IPv6 표기 차이 ─────────────

fn mapped(v4: Ipv4Addr) -> IpAddr {
    IpAddr::V6(v4.to_ipv6_mapped())
}

// 검증: C1 V38
#[tokio::test]
async fn ipv4로_바꿀_수_있는_ipv6_루프백은_그_컴퓨터다() {
    let h = start_with(|c| c).await;
    for peer in [mapped(Ipv4Addr::LOCALHOST), IpAddr::V6(Ipv6Addr::LOCALHOST)] {
        h.as_peer(peer);
        let reply = enter(h.addr, None).await;
        assert_eq!(reply.status, 204, "{peer}는 키 없이 들어온다");
        assert!(!reply.remote_cookie(), "{peer}는 그 컴퓨터의 세션이다");
    }
    h.as_peer(mapped(Ipv4Addr::LOCALHOST));
    let host = local_host(h.addr);
    let origin = format!("http://{host}");
    let reply = enter_as(h.addr, &host, &origin, None, &[PROXIED]).await;
    assert_eq!(
        reply.status, 401,
        "전달 헤더가 있으면 루프백으로 치지 않는다"
    );
}

// 검증: N3 V39 E2
#[tokio::test]
async fn 같은_기기의_다른_표기는_한_기기로_센다() {
    let h = start_with(|c| c).await;
    let key = h.key().await;
    let v4 = Ipv4Addr::new(203, 0, 113, 9);
    h.as_peer(mapped(v4));
    fail_times(h.addr, 3).await;
    h.as_peer(OUTSIDE_A);
    fail_times(h.addr, MAX_KEY_FAILURES - 3).await;
    let alerts = h.alerts(1).await;
    assert_eq!(alerts.len(), 1);
    assert_eq!(alerts[0].device, OUTSIDE_A, "알림의 기기는 IPv4 표기다");
    let keys = h.server.access_keys();
    let locked: Vec<IpAddr> = keys.locked().into_iter().map(|l| l.device).collect();
    assert_eq!(locked, [OUTSIDE_A]);
    assert!(
        h.actions(ACCESS_KEY_WRONG)
            .iter()
            .all(|r| r.origin == "203.0.113.9"),
        "접속 기록의 어디서도 IPv4 표기다"
    );
    h.as_peer(mapped(v4));
    assert_eq!(enter(h.addr, Some(&key)).await.status, LOCKED_STATUS);
    assert!(keys.unlock(mapped(v4)), "다른 표기로도 풀 수 있다");
    assert_eq!(enter(h.addr, Some(&key)).await.status, 204);
}

// 검증: C1 V38
#[tokio::test]
async fn host의_ipv6_표기는_허용_목록_글자_그대로만_받는다() {
    let h = start_with(|c| c).await;
    h.as_peer(LOOPBACK);
    let port = h.addr.port();
    let ok = format!("[::1]:{port}");
    let reply = enter_as(h.addr, &ok, &format!("http://{ok}"), None, &[]).await;
    assert_eq!(reply.status, 204, "[::1]은 허용 목록에 있다");
    for other in [
        format!("[0:0:0:0:0:0:0:1]:{port}"),
        format!("[::ffff:127.0.0.1]:{port}"),
    ] {
        let reply = enter_as(h.addr, &other, &format!("http://{other}"), None, &[]).await;
        assert_eq!(reply.status, 403, "목록에 없는 표기는 거부한다: {other}");
    }
}

// ───────────── 6. 취소된 로그인 요청의 자리 반납 ─────────────

/// 블로킹 일꾼이 하나뿐인 실행기. 키 대조(`verify`)는 일꾼을 기다리며 멈춰 서 있게 된다.
fn single_blocking_runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .max_blocking_threads(1)
        .enable_all()
        .build()
        .unwrap()
}

/// 블로킹 일꾼을 붙잡는다. 돌려받은 보내는 쪽에 보내면 풀어 준다.
async fn hold_blocking_worker() -> std::sync::mpsc::Sender<()> {
    let (release, wait) = std::sync::mpsc::channel::<()>();
    drop(tokio::task::spawn_blocking(move || {
        let _ = wait.recv();
    }));
    tokio::time::sleep(Duration::from_millis(100)).await;
    release
}

/// 로그인 요청을 보내고, 응답을 기다리지 않고 연결을 끊는다(키 대조 도중 취소).
async fn abandon_login(addr: SocketAddr, key: &str) {
    let host = local_host(addr);
    let body = json!({ ACCESS_KEY_FIELD: key }).to_string();
    let request = format!(
        "POST {SESSION_PATH} HTTP/1.1\r\nHost: {host}\r\nOrigin: http://{host}\r\n\
         Content-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    );
    let mut stream = TcpStream::connect(addr).await.unwrap();
    stream.write_all(request.as_bytes()).await.unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    drop(stream);
    tokio::time::sleep(Duration::from_millis(100)).await;
}

/// 키 대조가 멈춰 선 로그인 요청을 `n`번 버린 관문을 돌려준다(키는 미리 만들어 둔다).
async fn after_abandoned_logins(n: u32) -> (Harness, String) {
    let h = start_with(|c| c).await;
    let key = h.key().await;
    let release = hold_blocking_worker().await;
    for _ in 0..n {
        abandon_login(h.addr, WRONG_KEY).await;
    }
    release.send(()).unwrap();
    (h, key)
}

// 검증: N3 V39 E2
#[test]
fn 키_대조_도중_버려진_요청이_다섯_번이어도_맞는_키로_들어온다() {
    single_blocking_runtime().block_on(async {
        let (h, key) = after_abandoned_logins(MAX_KEY_FAILURES).await;
        let reply = enter(h.addr, Some(&key)).await;
        assert_eq!(
            reply.status, 204,
            "버려진 시도가 자리를 붙들고 있으면 안 된다"
        );
        assert!(reply.remote_cookie(), "원격 세션이다");
        assert!(
            h.server.access_keys().locked().is_empty(),
            "버려진 시도는 틀림으로 세지 않는다"
        );
        assert!(h.alerts.0.lock().unwrap().is_empty(), "잠금 알림도 없다");
    });
}

// 검증: N3 V39 E2
#[test]
fn 버려진_요청으로_막힌_기기도_열다섯_분_뒤에는_풀려_있다() {
    single_blocking_runtime().block_on(async {
        let (h, key) = after_abandoned_logins(MAX_KEY_FAILURES + 2).await;
        h.set_time(T0 + millis(LOCKOUT));
        let reply = enter(h.addr, Some(&key)).await;
        assert_eq!(reply.status, 204, "{LOCKOUT:?} 뒤에는 들어온다");
        assert!(
            h.server.access_keys().locked().is_empty(),
            "잠긴 기기 표에 남지 않는다"
        );
    });
}
