//! round67 시험이 함께 쓰는 도우미: "실제" 주소 자리의 로컬 가짜 서버 · 실행 파일 조립 굽히기 · 가짜 실행기.
//! round26의 공용 장치를 본떠 두고(그 폴더는 고치지 않는다) 67차에 맞게 줄였다.

use std::path::Path;
use std::sync::Mutex;
use std::time::Duration;

use pi_ai::AnthropicConfig;
use pi_data::{DataConfig, NaverNewsConfig};
use pi_market::TossConfig;
use price_inha::{
    CommandLine, CommandOutput, CommandRunner, RunAssembly, Settings, Upstreams, run_assembly,
};
use wiremock::matchers::any;
use wiremock::{Mock, MockServer, ResponseTemplate};

/// 한 걸음을 기다리는 가장 긴 시간.
pub const STEP: Duration = Duration::from_secs(15);
/// 바깥 요청이 없는지 지켜보는 시간.
pub const QUIET: Duration = Duration::from_millis(1_500);

/// 무엇을 물어도 503으로 답하고 받은 요청을 세는 로컬 가짜 서버.
async fn counting_server() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(any())
        .respond_with(ResponseTemplate::new(503))
        .mount(&server)
        .await;
    server
}

/// 증권사 · AI · 자료 · 뉴스 주소 자리의 가짜 서버들.
pub struct Outside {
    servers: Vec<MockServer>,
}

impl Outside {
    pub async fn start() -> Outside {
        let mut servers = Vec::new();
        for _ in 0..4 {
            servers.push(counting_server().await);
        }
        Outside { servers }
    }

    /// 넷이 받은 요청 수의 합.
    pub async fn total(&self) -> usize {
        let mut total = 0;
        for server in &self.servers {
            total += server
                .received_requests()
                .await
                .map_or(0, |list| list.len());
        }
        total
    }

    fn uri(&self, at: usize) -> String {
        self.servers[at].uri()
    }
}

/// 환경 변수가 하나도 없다.
pub fn no_env(_name: &str) -> Option<String> {
    None
}

/// 글자 조각들을 인자로.
pub fn args(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| (*s).to_string()).collect()
}

/// 임시 보관 폴더 · 빈 포트 · 짧은 간격의 `--demo` 설정(실행 파일의 `run --demo`와 같은 조립).
pub fn demo_run_settings(home: &Path) -> Settings {
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

/// 부른 명령 줄을 적어 두기만 하는 가짜 실행기(실제 브라우저를 띄우지 않는다).
#[derive(Default)]
pub struct Recorder {
    pub calls: Mutex<Vec<CommandLine>>,
}

impl Recorder {
    pub fn calls(&self) -> Vec<CommandLine> {
        self.calls.lock().unwrap().clone()
    }
}

impl CommandRunner for Recorder {
    fn run(&self, command: &CommandLine) -> std::io::Result<CommandOutput> {
        self.calls.lock().unwrap().push(command.clone());
        Ok(CommandOutput {
            success: true,
            stdout: String::new(),
            stderr: String::new(),
        })
    }
}
