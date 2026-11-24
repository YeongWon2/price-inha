//! 32차 시험의 공용 도우미: 서버 띄우기(다시 켜기 · 다른 기기로 보이기 포함) · 세션 · 요청.
//! round23 · round29의 도우미를 본떴다(그 폴더들은 고치지 않는다). 바깥 주소 자리는 닫힌 로컬 포트다.

use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use pi_ai::AnthropicConfig;
use pi_market::TossConfig;
use price_inha::{App, OFFICE_LOOKS_PATH, OFFICE_ME_PATH, Settings, Upstreams, Wiring};
use serde_json::Value;

/// 한 걸음의 기다림 상한.
pub const STEP: Duration = Duration::from_secs(15);

/// 다른 기기로 보일 때의 상대 주소(사설 대역, 실제로 닿지 않는다).
const REMOTE_PEER: [u8; 4] = [100, 64, 0, 34];

fn no_env(_name: &str) -> Option<String> {
    None
}

fn dead_base_url() -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    format!("http://127.0.0.1:{port}")
}

fn settings(home: &Path) -> Settings {
    let args: Vec<String> = ["--home", home.to_str().unwrap(), "--port", "0", "--demo"]
        .iter()
        .map(|s| (*s).to_string())
        .collect();
    Settings::parse(&args, &no_env, None).expect("설정을 읽어야 한다")
}

/// 시연 모드로 띄운 서버. 보관 폴더를 지켜 두었다가 다시 켤 수 있고, 다른 기기로 보이게 할 수 있다.
pub struct Bench {
    dir: tempfile::TempDir,
    remote: Arc<AtomicBool>,
    app: App,
}

impl Bench {
    pub async fn start() -> Bench {
        Bench::start_in(tempfile::tempdir().unwrap()).await
    }

    async fn start_in(dir: tempfile::TempDir) -> Bench {
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
                    base_url: dead,
                    timeout: Duration::from_secs(3),
                    max_retries: 0,
                    retry_base_delay: Duration::from_millis(1),
                },
            },
            peer_override: Some(peer),
            cli_locations: Vec::new(),
            ..Wiring::default()
        };
        let app = tokio::time::timeout(STEP, App::start(settings(dir.path()), wiring))
            .await
            .expect("서버가 시간 안에 떠야 한다")
            .expect("서버가 떠야 한다");
        Bench { dir, remote, app }
    }

    pub fn addr(&self) -> SocketAddr {
        self.app.local_addr()
    }

    /// 다른 기기로 보이게 한다.
    pub fn go_remote(&self) {
        self.remote.store(true, Ordering::SeqCst);
    }

    /// 끄고 같은 보관 폴더로 다시 켠다.
    pub async fn restart(self) -> Bench {
        let Bench { dir, app, .. } = self;
        tokio::time::timeout(STEP, app.shutdown())
            .await
            .expect("시간 안에 꺼져야 한다")
            .expect("끄기가 성공해야 한다");
        Bench::start_in(dir).await
    }

    pub async fn close(self) {
        tokio::time::timeout(STEP, self.app.shutdown())
            .await
            .expect("시간 안에 꺼져야 한다")
            .expect("끄기가 성공해야 한다");
    }
}

/// 받은 응답: 상태 · 본문 글자 · JSON 본문(JSON이 아니면 `Null`).
pub struct Reply {
    pub status: u16,
    pub text: String,
    pub body: Value,
}

async fn send(request: reqwest::RequestBuilder) -> Reply {
    let reply = tokio::time::timeout(STEP, request.send())
        .await
        .expect("응답이 시간 안에 와야 한다")
        .expect("요청이 닿아야 한다");
    let status = reply.status().as_u16();
    let text = reply.text().await.expect("본문");
    let body = serde_json::from_str(&text).unwrap_or(Value::Null);
    Reply { status, text, body }
}

pub fn origin_of(addr: SocketAddr) -> String {
    format!("http://{addr}")
}

/// 같은 출처에서 세션을 받아 `Cookie` 헤더 글자를 돌려준다.
pub async fn session(addr: SocketAddr) -> String {
    let origin = origin_of(addr);
    let reply = tokio::time::timeout(
        STEP,
        reqwest::Client::new()
            .post(format!("{origin}{}", pi_gate::SESSION_PATH))
            .header("Origin", &origin)
            .send(),
    )
    .await
    .expect("응답이 시간 안에 와야 한다")
    .expect("요청이 닿아야 한다");
    assert!(reply.status().is_success(), "세션을 받아야 한다");
    reply
        .headers()
        .get("set-cookie")
        .and_then(|v| v.to_str().ok())
        .map(|raw| raw.split(';').next().unwrap().trim().to_string())
        .expect("세션 쿠키가 있어야 한다")
}

/// `GET` (쿠키가 있으면 그 세션으로).
pub async fn get(addr: SocketAddr, path: &str, cookie: Option<&str>) -> Reply {
    let mut request = reqwest::Client::new().get(format!("{}{path}", origin_of(addr)));
    if let Some(cookie) = cookie {
        request = request.header("Cookie", cookie);
    }
    send(request).await
}

/// `POST` JSON 글자 `body`. `cookie` · `origin`이 `None`이면 그 머리를 붙이지 않는다.
pub async fn post_raw(
    addr: SocketAddr,
    path: &str,
    cookie: Option<&str>,
    origin: Option<&str>,
    body: &str,
) -> Reply {
    let mut request = reqwest::Client::new()
        .post(format!("{}{path}", origin_of(addr)))
        .header("Content-Type", "application/json")
        .body(body.to_owned());
    if let Some(cookie) = cookie {
        request = request.header("Cookie", cookie);
    }
    if let Some(origin) = origin {
        request = request.header("Origin", origin);
    }
    send(request).await
}

/// 같은 출처 · 그 세션으로 JSON을 `POST`한다.
pub async fn post(addr: SocketAddr, path: &str, cookie: &str, body: &Value) -> Reply {
    let origin = origin_of(addr);
    post_raw(addr, path, Some(cookie), Some(&origin), &body.to_string()).await
}

/// 내 캐릭터를 읽는다(200이어야 한다).
pub async fn me(addr: SocketAddr, cookie: &str) -> Value {
    let reply = get(addr, OFFICE_ME_PATH, Some(cookie)).await;
    assert_eq!(reply.status, 200, "내 캐릭터 읽기: {}", reply.text);
    reply.body
}

/// 원본 외형 목록([`OFFICE_LOOKS_PATH`]의 `available`).
pub async fn available_looks(addr: SocketAddr, cookie: &str) -> Vec<String> {
    let reply = get(addr, OFFICE_LOOKS_PATH, Some(cookie)).await;
    assert_eq!(reply.status, 200, "외형 목록: {}", reply.text);
    reply.body["available"]
        .as_array()
        .expect("외형 목록")
        .iter()
        .map(|v| v.as_str().expect("외형 id").to_owned())
        .collect()
}

/// 객체의 칸 이름들(정렬).
pub fn keys_of(value: &Value) -> Vec<&str> {
    let mut keys: Vec<&str> = value
        .as_object()
        .unwrap_or_else(|| panic!("객체여야 한다: {value}"))
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    keys
}
