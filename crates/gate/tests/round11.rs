//! pi-gate 11차 테스트: 다른 프로세스가 올린 접속 키 세대를 도는 서버가 본다.
//!
//! 근거: `docs/dev/modules/gate.md` "11차 작업" 1~3, `dev.md` C1 · V41, 결정 2026-11-16 8번.
//!
//! 다른 프로세스(예: 서버를 못 찾은 `access-key`)는 같은 저장소 파일을 `Store::open`으로 따로 열어
//! `set_access_key`를 부르는 것으로 흉내 낸다. 루프백이 아닌 상대는 `GateBuilder::peer_override`로
//! 흉내 낸다. 시간은 테스트의 시계가 정한다(저장소를 다시 읽을 만큼 시계를 앞으로 돌린다).
//! 시험용 키는 진짜 키처럼 보이지 않는 글자를 쓴다.
//!
//! 읽기 실패(저장소를 읽지 못할 때 원격 요청이 401이고 세션은 버려지지 않는다)는 열린 연결의
//! 저장소를 공개 API만으로 망가뜨릴 방법이 없어 여기서 확인하지 않는다.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::Router;
use axum::routing::get;
use futures::StreamExt;
use pi_gate::{
    ACCESS_KEY_FIELD, Clock, Gate, GateConfig, GateServer, LAST_SEQ_PARAM, SESSION_COOKIE,
    SESSION_PATH, WS_PATH,
};
use pi_store::{AccessKeyHash, Store, UnixMillis};
use serde_json::json;
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
const PING_PATH: &str = "/api/ping";
const OUTSIDE: IpAddr = IpAddr::V4(Ipv4Addr::new(203, 0, 113, 9));
const LOOPBACK: IpAddr = IpAddr::V4(Ipv4Addr::LOCALHOST);
/// 다른 프로세스가 새로 둔 접속 키(진짜 키처럼 보이지 않는 글자).
const OTHER_KEY: &str = "TEST-OTHER-PROCESS-KEY-0001";
const OLD_KEY_HINT: &str = "TEST-WRONG-KEY-0000";
/// 앞서 읽은 값을 다시 쓰는 시간(11차 1번)을 확실히 넘기는 만큼.
const PAST_CACHE: i64 = 2_000;
/// 앞서 읽은 값을 다시 쓰는 시간 안.
const WITHIN_CACHE: i64 = 400;

type Ws = WebSocketStream<MaybeTlsStream<TcpStream>>;

struct TestClock(AtomicI64);

impl Clock for TestClock {
    fn now(&self) -> UnixMillis {
        UnixMillis(self.0.load(Ordering::SeqCst))
    }
}

struct Harness {
    _dir: TempDir,
    path: PathBuf,
    clock: Arc<TestClock>,
    peer: Arc<Mutex<IpAddr>>,
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

    async fn key(&self) -> String {
        self.server.access_keys().issue().await.unwrap().key
    }

    /// 도는 서버 밖의 다른 프로세스가 같은 저장소 파일에 새 접속 키(`OTHER_KEY`)를 둔다.
    /// 이 서버는 이 일을 모른다. 새 세대를 돌려준다.
    fn other_process_sets_key(&self) -> u64 {
        let other = Store::open(&self.path).unwrap();
        let salt = vec![7_u8; 16];
        let mut hasher = Sha256::new();
        hasher.update(&salt);
        hasher.update(OTHER_KEY.as_bytes());
        let hash = AccessKeyHash {
            sha256: hasher.finalize().into(),
            salt,
        };
        other
            .set_access_key(hash, UnixMillis(T0))
            .unwrap()
            .generation
    }
}

async fn start_with(config: impl FnOnce(GateConfig) -> GateConfig) -> Harness {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("gate.sqlite");
    let store = Store::open(&path).unwrap();
    let clock = Arc::new(TestClock(AtomicI64::new(T0)));
    let peer = Arc::new(Mutex::new(OUTSIDE));
    let protected = Router::new().route(PING_PATH, get(|| async { "pong" }));
    let seen = Arc::clone(&peer);
    let builder = Gate::builder(store, config(GateConfig::local(0)))
        .clock(Arc::clone(&clock) as Arc<dyn Clock>)
        .protected_routes(protected)
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
        path,
        clock,
        peer,
        server,
        addr,
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
}

fn host_of(addr: SocketAddr) -> String {
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

/// 세션을 달라고 한다. `key`가 있으면 JSON 본문에 싣는다.
async fn enter(addr: SocketAddr, key: Option<&str>) -> Reply {
    let host = host_of(addr);
    let origin = format!("http://{host}");
    let mut headers = vec![("Host", host.as_str()), ("Origin", origin.as_str())];
    let body = key.map_or(String::new(), |key| {
        json!({ ACCESS_KEY_FIELD: key }).to_string()
    });
    if key.is_some() {
        headers.push(("Content-Type", "application/json"));
    }
    http(addr, "POST", SESSION_PATH, &headers, &body).await
}

/// 맞는 키로 들어오고 쿠키를 돌려준다.
async fn login(addr: SocketAddr, key: &str) -> String {
    let reply = enter(addr, Some(key)).await;
    assert_eq!(reply.status, 204, "맞는 키면 들어와야 한다");
    reply.cookie().expect("세션 쿠키가 있어야 한다")
}

async fn get_ping(addr: SocketAddr, cookie: &str) -> u16 {
    let host = host_of(addr);
    let headers = [("Host", host.as_str()), ("Cookie", cookie)];
    http(addr, "GET", PING_PATH, &headers, "").await.status
}

async fn ws_try(addr: SocketAddr, cookie: &str) -> Result<Ws, u16> {
    let host = host_of(addr);
    let url = format!("ws://{host}{WS_PATH}?{LAST_SEQ_PARAM}=0");
    let mut request = url.into_client_request().unwrap();
    let headers = request.headers_mut();
    headers.insert("Origin", format!("http://{host}").parse().unwrap());
    headers.insert("Cookie", cookie.parse().unwrap());
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

// ───────────── 1 · 2. 원격 세션을 볼 때 저장소의 지금 세대를 본다 ─────────────

// 검증: V41, C1, M6
#[tokio::test]
async fn 다른_프로세스가_올린_세대를_다음_원격_요청에서_알고_앞_세대_세션을_버린다() {
    let h = start_with(|c| c).await;
    let first = h.key().await;
    let remote = login(h.addr, &first).await;
    h.as_peer(LOOPBACK);
    let local = enter(h.addr, None).await.cookie().expect("그 컴퓨터 세션");
    h.as_peer(OUTSIDE);

    // 아무도 세대를 올리지 않았으면 시간이 지나도 그대로다.
    assert_eq!(get_ping(h.addr, &remote).await, 200);
    h.set_time(T0 + PAST_CACHE);
    assert_eq!(
        get_ping(h.addr, &remote).await,
        200,
        "세대가 같으면 저장소를 다시 읽어도 세션은 산다"
    );

    let generation = h.other_process_sets_key();
    assert_eq!(generation, 2, "다른 프로세스가 둔 새 세대");
    h.set_time(T0 + 2 * PAST_CACHE);

    assert_eq!(
        get_ping(h.addr, &remote).await,
        401,
        "다른 프로세스가 올린 세대를 알면 앞 세대 원격 세션은 세션 없음과 같다"
    );
    assert_eq!(
        ws_try(h.addr, &remote).await.err(),
        Some(401),
        "앞 세대 쿠키로는 실시간 연결도 열리지 않는다"
    );

    h.as_peer(LOOPBACK);
    assert_eq!(
        get_ping(h.addr, &local).await,
        200,
        "그 컴퓨터의 세션은 그대로다"
    );
    h.as_peer(OUTSIDE);

    let old = enter(h.addr, Some(&first)).await;
    assert_eq!(old.status, 401, "앞 키는 맞지 않는다");
    assert!(old.cookie().is_none());
    let wrong = enter(h.addr, Some(OLD_KEY_HINT)).await;
    assert_eq!(wrong.status, 401);

    let fresh = login(h.addr, OTHER_KEY).await;
    assert_eq!(
        get_ping(h.addr, &fresh).await,
        200,
        "다른 프로세스가 둔 새 키로 다시 들어올 수 있다"
    );
    assert_eq!(
        get_ping(h.addr, &remote).await,
        401,
        "새로 들어온 뒤에도 앞 세대 세션은 되살아나지 않는다"
    );
}

// 검증: V41, C1
#[tokio::test]
async fn 저장소를_읽은_값은_1초_안에는_다시_쓰고_지나면_다시_읽는다() {
    let h = start_with(|c| c).await;
    let key = h.key().await;
    let remote = login(h.addr, &key).await;

    assert_eq!(get_ping(h.addr, &remote).await, 200, "여기서 한 번 읽는다");
    h.other_process_sets_key();

    h.set_time(T0 + WITHIN_CACHE);
    assert_eq!(
        get_ping(h.addr, &remote).await,
        200,
        "1초 안의 요청은 앞서 읽은 값을 다시 쓴다(저장소를 다시 읽지 않는다)"
    );
    assert_eq!(get_ping(h.addr, &remote).await, 200);

    h.set_time(T0 + PAST_CACHE);
    assert_eq!(
        get_ping(h.addr, &remote).await,
        401,
        "1초가 지나면 저장소를 다시 읽고 올라간 세대를 안다"
    );
}

// 검증: V41, C1
#[tokio::test]
async fn 그_컴퓨터의_요청은_저장소의_세대를_보지_않는다() {
    let h = start_with(|c| c).await;
    h.as_peer(LOOPBACK);
    let local = enter(h.addr, None).await.cookie().expect("그 컴퓨터 세션");
    h.other_process_sets_key();
    h.set_time(T0 + PAST_CACHE);

    assert_eq!(get_ping(h.addr, &local).await, 200, "루프백은 그대로다");
    assert_eq!(
        enter(h.addr, None).await.status,
        204,
        "그 컴퓨터는 키 없이 새로 들어오기도 한다"
    );
    h.set_time(T0 + 2 * PAST_CACHE);
    assert_eq!(get_ping(h.addr, &local).await, 200);

    // 저장소의 세대가 올랐어도 원격은 이제 앞 세대가 없는 새 상태에서 시작한다.
    h.as_peer(OUTSIDE);
    let fresh = login(h.addr, OTHER_KEY).await;
    assert_eq!(get_ping(h.addr, &fresh).await, 200);
}

// ───────────── 3. 열린 원격 웹소켓도 ping마다 본다 ─────────────

// 검증: V41, C1, C2
#[tokio::test]
async fn 다른_프로세스가_세대를_올리면_열린_원격_웹소켓이_다음_ping_안에_닫힌다() {
    let h = start_with(|c| GateConfig {
        keepalive_interval: Duration::from_millis(100),
        pong_timeout: Duration::from_secs(5),
        ..c
    })
    .await;
    let key = h.key().await;
    let remote = login(h.addr, &key).await;
    let mut remote_ws = ws_try(h.addr, &remote).await.expect("원격 실시간 연결");
    h.as_peer(LOOPBACK);
    let local = enter(h.addr, None).await.cookie().expect("그 컴퓨터 세션");
    let mut local_ws = ws_try(h.addr, &local).await.expect("그 컴퓨터의 연결");
    h.as_peer(OUTSIDE);

    assert!(
        !closed_within(&mut remote_ws, Duration::from_millis(400)).await,
        "세대가 그대로인 동안 ping이 오가도 연결은 산다"
    );

    h.other_process_sets_key();
    h.set_time(T0 + PAST_CACHE);

    assert!(
        closed_within(&mut remote_ws, Duration::from_secs(5)).await,
        "올라간 세대를 알면 이 프로세스가 키를 바꾸지 않았어도 다음 ping 안에 닫는다"
    );
    assert!(
        !closed_within(&mut local_ws, Duration::from_millis(500)).await,
        "그 컴퓨터의 연결은 닫지 않는다"
    );
    assert_eq!(get_ping(h.addr, &remote).await, 401);
}
