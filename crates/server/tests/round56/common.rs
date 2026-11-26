//! 기능 H 시험의 공용 도우미: 서버 띄우기, 세션(그 컴퓨터 · 다른 기기), HTTP 요청(round13 발판을 본뜬다).

use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use pi_ai::AnthropicConfig;
use pi_market::TossConfig;
use pi_store::{AccessKeyHash, UnixMillis};
use price_inha::{App, Launch, Reach, Settings, TradingWiring, Upstreams, Wiring};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tempfile::TempDir;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// 한 걸음의 기다림 상한.
pub const STEP: Duration = Duration::from_secs(15);

/// 다른 기기의 접속 키(시험용, 글자 수 · 글자 모음은 `pi_gate`의 것).
const ACCESS_KEY: &str = "0123456789ABCDEFGHJK";

/// 다른 기기로 볼 상대 주소.
const REMOTE_PEER: [u8; 4] = [100, 64, 0, 9];

/// 전달 머리글에 적는 값(관문은 값을 읽지 않고 있는지만 본다).
pub const FORWARDED_FOR: &str = "203.0.113.9";

fn no_env(_name: &str) -> Option<String> {
    None
}

/// 설정: 그 보관 폴더 · 빈 포트 · 시연.
fn settings(home: &std::path::Path) -> Settings {
    let args: Vec<String> = ["--home", home.to_str().unwrap(), "--port", "0", "--demo"]
        .iter()
        .map(|s| (*s).to_string())
        .collect();
    Settings::parse(&args, &no_env, None).expect("설정을 읽어야 한다")
}

/// 아무도 듣지 않는 로컬 주소(실제 바깥 주소로 나가지 않게).
fn dead_base_url() -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    format!("http://127.0.0.1:{port}")
}

/// AI 키 확인(`GET /v1/models`)에 늘 성공으로 답하는 가짜 AI 서버.
pub async fn fake_ai() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/models"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data": []})))
        .mount(&server)
        .await;
    server
}

/// 떠 있는 서버 하나와 그 보관 폴더.
pub struct Bench {
    _dir: TempDir,
    remote: Arc<AtomicBool>,
    app: App,
}

impl Bench {
    /// 새 보관 폴더로 띄운다. AI 키 확인은 `ai`로 간다.
    pub async fn start(ai: &str) -> Bench {
        let dir = tempfile::tempdir().unwrap();
        let remote = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&remote);
        let peer: pi_gate::PeerOverride = Arc::new(move |addr: SocketAddr| {
            if flag.load(Ordering::SeqCst) {
                SocketAddr::from((REMOTE_PEER, addr.port()))
            } else {
                addr
            }
        });
        let dead = dead_base_url();
        let wiring = Wiring {
            upstreams: Upstreams {
                toss: TossConfig {
                    base_url: dead.clone(),
                    timeout: Duration::from_secs(3),
                    token_failure_cooldown: Duration::ZERO,
                    ..TossConfig::default()
                },
                anthropic: AnthropicConfig {
                    base_url: ai.to_string(),
                    timeout: Duration::from_secs(3),
                    max_retries: 0,
                    retry_base_delay: Duration::from_millis(1),
                },
            },
            peer_override: Some(peer),
            cli_locations: Vec::new(),
            ..Wiring::default()
        };
        let push = pi_notify::HttpTransport::new(Duration::from_secs(3))
            .unwrap()
            .redirect_to(url::Url::parse(&dead).unwrap());
        let reach = Reach {
            push: Some(Arc::new(push)),
            backup: None,
        };
        let launch = Launch {
            trading: TradingWiring::default(),
            team: None,
            secrets: None,
        };
        let start = App::launch_with(settings(dir.path()), wiring, launch, reach);
        let app = tokio::time::timeout(STEP, start)
            .await
            .expect("서버가 시간 안에 떠야 한다")
            .expect("서버가 떠야 한다");
        Bench {
            _dir: dir,
            remote,
            app,
        }
    }

    pub fn addr(&self) -> SocketAddr {
        self.app.local_addr()
    }

    /// 다른 기기로 보이게 한다(세션 없이).
    pub fn go_remote(&self) {
        self.remote.store(true, Ordering::SeqCst);
    }

    /// 다른 기기의 원격 세션을 받는다: 접속 키를 두고, 다른 기기로 보이게 한 뒤 그 키로 들어간다.
    pub async fn remote_login(&self) -> String {
        let salt = vec![7_u8; 16];
        let mut hasher = Sha256::new();
        hasher.update(&salt);
        hasher.update(ACCESS_KEY.as_bytes());
        let hash = AccessKeyHash {
            sha256: hasher.finalize().into(),
            salt,
        };
        self.app
            .store()
            .set_access_key(hash, UnixMillis(0))
            .expect("접속 키를 둔다");
        self.go_remote();
        let body = json!({ pi_gate::ACCESS_KEY_FIELD: ACCESS_KEY });
        session_cookie(self.addr(), Some(body))
            .await
            .expect("원격 세션을 받아야 한다")
    }

    pub async fn close(self) {
        tokio::time::timeout(STEP, self.app.shutdown())
            .await
            .expect("시간 안에 꺼져야 한다")
            .expect("끄기가 성공해야 한다");
    }
}

// ───────────── HTTP ─────────────

pub fn origin_of(addr: SocketAddr) -> String {
    format!("http://127.0.0.1:{}", addr.port())
}

/// HTTP 응답 하나: 상태 · 내용 형식 · 세션 쿠키를 주었나 · 본문 바이트.
pub struct Reply {
    pub status: u16,
    pub content_type: String,
    pub sets_cookie: bool,
    pub bytes: Vec<u8>,
}

impl Reply {
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.bytes).into_owned()
    }
}

pub async fn send(request: reqwest::RequestBuilder) -> Reply {
    let reply = tokio::time::timeout(STEP, request.send())
        .await
        .expect("응답이 시간 안에 와야 한다")
        .expect("요청이 닿아야 한다");
    let status = reply.status().as_u16();
    let content_type = reply
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_string();
    let sets_cookie = reply.headers().contains_key("set-cookie");
    let bytes = reply.bytes().await.expect("본문을 읽어야 한다").to_vec();
    Reply {
        status,
        content_type,
        sets_cookie,
        bytes,
    }
}

/// 같은 출처에서 세션을 청한다. 받으면 `Cookie` 머리글 글자, 못 받으면 `None`.
/// `body`가 있으면 그 본문(접속 키)으로 들어간다 — 화면이 하듯 본문 없이 청하면 그 컴퓨터 세션이다.
pub async fn session_cookie(addr: SocketAddr, body: Option<Value>) -> Option<String> {
    let origin = origin_of(addr);
    let mut request = reqwest::Client::new()
        .post(format!("{origin}{}", pi_gate::SESSION_PATH))
        .header("Origin", &origin);
    if let Some(body) = body {
        request = request
            .header("Content-Type", "application/json")
            .body(body.to_string());
    }
    let reply = request.send().await.expect("세션 요청이 닿아야 한다");
    if !reply.status().is_success() {
        return None;
    }
    let raw = reply.headers().get("set-cookie")?.to_str().ok()?;
    Some(raw.split(';').next()?.trim().to_string())
}

/// 읽는다. `cookie`가 없으면 세션 없이, `forwarded`면 전달 머리글(`X-Forwarded-For`)을 붙인다.
pub async fn get(addr: SocketAddr, path: &str, cookie: Option<&str>, forwarded: bool) -> Reply {
    let mut request = reqwest::Client::new().get(format!("{}{path}", origin_of(addr)));
    if let Some(cookie) = cookie {
        request = request.header("Cookie", cookie);
    }
    if forwarded {
        request = request.header("X-Forwarded-For", FORWARDED_FOR);
    }
    send(request).await
}

/// 같은 출처로 JSON을 보낸다(`cookie`가 없으면 세션 없이).
pub async fn post(addr: SocketAddr, path: &str, cookie: Option<&str>, body: &Value) -> Reply {
    let mut request = reqwest::Client::new()
        .post(format!("{}{path}", origin_of(addr)))
        .header("Origin", origin_of(addr))
        .header("Content-Type", "application/json")
        .body(body.to_string());
    if let Some(cookie) = cookie {
        request = request.header("Cookie", cookie);
    }
    send(request).await
}
