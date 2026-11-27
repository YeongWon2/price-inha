//! 65차 시험의 공용 도우미: 실행 파일의 `--demo` 조립으로 띄우기 · 시계 · 바깥 주소 자리의 로컬 가짜 서버 · 세션 ·
//! 요청 · 가짜 바깥 명령 실행기 · 서비스 재료. round61의 도우미를 본떴다(그 폴더는 고치지 않는다).

use std::io;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, FixedOffset, NaiveDate, TimeZone, Utc};
use pi_ai::AnthropicConfig;
use pi_data::{DataConfig, NaverNewsConfig};
use pi_market::{Clock, Symbol, TossConfig};
use pi_secrets::{
    Credentials, KeyKind, KeySlot, ProviderKeys, SecretStore, Verifier, VerifyFailure,
};
use price_inha::{
    App, CommandLine, CommandOutput, CommandRunner, Os, RunAssembly, ServiceSetup, Settings,
    Upstreams, run_assembly, save_watchlist,
};
use serde_json::Value;
use tempfile::TempDir;
use wiremock::matchers::any;
use wiremock::{Mock, MockServer, ResponseTemplate};

/// 한 걸음을 기다리는 가장 긴 시간.
pub const STEP: Duration = Duration::from_secs(20);

// ───────────── 시계 ─────────────

/// 시험이 정하는 시계. 자는 것은 짧게만 잔다.
pub struct TestClock(AtomicI64);

#[async_trait]
impl Clock for TestClock {
    fn now(&self) -> DateTime<Utc> {
        Utc.timestamp_millis_opt(self.0.load(Ordering::SeqCst))
            .unwrap()
    }

    async fn sleep(&self, duration: Duration) {
        tokio::time::sleep(duration.min(Duration::from_millis(10))).await;
    }
}

/// 2026-11-20 금요일 17:00 한국(장 마감 뒤 — 첫 tick이 논의를 열지 않는다).
pub fn after_close() -> DateTime<Utc> {
    let date = NaiveDate::from_ymd_opt(2026, 11, 20).unwrap();
    FixedOffset::east_opt(9 * 3600)
        .unwrap()
        .from_local_datetime(&date.and_hms_opt(17, 0, 0).unwrap())
        .unwrap()
        .with_timezone(&Utc)
}

// ───────────── 가짜 바깥 명령 · 서비스 재료 ─────────────

/// 정의에 실릴 실행 파일 경로(가짜. 실제로 띄우지 않는다).
pub const EXE: &str = "/opt/price-inha/bin/price-inha";

/// 정한 표준 출력으로 답하는 가짜 실행기(실제 서비스 관리자를 부르지 않는다).
pub struct ScriptedRunner {
    answer: Box<dyn Fn(&CommandLine) -> CommandOutput + Send + Sync>,
}

impl ScriptedRunner {
    /// 모든 명령에 성공 · 빈 출력으로 답한다.
    pub fn ok() -> ScriptedRunner {
        ScriptedRunner::answering(|_| CommandOutput {
            success: true,
            ..CommandOutput::default()
        })
    }

    /// `answer`로 답한다.
    pub fn answering(
        answer: impl Fn(&CommandLine) -> CommandOutput + Send + Sync + 'static,
    ) -> ScriptedRunner {
        ScriptedRunner {
            answer: Box::new(answer),
        }
    }
}

impl CommandRunner for ScriptedRunner {
    fn run(&self, command: &CommandLine) -> io::Result<CommandOutput> {
        Ok((self.answer)(command))
    }
}

/// 시험용 서비스 재료. 정의 폴더는 `definitions`, 실행 파일은 [`EXE`].
pub fn setup(os: Os, home: &Path, definitions: &Path, run_args: &[&str]) -> ServiceSetup {
    ServiceSetup {
        os,
        exe: PathBuf::from(EXE),
        home: home.to_path_buf(),
        definition_dir: definitions.to_path_buf(),
        uid: 501,
        path_env: "/usr/local/bin:/usr/bin:/bin".to_string(),
        run_args: run_args.iter().map(|arg| (*arg).to_string()).collect(),
    }
}

/// 보관 폴더 `home`의 설정(루프백, 포트 0, 환경 변수 없음).
pub fn plain_settings(home: &Path) -> Settings {
    let args: Vec<String> = ["--home", home.to_str().unwrap(), "--port", "0"]
        .iter()
        .map(|arg| (*arg).to_string())
        .collect();
    Settings::parse(&args, &no_env, None).unwrap()
}

// ───────────── "실제" 주소 자리의 가짜 서버 ─────────────

async fn refusing_server() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(any())
        .respond_with(ResponseTemplate::new(503))
        .mount(&server)
        .await;
    server
}

struct Outside {
    broker: MockServer,
    ai: MockServer,
    data: MockServer,
    news: MockServer,
}

impl Outside {
    async fn start() -> Outside {
        Outside {
            broker: refusing_server().await,
            ai: refusing_server().await,
            data: refusing_server().await,
            news: refusing_server().await,
        }
    }
}

// ───────────── 보관 폴더 · 설정 · 조립 ─────────────

fn no_env(_name: &str) -> Option<String> {
    None
}

/// 시연 모드 설정.
fn demo_settings(home: &Path) -> Settings {
    let args: Vec<String> = [
        "--home",
        home.to_str().unwrap(),
        "--port",
        "0",
        "--interval-ms",
        "20",
        "--retry-initial-ms",
        "20",
        "--retry-max-ms",
        "40",
        "--demo",
    ]
    .iter()
    .map(|s| (*s).to_string())
    .collect();
    Settings::parse(&args, &no_env, None).expect("설정을 읽어야 한다")
}

struct AcceptAll;

impl Verifier for AcceptAll {
    async fn verify(&self, _kind: KeyKind, _values: &Credentials) -> Result<(), VerifyFailure> {
        Ok(())
    }

    async fn verify_slot(
        &self,
        _slot: KeySlot,
        _values: &Credentials,
    ) -> Result<(), VerifyFailure> {
        Ok(())
    }

    async fn verify_provider(
        &self,
        _keys: ProviderKeys,
        _values: &Credentials,
    ) -> Result<(), VerifyFailure> {
        Ok(())
    }
}

async fn prepare(home: &Path) {
    let secrets = SecretStore::open(home).unwrap();
    let broker = Credentials::new()
        .with("client_id", "test-client-id-round65")
        .with("client_secret", "test-client-secret-round65");
    secrets
        .register(KeyKind::Broker, broker, &AcceptAll)
        .await
        .unwrap();
    let ai = Credentials::new().with("api_key", "test-ai-key-round65-zz");
    secrets
        .register(KeyKind::AiApiKey, ai, &AcceptAll)
        .await
        .unwrap();
    save_watchlist(home, &[Symbol::new("005930")]).unwrap();
}

fn bend(mut assembly: RunAssembly, clock: &Arc<TestClock>, outside: &Outside) -> RunAssembly {
    assembly.wiring.clock = Arc::clone(clock) as Arc<dyn Clock>;
    assembly.wiring.cli_locations = Vec::new();
    assembly.wiring.upstreams = Upstreams {
        toss: TossConfig {
            base_url: outside.broker.uri(),
            timeout: Duration::from_secs(3),
            token_failure_cooldown: Duration::ZERO,
            ..TossConfig::default()
        },
        anthropic: AnthropicConfig {
            base_url: outside.ai.uri(),
            timeout: Duration::from_secs(3),
            max_retries: 0,
            retry_base_delay: Duration::from_millis(1),
        },
    };
    if let Some(team) = assembly.launch.team.as_mut() {
        let data = outside.data.uri();
        team.data = DataConfig::new(data.clone(), data.clone(), data);
        team.data.timeout = Duration::from_secs(3);
        team.news = NaverNewsConfig::new(outside.news.uri()).with_timeout(Duration::from_secs(3));
    }
    assembly.demo_ai_pause = Duration::ZERO;
    assembly
}

/// 떠 있는 서버 하나와 그 보관 폴더 · 가짜 서버들.
pub struct Bench {
    _dir: TempDir,
    _outside: Outside,
    app: App,
}

impl Bench {
    /// 장 마감 뒤에 시연 모드로 띄운다.
    pub async fn start() -> Bench {
        let dir = tempfile::tempdir().unwrap();
        prepare(dir.path()).await;
        let clock = Arc::new(TestClock(AtomicI64::new(after_close().timestamp_millis())));
        let outside = Outside::start().await;
        let settings = demo_settings(dir.path());
        let assembly = bend(run_assembly(&settings, None), &clock, &outside);
        let app = tokio::time::timeout(STEP, App::launch_run(settings, assembly))
            .await
            .expect("서버가 시간 안에 떠야 한다")
            .expect("서버가 떠야 한다");
        Bench {
            _dir: dir,
            _outside: outside,
            app,
        }
    }

    /// 끈다.
    pub async fn close(self) {
        tokio::time::timeout(STEP, self.app.shutdown())
            .await
            .expect("시간 안에 꺼져야 한다")
            .expect("끄기가 성공해야 한다");
    }

    /// 새 세션으로 읽는다.
    pub async fn get(&self, path: &str) -> Reply {
        let addr = self.app.local_addr();
        let cookie = session(addr).await;
        let request = reqwest::Client::new()
            .get(format!("{}{path}", origin_of(addr)))
            .header("Cookie", cookie);
        send(request).await
    }

    /// 새 세션으로 JSON을 보낸다(같은 출처 · 루프백).
    pub async fn post(&self, path: &str, body: &Value) -> Reply {
        let addr = self.app.local_addr();
        let cookie = session(addr).await;
        let origin = origin_of(addr);
        let request = reqwest::Client::new()
            .post(format!("{origin}{path}"))
            .header("Cookie", cookie)
            .header("Origin", &origin)
            .json(body);
        send(request).await
    }
}

// ───────────── 요청 ─────────────

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

fn origin_of(addr: SocketAddr) -> String {
    format!("http://{addr}")
}

async fn session(addr: SocketAddr) -> String {
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
