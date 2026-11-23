//! 11차 시험의 공용 도우미: 서버 띄우기 · 세션 · 머리까지 받는 요청.

use std::net::SocketAddr;
use std::path::Path;
use std::time::Duration;

use pi_ai::AnthropicConfig;
use pi_market::TossConfig;
use price_inha::{App, Settings, Upstreams, Wiring};

/// 한 걸음의 기다림 상한.
pub const STEP: Duration = Duration::from_secs(15);

fn no_env(_name: &str) -> Option<String> {
    None
}

/// 아무도 듣지 않는 로컬 주소(실제 바깥 주소로 나가지 않게).
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

/// 시연 모드로 띄운 서버.
pub struct Bench {
    _dir: tempfile::TempDir,
    app: App,
}

impl Bench {
    pub async fn start() -> Bench {
        let dir = tempfile::tempdir().unwrap();
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
            peer_override: None,
            cli_locations: Vec::new(),
            ..Wiring::default()
        };
        let app = tokio::time::timeout(STEP, App::start(settings(dir.path()), wiring))
            .await
            .expect("서버가 시간 안에 떠야 한다")
            .expect("서버가 떠야 한다");
        Bench { _dir: dir, app }
    }

    pub fn addr(&self) -> SocketAddr {
        self.app.local_addr()
    }

    pub async fn close(self) {
        tokio::time::timeout(STEP, self.app.shutdown())
            .await
            .expect("시간 안에 꺼져야 한다")
            .expect("끄기가 성공해야 한다");
    }
}

/// 받은 응답: 상태 · 머리 · 본문 바이트.
pub struct Reply {
    pub status: u16,
    pub headers: reqwest::header::HeaderMap,
    pub bytes: Vec<u8>,
}

impl Reply {
    /// 그 머리의 값(한 번만 붙어야 한다). 없으면 빈 글.
    pub fn header(&self, name: &str) -> String {
        let values: Vec<&str> = self
            .headers
            .get_all(name)
            .iter()
            .map(|v| v.to_str().unwrap())
            .collect();
        assert!(values.len() <= 1, "{name}는 한 번만 붙는다: {values:?}");
        values.first().map(|v| (*v).to_owned()).unwrap_or_default()
    }

    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.bytes).into_owned()
    }
}

/// 세션을 받아 `Cookie` 헤더 글자를 돌려준다(그 컴퓨터에서는 접속 키 없이).
pub async fn session(addr: SocketAddr) -> String {
    let origin = format!("http://{addr}");
    let request = reqwest::Client::new()
        .post(format!("{origin}{}", pi_gate::SESSION_PATH))
        .header("Origin", &origin);
    let reply = tokio::time::timeout(STEP, request.send())
        .await
        .expect("응답이 시간 안에 와야 한다")
        .expect("요청이 닿아야 한다");
    assert!(
        reply.status().is_success(),
        "세션을 받아야 한다: {}",
        reply.status()
    );
    reply
        .headers()
        .get("set-cookie")
        .and_then(|v| v.to_str().ok())
        .map(|raw| raw.split(';').next().unwrap().trim().to_string())
        .expect("세션 쿠키가 있어야 한다")
}

/// `GET` (쿠키가 있으면 그 세션으로).
pub async fn get(addr: SocketAddr, path: &str, cookie: Option<&str>) -> Reply {
    let mut request = reqwest::Client::new().get(format!("http://{addr}{path}"));
    if let Some(cookie) = cookie {
        request = request.header("Cookie", cookie);
    }
    let reply = tokio::time::timeout(STEP, request.send())
        .await
        .expect("응답이 시간 안에 와야 한다")
        .expect("요청이 닿아야 한다");
    let status = reply.status().as_u16();
    let headers = reply.headers().clone();
    let bytes = reply.bytes().await.expect("본문").to_vec();
    Reply {
        status,
        headers,
        bytes,
    }
}
