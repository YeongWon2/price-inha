//! (9차 보강 1) 도는 서버가 있을 때의 `access-key`: 세대와 세션은 도는 서버의 메모리에 있으므로,
//! 그 서버가 키를 만들게 해야 앞 세대 원격 세션 · 열린 WebSocket이 끊긴다(V41).
//!
//! 서버는 루프백 빈 포트에 시연 모드로 띄운다(실제 증권사 · AI 주소 없음). 다른 기기는 전달 헤더로 흉내 낸다
//! (관문은 전달 헤더가 있기만 해도 그 컴퓨터로 치지 않는다 — gate 10차).

use std::net::SocketAddr;
use std::path::Path;
use std::time::Duration;

use futures::StreamExt;
use pi_store::Store;
use price_inha::{
    ACCESS_KEY_PATH, ACCESS_LOG_PATH, AccessKeyRequest, App, CliError, DB_FILE, Launch, Settings,
    TradingWiring, Wiring, access_key,
};
use serde_json::{Value, json};
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};
use wiremock::matchers::any;
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::common::settings_at;

type Ws = WebSocketStream<MaybeTlsStream<TcpStream>>;

/// 한 걸음의 기다림 상한.
const STEP: Duration = Duration::from_secs(15);

/// 프록시를 거쳐 온 요청의 전달 헤더(값은 읽지 않는다. 있기만 해도 다른 기기다).
const PROXIED: (&str, &str) = ("X-Forwarded-For", "100.64.0.7");

const ROTATE: AccessKeyRequest = AccessKeyRequest {
    rotate: true,
    remote_name: None,
};

/// 보관 폴더 `home`으로 서버를 루프백 빈 포트에 띄운다(시연 모드, 팀 · 바깥 잇기 없음).
async fn launch(home: &Path) -> App {
    let args: Vec<String> = [
        "--home",
        home.to_str().unwrap(),
        "--port",
        "0",
        "--interval-ms",
        "20",
        "--demo",
    ]
    .iter()
    .map(|arg| (*arg).to_string())
    .collect();
    let settings = Settings::parse(&args, &|_| None, None).unwrap();
    let wiring = Wiring {
        cli_locations: Vec::new(),
        ..Wiring::default()
    };
    let launch = Launch {
        trading: TradingWiring::default(),
        team: None,
        secrets: None,
    };
    tokio::time::timeout(STEP, App::launch(settings, wiring, launch))
        .await
        .expect("서버가 시간 안에 떠야 한다")
        .expect("서버가 떠야 한다")
}

async fn close(app: App) {
    tokio::time::timeout(STEP, app.shutdown())
        .await
        .expect("시간 안에 꺼져야 한다")
        .expect("끄기가 성공해야 한다");
}

fn origin_of(addr: SocketAddr) -> String {
    format!("http://127.0.0.1:{}", addr.port())
}

/// HTTP 응답 하나: 상태 · JSON 본문(JSON이 아니면 `Null`) · `Set-Cookie`의 첫 마디.
struct Reply {
    status: u16,
    body: Value,
    cookie: Option<String>,
}

async fn send(request: reqwest::RequestBuilder, remote: bool) -> Reply {
    let request = if remote {
        request.header(PROXIED.0, PROXIED.1)
    } else {
        request
    };
    let reply = tokio::time::timeout(STEP, request.send())
        .await
        .expect("응답이 시간 안에 와야 한다")
        .expect("요청이 닿아야 한다");
    let status = reply.status().as_u16();
    let cookie = reply
        .headers()
        .get("set-cookie")
        .and_then(|value| value.to_str().ok())
        .map(|raw| raw.split(';').next().unwrap().trim().to_string());
    let bytes = reply.bytes().await.expect("본문을 읽어야 한다");
    Reply {
        status,
        body: serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        cookie,
    }
}

/// 세션을 청한다. `key`가 있으면 접속 키를 본문에 싣는다. `remote`면 다른 기기로 보인다.
async fn enter(addr: SocketAddr, key: Option<&str>, remote: bool) -> Reply {
    let origin = origin_of(addr);
    let mut request = reqwest::Client::new()
        .post(format!("{origin}{}", pi_gate::SESSION_PATH))
        .header("Origin", &origin);
    if let Some(key) = key {
        request = request
            .header("Content-Type", "application/json")
            .body(json!({ pi_gate::ACCESS_KEY_FIELD: key }).to_string());
    }
    send(request, remote).await
}

/// 다른 기기에서 접속 키로 들어와 원격 세션 쿠키를 받는다.
async fn remote_session(addr: SocketAddr, key: &str) -> String {
    let reply = enter(addr, Some(key), true).await;
    assert!(
        (200..300).contains(&reply.status),
        "맞는 키로는 들어와야 한다: {}",
        reply.status
    );
    reply.cookie.expect("세션 쿠키가 있어야 한다")
}

/// 다른 기기에서 그 쿠키로 접속 기록을 청한 답의 상태.
async fn remote_get(addr: SocketAddr, cookie: &str) -> u16 {
    let request = reqwest::Client::new()
        .get(format!("{}{ACCESS_LOG_PATH}", origin_of(addr)))
        .header("Cookie", cookie);
    send(request, true).await.status
}

/// 그 컴퓨터의 화면 경로로 접속 키를 발급받는다.
async fn issue_on_screen(addr: SocketAddr) -> String {
    let cookie = enter(addr, None, false)
        .await
        .cookie
        .expect("그 컴퓨터는 키 없이 세션을 받는다");
    let origin = origin_of(addr);
    let request = reqwest::Client::new()
        .post(format!("{origin}{ACCESS_KEY_PATH}"))
        .header("Origin", &origin)
        .header("Content-Type", "application/json")
        .header("Cookie", cookie)
        .body("{}");
    let reply = send(request, false).await;
    assert_eq!(reply.status, 200, "화면 경로로 발급이 되어야 한다");
    reply.body[pi_gate::ACCESS_KEY_FIELD]
        .as_str()
        .expect("답에 키가 있어야 한다")
        .to_string()
}

/// 다른 기기에서 원격 세션으로 실시간 연결을 연다.
async fn remote_ws(addr: SocketAddr, cookie: &str) -> Ws {
    let url = format!(
        "ws://127.0.0.1:{}{}?{}=0",
        addr.port(),
        pi_gate::WS_PATH,
        pi_gate::LAST_SEQ_PARAM
    );
    let mut request = url.into_client_request().unwrap();
    let headers = request.headers_mut();
    headers.insert("Cookie", cookie.parse().unwrap());
    headers.insert("Origin", origin_of(addr).parse().unwrap());
    headers.insert(PROXIED.0, PROXIED.1.parse().unwrap());
    let (ws, _) = tokio::time::timeout(STEP, tokio_tungstenite::connect_async(request))
        .await
        .expect("웹소켓 응답이 시간 안에 와야 한다")
        .expect("원격 세션이 있으면 웹소켓이 열려야 한다");
    ws
}

/// `wait` 안에 연결이 닫히면 참. 글자 프레임 · ping은 읽고 넘긴다.
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

/// 저장소의 접속 키 세대. 저장소 파일이나 키가 없으면 `None`.
fn stored_generation(home: &Path) -> Option<u64> {
    let file = home.join(DB_FILE);
    if !file.exists() {
        return None;
    }
    let store = Store::open(file).unwrap();
    store.access_key().unwrap().map(|record| record.generation)
}

/// 지금 아무도 듣지 않는 루프백 포트(잠깐 열었다 닫은 포트. 연결하면 거부된다).
fn refused_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap().port()
}

// 검증: V41 D2
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 도는_서버의_키를_바꾸면_앞_세대_원격_세션과_웹소켓이_끊긴다() {
    let home = tempfile::tempdir().unwrap();
    let app = launch(home.path()).await;
    let addr = app.local_addr();
    let first = issue_on_screen(addr).await;
    let old = remote_session(addr, &first).await;
    assert_eq!(
        remote_get(addr, &old).await,
        200,
        "원격 세션으로 들어와 있다"
    );
    let mut ws = remote_ws(addr, &old).await;
    let cli = settings_at(home.path(), addr.port());

    let refused = access_key(&cli, &AccessKeyRequest::default()).await;
    assert!(
        matches!(refused, Err(CliError::AccessKeyExists)),
        "도는 서버가 있어도 --rotate 없이는 거부: {refused:?}"
    );
    assert_eq!(
        remote_get(addr, &old).await,
        200,
        "거부하면 아무것도 끊기지 않는다"
    );

    let shown = access_key(&cli, &ROTATE)
        .await
        .expect("도는 서버에게 키를 바꾸게 한다");
    assert_eq!(shown.generation(), 2, "세대는 1만 오른다");
    assert_ne!(shown.key(), first);
    assert_eq!(shown.text().matches(shown.key()).count(), 1, "글에 한 번만");
    assert!(
        shown.text().contains("끊깁니다"),
        "실제로 끊기는 길에서는 그렇게 안내한다"
    );

    assert_eq!(
        remote_get(addr, &old).await,
        401,
        "앞 세대의 원격 세션은 끊겨야 한다"
    );
    assert!(
        closed_within(&mut ws, STEP).await,
        "앞 세대의 열린 웹소켓은 닫혀야 한다"
    );
    assert_eq!(
        enter(addr, Some(&first), true).await.status,
        401,
        "앞 키는 더 맞지 않는다"
    );
    let fresh = remote_session(addr, shown.key()).await;
    assert_eq!(
        remote_get(addr, &fresh).await,
        200,
        "새 키로는 들어올 수 있다"
    );

    close(app).await;
    assert_eq!(
        stored_generation(home.path()),
        Some(2),
        "저장소 세대도 1만 올랐다"
    );
}

// 검증: V41 D2
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 도는_서버에_키가_없으면_그_서버가_처음_키를_만든다() {
    let home = tempfile::tempdir().unwrap();
    let app = launch(home.path()).await;
    let addr = app.local_addr();
    let cli = settings_at(home.path(), addr.port());

    let shown = access_key(&cli, &AccessKeyRequest::default())
        .await
        .expect("처음은 만든다");
    assert_eq!(shown.generation(), 1);
    let cookie = remote_session(addr, shown.key()).await;
    assert_eq!(
        remote_get(addr, &cookie).await,
        200,
        "도는 서버가 그 키를 안다"
    );

    close(app).await;
    assert_eq!(stored_generation(home.path()), Some(1));
}

// 검증: V41 D2 N8
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 그_포트가_다른_프로그램이면_키를_만들지_않는다() {
    let other = MockServer::start().await;
    Mock::given(any())
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "hello": "world" })))
        .mount(&other)
        .await;
    let port = other.address().port();

    let empty = tempfile::tempdir().unwrap();
    let made = access_key(
        &settings_at(empty.path(), port),
        &AccessKeyRequest::default(),
    )
    .await;
    match &made {
        Err(error @ CliError::NotOurServer { port: asked }) => {
            assert_eq!(*asked, port);
            assert!(
                error.to_string().contains("--port"),
                "고치는 길을 안내한다: {error}"
            );
        }
        other => panic!("키를 만들지 않고 오류여야 한다: {other:?}"),
    }
    assert_eq!(stored_generation(empty.path()), None, "키가 생기지 않는다");

    // 이미 키가 있는 홈: 서버가 없을 때 직접 만든 1세대가 그대로 남는다.
    let home = tempfile::tempdir().unwrap();
    let first = access_key(
        &settings_at(home.path(), refused_port()),
        &AccessKeyRequest::default(),
    )
    .await
    .expect("서버가 없으면 직접 만든다");
    let rotated = access_key(&settings_at(home.path(), port), &ROTATE).await;
    assert!(
        matches!(rotated, Err(CliError::NotOurServer { .. })),
        "{rotated:?}"
    );
    assert_eq!(stored_generation(home.path()), Some(1), "세대 그대로");

    let seen = other.received_requests().await.unwrap_or_default();
    assert!(
        seen.iter().all(|request| {
            !String::from_utf8_lossy(&request.body).contains(first.key())
                && !format!("{:?}", request.headers).contains(first.key())
        }),
        "다른 프로그램에 키 글자를 보내지 않는다"
    );
}

// 검증: D2
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 연결이_거부되면_도는_서버가_없으니_직접_만든다() {
    let home = tempfile::tempdir().unwrap();
    let cli = settings_at(home.path(), refused_port());
    let first = access_key(&cli, &AccessKeyRequest::default())
        .await
        .expect("처음은 만든다");
    assert_eq!(first.generation(), 1);
    assert_eq!(stored_generation(home.path()), Some(1));
    assert!(
        !first.text().contains("끊깁니다"),
        "끊을 연결이 없는 길에서는 끊긴다고 안내하지 않는다"
    );
    let second = access_key(&cli, &ROTATE).await.expect("바꾼다");
    assert_eq!(second.generation(), 2);
    assert_eq!(stored_generation(home.path()), Some(2));
}
