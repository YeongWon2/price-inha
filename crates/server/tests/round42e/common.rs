//! 다듬기 묶음 E의 발판: 실행 파일 조립(`run_assembly`)으로 띄우되 바깥 자리(증권사 · AI · 자료 · 네이버 · 텔레그램)는
//! 모두 받은 요청을 세는 로컬 가짜 서버(무엇이든 503)로 바꾼다. 세션 · 같은 출처 요청 도우미.

use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use pi_ai::AnthropicConfig;
use pi_data::DataConfig;
use pi_market::{Clock, TossConfig};
use price_inha::{App, MessengerWiring, Settings, Upstreams, run_assembly};
use serde_json::Value;
use wiremock::matchers::any;
use wiremock::{Mock, MockServer, ResponseTemplate};

/// 한 단계의 시간 상한.
pub const STEP: Duration = Duration::from_secs(15);

/// 무엇을 물어도 503으로 답하고 받은 요청을 세는 로컬 가짜 서버.
pub async fn counting_server() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(any())
        .respond_with(ResponseTemplate::new(503))
        .mount(&server)
        .await;
    server
}

/// 받은 요청 수.
pub async fn received(server: &MockServer) -> usize {
    server
        .received_requests()
        .await
        .map_or(0, |list| list.len())
}

/// 보관 폴더 `home`으로 읽은 실행 설정(포트 0, `demo`면 `--demo`). 환경 변수는 읽지 않는다.
pub fn run_settings(home: &Path, demo: bool) -> Settings {
    let mut args: Vec<String> = [
        "--home",
        home.to_str().unwrap(),
        "--port",
        "0",
        "--interval-ms",
        "20",
    ]
    .iter()
    .map(|s| (*s).to_string())
    .collect();
    if demo {
        args.push("--demo".to_string());
    }
    Settings::parse(&args, &|_| None, None).expect("설정을 읽어야 한다")
}

/// 띄운 서버와 바깥 자리 가짜 서버들.
pub struct Running {
    pub app: App,
    pub outside: Vec<(&'static str, MockServer)>,
}

impl Running {
    /// 실행 파일 조립으로 띄운다(시계는 실제 시계, 바깥 주소는 모두 가짜 서버).
    pub async fn start(home: &Path, demo: bool) -> Running {
        let outside = vec![
            ("증권사", counting_server().await),
            ("AI", counting_server().await),
            ("자료", counting_server().await),
            ("네이버", counting_server().await),
            ("텔레그램", counting_server().await),
        ];
        let settings = run_settings(home, demo);
        let mut assembly = run_assembly(&settings, None);
        assembly.wiring.clock = Arc::new(pi_market::SystemClock) as Arc<dyn Clock>;
        assembly.wiring.cli_locations = Vec::new();
        assembly.wiring.upstreams = Upstreams {
            toss: TossConfig {
                base_url: outside[0].1.uri(),
                timeout: Duration::from_secs(3),
                token_failure_cooldown: Duration::ZERO,
                ..TossConfig::default()
            },
            anthropic: AnthropicConfig {
                base_url: outside[1].1.uri(),
                timeout: Duration::from_secs(3),
                max_retries: 0,
                retry_base_delay: Duration::from_millis(1),
            },
        };
        if let Some(team) = assembly.launch.team.as_mut() {
            let data_uri = outside[2].1.uri();
            team.data = DataConfig::new(data_uri.clone(), data_uri.clone(), data_uri);
            team.data.timeout = Duration::from_secs(3);
            team.news = pi_data::NaverNewsConfig::new(outside[3].1.uri())
                .with_timeout(Duration::from_secs(3));
        }
        assembly.demo_ai_pause = Duration::ZERO;
        assembly.messenger = MessengerWiring {
            telegram_api_base: outside[4].1.uri(),
            poll_timeout_secs: 1,
            request_timeout: Duration::from_secs(3),
            backoff_initial: Duration::from_secs(3),
            backoff_max: Duration::from_secs(7),
            send_attempts: 1,
            ..MessengerWiring::default()
        };
        let app = tokio::time::timeout(STEP, App::launch_run(settings, assembly))
            .await
            .expect("서버가 시간 안에 떠야 한다")
            .expect("서버가 떠야 한다");
        Running { app, outside }
    }

    pub fn addr(&self) -> SocketAddr {
        self.app.local_addr()
    }

    /// 바깥 가짜 서버로 요청이 하나도 가지 않았는지 본다.
    pub async fn assert_stayed_home(&self) {
        for (name, server) in &self.outside {
            assert_eq!(received(server).await, 0, "{name}로 나가지 않는다");
        }
    }

    pub async fn close(self) {
        tokio::time::timeout(STEP, self.app.shutdown())
            .await
            .expect("시간 안에 꺼져야 한다")
            .expect("끄기가 성공해야 한다");
    }
}

fn origin_of(addr: SocketAddr) -> String {
    format!("http://127.0.0.1:{}", addr.port())
}

/// HTTP 응답 하나: 상태 · JSON 본문(JSON이 아니면 `Null`) · 본문 글.
pub struct Reply {
    pub status: u16,
    pub body: Value,
    pub text: String,
}

async fn send(request: reqwest::RequestBuilder) -> Reply {
    let reply = tokio::time::timeout(STEP, request.send())
        .await
        .expect("응답이 시간 안에 와야 한다")
        .expect("요청이 닿아야 한다");
    let status = reply.status().as_u16();
    let text = reply.text().await.expect("본문을 읽어야 한다");
    let body = serde_json::from_str(&text).unwrap_or(Value::Null);
    Reply { status, body, text }
}

/// 같은 출처에서 (그 컴퓨터의) 세션을 받아 `Cookie` 헤더 글자를 돌려준다.
pub async fn login(addr: SocketAddr) -> String {
    let origin = origin_of(addr);
    let reply = reqwest::Client::new()
        .post(format!("{origin}{}", pi_gate::SESSION_PATH))
        .header("Origin", &origin)
        .send()
        .await
        .expect("세션 요청이 닿아야 한다");
    assert!(reply.status().is_success(), "세션을 받아야 한다");
    let raw = reply
        .headers()
        .get("set-cookie")
        .expect("세션 쿠키가 있어야 한다")
        .to_str()
        .unwrap();
    raw.split(';').next().unwrap().trim().to_string()
}

/// 같은 출처(`Origin`)에서 세션을 받아 `method`를 보낸다(`body`는 JSON, 없으면 몸통 없음).
pub async fn send_json(
    addr: SocketAddr,
    method: reqwest::Method,
    path: &str,
    body: Option<&Value>,
) -> Reply {
    let cookie = login(addr).await;
    let origin = origin_of(addr);
    let mut request = reqwest::Client::new()
        .request(method, format!("{origin}{path}"))
        .header("Origin", &origin)
        .header("Cookie", &cookie);
    if let Some(body) = body {
        request = request
            .header("Content-Type", "application/json")
            .body(body.to_string());
    }
    send(request).await
}

/// 세션을 받아 읽는다.
pub async fn get(addr: SocketAddr, path: &str) -> Reply {
    send_json(addr, reqwest::Method::GET, path, None).await
}

/// 세션을 받아 같은 출처에서 JSON을 `POST`한다.
pub async fn post(addr: SocketAddr, path: &str, body: &Value) -> Reply {
    send_json(addr, reqwest::Method::POST, path, Some(body)).await
}

/// JSON 물체의 칸 이름들(가나다순).
pub fn keys_of(value: &Value) -> Vec<String> {
    let mut keys: Vec<String> = value
        .as_object()
        .map(|object| object.keys().cloned().collect())
        .unwrap_or_default();
    keys.sort();
    keys
}
