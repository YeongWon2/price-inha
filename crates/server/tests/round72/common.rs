//! round72 시험이 함께 쓰는 도우미: 받아들이기 · 거절 · 장애로 답하는 로컬 가짜 바깥 서버, 실행 파일 조립 굽히기,
//! 세션을 받아 부르는 도우미. 실제 증권사 · AI · 자료 서버를 부르지 않는다.

use std::net::SocketAddr;
use std::path::Path;
use std::time::Duration;

use pi_ai::AnthropicConfig;
use pi_data::{DataConfig, NaverNewsConfig};
use pi_market::TossConfig;
use price_inha::{RunAssembly, Settings, Upstreams, run_assembly};
use serde_json::{Value, json};
use wiremock::matchers::{any, method};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

/// 한 걸음을 기다리는 가장 긴 시간.
pub const STEP: Duration = Duration::from_secs(15);
/// 바깥 요청이 없는지 지켜보는 시간.
pub const QUIET: Duration = Duration::from_millis(1_500);

/// 가짜 바깥 서버가 키 확인에 답하는 법.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Answer {
    /// 키를 받아들인다(토큰 · 모델 목록 · 뉴스를 준다).
    Accept,
    /// 키를 거절한다(401).
    Reject,
    /// 장애(503).
    Down,
}

/// 증권사(0) · AI(1) · 자료(2) · 뉴스(3) 주소 자리의 가짜 서버들.
pub struct Outside {
    servers: Vec<MockServer>,
}

async fn plain(status: u16) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(any())
        .respond_with(ResponseTemplate::new(status))
        .mount(&server)
        .await;
    server
}

async fn toss(answer: Answer) -> MockServer {
    match answer {
        Answer::Accept => {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                    "access_token": "test-quote-token-not-real-72", "token_type": "Bearer", "expires_in": 86_400
                })))
                .mount(&server)
                .await;
            Mock::given(method("GET"))
                .respond_with(ResponseTemplate::new(503))
                .mount(&server)
                .await;
            server
        }
        Answer::Reject => plain(401).await,
        Answer::Down => plain(503).await,
    }
}

async fn anthropic(answer: Answer) -> MockServer {
    match answer {
        Answer::Accept => {
            let server = MockServer::start().await;
            Mock::given(method("GET"))
                .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                    "data": [{"id": "claude-sonnet-5-5", "type": "model", "display_name": "test", "created_at": "2026-01-01T00:00:00Z"}],
                    "has_more": false, "first_id": "claude-sonnet-5-5", "last_id": "claude-sonnet-5-5"
                })))
                .mount(&server)
                .await;
            Mock::given(any())
                .respond_with(ResponseTemplate::new(503))
                .mount(&server)
                .await;
            server
        }
        Answer::Reject => plain(401).await,
        Answer::Down => plain(503).await,
    }
}

async fn news(answer: Answer) -> MockServer {
    match answer {
        Answer::Accept => {
            let server = MockServer::start().await;
            Mock::given(any())
                .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                    "lastBuildDate": "Mon, 26 Sep 2016 07:50:00 +0900", "total": 0, "start": 1, "display": 0, "items": []
                })))
                .mount(&server)
                .await;
            server
        }
        Answer::Reject => plain(401).await,
        Answer::Down => plain(503).await,
    }
}

impl Outside {
    /// 증권사 · AI · 뉴스가 `broker` · `ai` · `naver`로 답한다. 자료는 늘 503.
    pub async fn start(broker: Answer, ai: Answer, naver: Answer) -> Outside {
        let servers = vec![
            toss(broker).await,
            anthropic(ai).await,
            plain(503).await,
            news(naver).await,
        ];
        Outside { servers }
    }

    /// 아무것도 받아들이지 않는다(시연용 — 요청이 오면 안 된다).
    pub async fn silent() -> Outside {
        Outside::start(Answer::Down, Answer::Down, Answer::Down).await
    }

    /// 넷이 받은 요청 수의 합.
    pub async fn total(&self) -> usize {
        let mut total = 0;
        for at in 0..self.servers.len() {
            total += self.requests(at).await.len();
        }
        total
    }

    /// 그 자리가 받은 요청들(받은 순서).
    pub async fn requests(&self, at: usize) -> Vec<Request> {
        self.servers[at]
            .received_requests()
            .await
            .unwrap_or_default()
    }

    /// 증권사 자리가 받은 요청의 (방법, 경로)들.
    pub async fn broker_calls(&self) -> Vec<(String, String)> {
        self.requests(0)
            .await
            .iter()
            .map(|r| (r.method.to_string(), r.url.path().to_string()))
            .collect()
    }

    fn uri(&self, at: usize) -> String {
        self.servers[at].uri()
    }
}

fn args(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| (*s).to_string()).collect()
}

fn no_env(_name: &str) -> Option<String> {
    None
}

/// `run --demo`의 설정(임시 보관 폴더 · 빈 포트 · 짧은 간격).
pub fn demo_settings(home: &Path) -> Settings {
    let home = home.to_str().unwrap();
    let list = [
        "--home",
        home,
        "--port",
        "0",
        "--interval-ms",
        "50",
        "--demo",
    ];
    Settings::parse(&args(&list), &no_env, None).expect("설정을 읽어야 한다")
}

/// 시연이 아닌 `run`의 설정: 종목 하나 · 짧은 간격 · 짧은 물러서기.
pub fn live_settings(home: &Path) -> Settings {
    let home = home.to_str().unwrap();
    let list = [
        "--home",
        home,
        "--port",
        "0",
        "--symbols",
        "005930",
        "--interval-ms",
        "50",
        "--retry-initial-ms",
        "100",
        "--retry-max-ms",
        "200",
    ];
    Settings::parse(&args(&list), &no_env, None).expect("설정을 읽어야 한다")
}

/// 실행 파일 조립에서 바깥 주소 · CLI 찾는 자리만 바꾼다.
pub fn assembly(settings: &Settings, outside: &Outside) -> RunAssembly {
    let mut assembly = run_assembly(settings, None);
    assembly.wiring.cli_locations = Vec::new();
    assembly.wiring.upstreams = Upstreams {
        toss: TossConfig {
            base_url: outside.uri(0),
            timeout: Duration::from_secs(3),
            token_failure_cooldown: Duration::ZERO,
            ..TossConfig::default()
        },
        anthropic: AnthropicConfig {
            base_url: outside.uri(1),
            timeout: Duration::from_secs(3),
            max_retries: 0,
            retry_base_delay: Duration::from_millis(1),
        },
    };
    if let Some(team) = assembly.launch.team.as_mut() {
        let data = outside.uri(2);
        team.data = DataConfig::new(data.clone(), data.clone(), data);
        team.data.timeout = Duration::from_secs(3);
        team.news = NaverNewsConfig::new(outside.uri(3)).with_timeout(Duration::from_secs(3));
    }
    assembly.demo_ai_pause = Duration::from_millis(20);
    assembly
}

fn origin_of(addr: SocketAddr) -> String {
    format!("http://127.0.0.1:{}", addr.port())
}

/// 세션을 받아 같은 출처에서 부른다(`body`가 있으면 POST). (상태, JSON 본문).
pub async fn call(addr: SocketAddr, path: &str, body: Option<&Value>) -> (u16, Value) {
    let origin = origin_of(addr);
    let client = reqwest::Client::new();
    let reply = client
        .post(format!("{origin}{}", pi_gate::SESSION_PATH))
        .header("Origin", &origin)
        .send()
        .await
        .expect("세션 요청이 닿아야 한다");
    let cookie = reply
        .headers()
        .get("set-cookie")
        .expect("세션 쿠키가 있어야 한다")
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .trim()
        .to_string();
    let request = match body {
        Some(body) => client.post(format!("{origin}{path}")).json(body),
        None => client.get(format!("{origin}{path}")),
    };
    let reply = request
        .header("Origin", &origin)
        .header("Cookie", cookie)
        .send()
        .await
        .expect("요청이 닿아야 한다");
    let status = reply.status().as_u16();
    (status, reply.json().await.unwrap_or(Value::Null))
}

/// 시험용 증권사 키 값(실제 값이 아니다).
pub const BROKER_ID: &str = "test-client-id-0072";
/// 시험용 증권사 비밀(실제 값이 아니다).
pub const BROKER_SECRET: &str = "test-client-secret-not-real-72";
/// 시험용 AI 키(실제 값이 아니다).
pub const AI_KEY: &str = "test-ai-key-not-real-0072";
/// 시험용 네이버 뉴스 아이디(실제 값이 아니다).
pub const NAVER_ID: &str = "test-naver-id-0072";
/// 시험용 네이버 뉴스 비밀(실제 값이 아니다).
pub const NAVER_SECRET: &str = "test-naver-secret-0072";

/// 증권사 키 등록 본문.
pub fn broker_body() -> Value {
    json!({"item": "broker", "fields": {"client_id": BROKER_ID, "client_secret": BROKER_SECRET}})
}

/// AI API 키 등록 본문.
pub fn ai_body() -> Value {
    json!({"item": "ai_api_key", "fields": {"api_key": AI_KEY}})
}

/// 네이버 뉴스 키 등록 본문.
pub fn naver_body() -> Value {
    json!({"item": "naver_news", "fields": {"client_id": NAVER_ID, "client_secret": NAVER_SECRET}})
}

/// `pred`가 참이 될 때까지(최대 [`STEP`]) 짧게 기다린다. 참이 됐는가.
pub async fn eventually<F, Fut>(mut pred: F) -> bool
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    let until = tokio::time::Instant::now() + STEP;
    while tokio::time::Instant::now() < until {
        if pred().await {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    false
}

/// 명령을 실제로 돌리지 않고 실패로 답하는 가짜 실행기(진단이 서비스를 볼 때).
pub struct NoCommands;

impl price_inha::CommandRunner for NoCommands {
    fn run(
        &self,
        _command: &price_inha::CommandLine,
    ) -> std::io::Result<price_inha::CommandOutput> {
        Ok(price_inha::CommandOutput {
            success: false,
            stdout: String::new(),
            stderr: String::new(),
        })
    }
}
