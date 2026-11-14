//! pi-gate 3차 테스트: 그 컴퓨터에서만 열리는 경로(`local_only` · `GateBuilder::local_routes`).
//!
//! 근거: `docs/dev/modules/gate.md` "3차 작업" 규칙 4 — 키 등록은 그 컴퓨터에서만.
//! 접속 관문의 "보호된 경로" 확인과 별개로, 그 경로 자체가 상대 주소를 한 번 더 본다.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use axum::Router;
use axum::routing::{get, post};
use pi_gate::{
    Gate, GateBuilder, GateConfig, GateServer, LOCAL_ONLY_STATUS, Peer, SESSION_COOKIE,
    SESSION_PATH, local_only,
};
use pi_store::Store;
use tempfile::TempDir;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

/// 한 걸음을 기다리는 가장 긴 시간.
const STEP: Duration = Duration::from_secs(10);
/// 그 컴퓨터에서만 열리는 경로.
const KEYS_PATH: &str = "/api/keys";
const LOOK_PATH: &str = "/api/look";
/// 루프백이 아닌 상대(문서용 주소 대역).
const OUTSIDE_IP: IpAddr = IpAddr::V4(Ipv4Addr::new(203, 0, 113, 9));

/// 핸들러가 불린 횟수를 세는 경로 둘(`POST` 하나, `GET` 하나).
fn counted(hits: &Arc<AtomicUsize>) -> Router {
    let (h1, h2) = (Arc::clone(hits), Arc::clone(hits));
    Router::new()
        .route(
            KEYS_PATH,
            post(move || {
                let h = Arc::clone(&h1);
                async move {
                    h.fetch_add(1, Ordering::SeqCst);
                    "registered"
                }
            }),
        )
        .route(
            LOOK_PATH,
            get(move || {
                let h = Arc::clone(&h2);
                async move {
                    h.fetch_add(1, Ordering::SeqCst);
                    "looked"
                }
            }),
        )
}

/// 접속 관문 없이 경로만 띄운다(관문이 요청을 지나가게 했다고 치는 자리).
async fn serve_plain(router: Router) -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let _ = axum::serve(listener, router).await;
    });
    addr
}

struct Reply {
    status: u16,
    headers: String,
    body: String,
}

/// HTTP/1.1 요청 하나를 날것으로 보낸다. 헤더는 준 것만 나간다.
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
    let status = head
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|code| code.parse().ok())
        .expect("상태 줄이 있어야 한다");
    Reply {
        status,
        headers: head.to_string(),
        body: body.to_string(),
    }
}

fn host_of(addr: SocketAddr) -> String {
    format!("127.0.0.1:{}", addr.port())
}

// ───────────── local_only: 경로 자체의 확인 ─────────────

// 검증: C3, C16, C15
#[tokio::test]
async fn 관문을_지난_요청이어도_루프백이_아니면_그_경로가_거부한다() {
    // 접속 키(T17)가 생긴 뒤를 흉내 낸다: 관문이 밖에서 온 요청을 지나가게 했고,
    // 확인을 지난 상대 주소(`Peer`)가 요청의 확장에 들어 있다.
    let hits = Arc::new(AtomicUsize::new(0));
    let router = local_only(counted(&hits)).layer(axum::Extension(Peer(OUTSIDE_IP)));
    let addr = serve_plain(router).await;
    let host = host_of(addr);

    for (method, path) in [("POST", KEYS_PATH), ("GET", LOOK_PATH)] {
        let reply = http(addr, method, path, &[("Host", &host)]).await;
        assert_eq!(
            reply.status, LOCAL_ONLY_STATUS,
            "{method} {path}: 그 컴퓨터 밖에서는 열리지 않는다"
        );
        assert!(
            !reply.body.contains("registered") && !reply.body.contains("looked"),
            "핸들러의 응답이 나가면 안 된다"
        );
        assert!(
            !format!("{}{}", reply.headers, reply.body).contains("203.0.113.9"),
            "거부 응답에 상대 주소를 싣지 않는다"
        );
    }
    assert_eq!(
        hits.load(Ordering::SeqCst),
        0,
        "거부된 요청은 핸들러에 닿지 않는다"
    );
}

// 검증: C3, C15
#[tokio::test]
async fn 루프백에서_온_요청은_그_경로가_받는다() {
    for ip in [
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        IpAddr::V6(Ipv6Addr::LOCALHOST),
        IpAddr::V4(Ipv4Addr::new(127, 8, 8, 8)),
    ] {
        let hits = Arc::new(AtomicUsize::new(0));
        let router = local_only(counted(&hits)).layer(axum::Extension(Peer(ip)));
        let addr = serve_plain(router).await;
        let reply = http(addr, "POST", KEYS_PATH, &[("Host", &host_of(addr))]).await;
        assert_eq!(reply.status, 200, "{ip}");
        assert_eq!(reply.body, "registered", "{ip}");
        assert_eq!(hits.load(Ordering::SeqCst), 1, "{ip}");
    }
}

// 검증: C3, C15
#[tokio::test]
async fn 상대_주소를_모르면_그_경로는_열리지_않는다() {
    // 확장에 `Peer`가 없다(관문을 거치지 않았거나 확인을 건너뛴 요청). 모르면 닫는다.
    let hits = Arc::new(AtomicUsize::new(0));
    let addr = serve_plain(local_only(counted(&hits))).await;
    let reply = http(addr, "POST", KEYS_PATH, &[("Host", &host_of(addr))]).await;
    assert_eq!(reply.status, LOCAL_ONLY_STATUS);
    assert_eq!(hits.load(Ordering::SeqCst), 0);
}

// 검증: C3, C15
#[tokio::test]
async fn 전달_헤더로는_그_컴퓨터인_척할_수_없다() {
    let hits = Arc::new(AtomicUsize::new(0));
    let router = local_only(counted(&hits)).layer(axum::Extension(Peer(OUTSIDE_IP)));
    let addr = serve_plain(router).await;
    let reply = http(
        addr,
        "POST",
        KEYS_PATH,
        &[
            ("Host", &host_of(addr)),
            ("X-Forwarded-For", "127.0.0.1"),
            ("Forwarded", "for=127.0.0.1"),
            ("X-Real-IP", "127.0.0.1"),
        ],
    )
    .await;
    assert_eq!(reply.status, LOCAL_ONLY_STATUS);
    assert_eq!(hits.load(Ordering::SeqCst), 0);
}

// ───────────── GateBuilder::local_routes: 관문에 끼운 모습 ─────────────

struct Harness {
    _dir: TempDir,
    server: GateServer,
    addr: SocketAddr,
    hits: Arc<AtomicUsize>,
}

async fn start(build: impl FnOnce(GateBuilder) -> GateBuilder) -> Harness {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("gate.sqlite")).unwrap();
    let hits = Arc::new(AtomicUsize::new(0));
    let builder = Gate::builder(store, GateConfig::local(0)).local_routes(counted(&hits));
    let server = tokio::time::timeout(STEP, build(builder).build().serve())
        .await
        .expect("서버가 시간 안에 떠야 한다")
        .expect("서버가 떠야 한다");
    let addr = server.local_addr();
    Harness {
        _dir: dir,
        server,
        addr,
        hits,
    }
}

async fn stop(harness: Harness) {
    tokio::time::timeout(STEP, harness.server.shutdown())
        .await
        .expect("시간 안에 꺼져야 한다")
        .expect("끄기가 성공해야 한다");
}

/// 같은 출처에서 세션을 받아 `Cookie` 헤더 글자를 돌려준다.
async fn login(addr: SocketAddr) -> String {
    let host = host_of(addr);
    let origin = format!("http://{host}");
    let reply = http(
        addr,
        "POST",
        SESSION_PATH,
        &[("Host", &host), ("Origin", &origin)],
    )
    .await;
    assert_eq!(reply.status, 204, "루프백에서는 세션을 받아야 한다");
    let value = reply
        .headers
        .lines()
        .filter_map(|line| line.split_once(':'))
        .find(|(name, _)| name.eq_ignore_ascii_case("set-cookie"))
        .and_then(|(_, value)| value.trim().split(';').next())
        .and_then(|pair| pair.split_once('='))
        .map(|(_, value)| value.to_string())
        .expect("세션 쿠키가 있어야 한다");
    format!("{SESSION_COOKIE}={value}")
}

// 검증: C3, C16, C15
#[tokio::test]
async fn 그_컴퓨터_전용_경로는_세션과_같은_출처가_있어야_열린다() {
    let h = start(|b| b).await;
    let (host, origin) = (host_of(h.addr), format!("http://{}", host_of(h.addr)));
    let cookie = login(h.addr).await;

    // 세션이 없으면 401.
    let reply = http(
        h.addr,
        "POST",
        KEYS_PATH,
        &[("Host", &host), ("Origin", &origin)],
    )
    .await;
    assert_eq!(reply.status, 401, "세션 없이 열리면 안 된다");
    let reply = http(h.addr, "GET", LOOK_PATH, &[("Host", &host)]).await;
    assert_eq!(reply.status, 401);

    // 상태를 바꾸는 요청은 같은 출처여야 한다.
    let reply = http(
        h.addr,
        "POST",
        KEYS_PATH,
        &[("Host", &host), ("Cookie", &cookie)],
    )
    .await;
    assert!(
        (400..500).contains(&reply.status),
        "Origin이 없으면 거부: {}",
        reply.status
    );
    let reply = http(
        h.addr,
        "POST",
        KEYS_PATH,
        &[
            ("Host", &host),
            ("Cookie", &cookie),
            ("Origin", "http://evil.example"),
        ],
    )
    .await;
    assert!(
        (400..500).contains(&reply.status),
        "다른 출처면 거부: {}",
        reply.status
    );
    assert_eq!(
        h.hits.load(Ordering::SeqCst),
        0,
        "거부된 요청은 핸들러에 닿지 않는다"
    );

    // 세션과 같은 출처가 있으면 열린다.
    let reply = http(
        h.addr,
        "POST",
        KEYS_PATH,
        &[("Host", &host), ("Cookie", &cookie), ("Origin", &origin)],
    )
    .await;
    assert_eq!(reply.status, 200);
    assert_eq!(reply.body, "registered");
    let reply = http(
        h.addr,
        "GET",
        LOOK_PATH,
        &[("Host", &host), ("Cookie", &cookie)],
    )
    .await;
    assert_eq!(reply.status, 200);
    assert_eq!(h.hits.load(Ordering::SeqCst), 2);
    stop(h).await;
}

// 검증: C3, C15
#[tokio::test]
async fn 루프백이_아닌_상대는_그_컴퓨터_전용_경로에_닿지_못한다() {
    let outside = SocketAddr::new(OUTSIDE_IP, 40000);
    let h = start(|b| b.peer_override(Arc::new(move |_actual| outside))).await;
    let (host, origin) = (host_of(h.addr), format!("http://{}", host_of(h.addr)));
    let reply = http(
        h.addr,
        "POST",
        KEYS_PATH,
        &[("Host", &host), ("Origin", &origin)],
    )
    .await;
    assert!(
        reply.status == 401 || reply.status == LOCAL_ONLY_STATUS,
        "밖에서 온 요청은 거부한다: {}",
        reply.status
    );
    assert_eq!(h.hits.load(Ordering::SeqCst), 0);
    stop(h).await;
}
