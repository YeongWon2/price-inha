//! 구독용 공급자: 공식 Claude Code CLI를 "도구 없는 한 번 호출"로 부른다(ai.md 2차).
//!
//! 원본은 OpenClaw `claude-cli`의 곁가지 한 번 호출이다(`docs/research/cli-backend.md` 2.5 · 5장).
//! 구독 토큰을 직접 다루지 않는다. 로그인은 CLI가 쥔다. 세션 이어 가기 · 오래 사는 프로세스 ·
//! MCP 도구 넘기기는 하지 않는다.

mod check;
mod command;
mod groups;
mod os;
mod output;
mod process;
mod spawn;

use std::fmt;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use tokio::sync::{Mutex, Semaphore};

use self::groups::Groups;
use crate::error::AiError;
use crate::types::{AiProvider, CompletionRequest, CompletionResponse};

/// 동시에 띄우는 CLI 프로세스 수의 기본값.
pub const CLI_DEFAULT_MAX_CONCURRENCY: usize = 2;

/// 표준 입력 상한(바이트). 시스템 글과 표준 입력을 합쳐 넘으면 띄우지 않고
/// [`AiError::InvalidRequest`]를 낸다.
pub const CLI_MAX_STDIN_BYTES: usize = 10_000_000;

/// 표준 출력 · 표준 오류를 읽는 상한의 기본값(바이트).
pub const CLI_DEFAULT_MAX_OUTPUT_BYTES: usize = 8 * 1024 * 1024;

/// 끝낼 때 `SIGTERM` 뒤 `SIGKILL`까지 기다리는 유예의 기본값.
pub const CLI_DEFAULT_KILL_GRACE: Duration = Duration::from_secs(1);

/// 끝낼 때 표준 입력을 닫은 뒤 `SIGTERM`을 보내기 전까지 스스로 끝나기를 기다리는 시간
/// (원본 `cli-transport.ts`의 0.5초, `cli-backend.md` 2.7 — ai.md 4차 4).
pub const CLI_STDIN_CLOSE_WAIT: Duration = Duration::from_millis(500);

/// `--version` 확인의 제한.
pub const CLI_VERSION_TIMEOUT: Duration = Duration::from_secs(5);

/// `auth status --json` 확인의 제한.
pub const CLI_AUTH_TIMEOUT: Duration = Duration::from_secs(10);

/// 쓸 수 있는 가장 낮은 CLI 버전(`--permission-prompts`가 이 버전부터다).
pub const CLI_MIN_VERSION: CliVersion = CliVersion {
    major: 2,
    minor: 1,
    patch: 259,
};

/// 한 번 호출의 제한 설정 JSON(`--settings`로 넘긴다, `cli-backend.md` 2.5).
pub const CLI_RESTRICTED_SETTINGS: &str = r#"{"disableAllHooks":true,"enabledPlugins":{},"autoMemoryEnabled":false,"claudeMdExcludes":["**/CLAUDE.md","**/CLAUDE.local.md","**/.claude/rules/**"]}"#;

/// CLI 공급자 설정. 모두 인자로 받는다(라이브러리는 환경 변수를 읽지 않는다).
///
/// 추가 인자를 받는 칸은 없다. `Debug`에 `base_env`의 값이 찍히지 않는다.
#[derive(Clone)]
pub struct ClaudeCliConfig {
    /// 실행 파일의 절대 경로. 없으면 [`AiError::NotConfigured`]이고 아무것도 띄우지 않는다(V31).
    pub command: Option<PathBuf>,
    /// 전용 로그인 폴더. 있으면 자식에 `CLAUDE_CONFIG_DIR`로 넣는다.
    pub config_dir: Option<PathBuf>,
    /// 자식의 작업 폴더(비어 있는 전용 폴더). 시스템 글 임시 파일도 여기에 쓴다.
    pub work_dir: PathBuf,
    /// 자식에게 줄 환경의 전부. 부모 환경은 물려주지 않는다.
    pub base_env: Vec<(String, String)>,
    /// 호출 하나의 전체 제한. 줄에서 기다린 시간도 든다.
    pub timeout: Duration,
    /// 동시에 산 자식 수의 상한.
    pub max_concurrency: usize,
    /// 표준 출력 · 표준 오류 각각을 읽는 상한(바이트).
    pub max_output_bytes: usize,
    /// 끝낼 때 `SIGTERM` 뒤 `SIGKILL`까지의 유예.
    pub kill_grace: Duration,
    /// `--version` 확인의 제한.
    pub version_timeout: Duration,
    /// `auth status --json` 확인의 제한.
    pub auth_timeout: Duration,
}

impl ClaudeCliConfig {
    /// 필수 값으로 설정을 만든다. 나머지는 기본값이다
    /// (동시 실행 [`CLI_DEFAULT_MAX_CONCURRENCY`], 출력 상한 [`CLI_DEFAULT_MAX_OUTPUT_BYTES`],
    /// 유예 [`CLI_DEFAULT_KILL_GRACE`], 확인 제한 [`CLI_VERSION_TIMEOUT`] · [`CLI_AUTH_TIMEOUT`],
    /// 로그인 폴더 없음).
    pub fn new(
        command: Option<PathBuf>,
        work_dir: PathBuf,
        base_env: Vec<(String, String)>,
        timeout: Duration,
    ) -> Self {
        Self {
            command,
            config_dir: None,
            work_dir,
            base_env,
            timeout,
            max_concurrency: CLI_DEFAULT_MAX_CONCURRENCY,
            max_output_bytes: CLI_DEFAULT_MAX_OUTPUT_BYTES,
            kill_grace: CLI_DEFAULT_KILL_GRACE,
            version_timeout: CLI_VERSION_TIMEOUT,
            auth_timeout: CLI_AUTH_TIMEOUT,
        }
    }
}

impl fmt::Debug for ClaudeCliConfig {
    /// `base_env`는 이름만 찍고 값은 찍지 않는다.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let env_names: Vec<&str> = self.base_env.iter().map(|(k, _)| k.as_str()).collect();
        f.debug_struct("ClaudeCliConfig")
            .field("command", &self.command)
            .field("config_dir", &self.config_dir)
            .field("work_dir", &self.work_dir)
            .field("base_env", &env_names)
            .field("timeout", &self.timeout)
            .field("max_concurrency", &self.max_concurrency)
            .field("max_output_bytes", &self.max_output_bytes)
            .field("kill_grace", &self.kill_grace)
            .field("version_timeout", &self.version_timeout)
            .field("auth_timeout", &self.auth_timeout)
            .finish()
    }
}

/// CLI 버전(`주.부.수`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CliVersion {
    /// 주 번호.
    pub major: u32,
    /// 부 번호.
    pub minor: u32,
    /// 수 번호.
    pub patch: u32,
}

/// 설치 · 로그인 확인 결과. 설정 화면이 그대로 보여 줄 수 있는 상태 하나.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CliStatus {
    /// 설치돼 있고 구독(`claude.ai`)으로 로그인돼 있다.
    LoggedIn {
        /// 설치된 버전.
        version: CliVersion,
        /// `authMethod` 값(늘 `claude.ai`).
        auth_method: String,
        /// `subscriptionType` 값(`pro` · `max` 등). 없으면 `None`.
        subscription_type: Option<String>,
    },
    /// 로그인돼 있지 않다(끝 코드 1 또는 `loggedIn` 거짓).
    NotLoggedIn,
    /// 실행 파일이 없다(설정에 없거나 그 경로에 없다).
    NotInstalled,
    /// 확인이 제한 안에 끝나지 않았다. 또는 앞 확인의 프로세스가 아직 살아 있어
    /// 새로 띄우지 않았다(ai.md 4차 2). 서버는 "확인하지 못함"으로 보인다.
    TimedOut,
    /// 확인 결과를 읽지 못했다(JSON이 아님, 버전 모양이 아님 등).
    Unreadable,
    /// 로그인은 됐지만 구독이 아니다(`authMethod`가 `claude.ai`가 아님).
    NotSubscription {
        /// 받은 `authMethod` 값.
        auth_method: String,
    },
    /// 설치된 버전이 [`CLI_MIN_VERSION`]보다 낮다.
    TooOld {
        /// 설치된 버전.
        version: CliVersion,
    },
}

/// 확인의 자세한 결과(ai.md 5차 5). [`ClaudeCliProvider::check_detailed`]가 돌려준다.
///
/// [`CliStatus::TimedOut`] 하나로 묶이던 "앞 확인이 아직 돈다"를 따로 가른다.
/// [`ClaudeCliProvider::check`]는 지금처럼 [`CliStatus`]만 돌려주고, 이 갈래를 [`CliStatus::TimedOut`]으로 보인다.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CliCheck {
    /// 이번 확인이 결과를 냈다(이번 확인 자신이 제한을 넘긴 [`CliStatus::TimedOut`]도 여기다).
    Status(CliStatus),
    /// 앞 확인의 프로세스가 아직 살아 있어 새로 띄우지 않았다(ai.md 4차 2 · 5차 5).
    PreviousProbeAlive,
}

/// 이 모듈이 작업 폴더에 두는 임시 파일(시스템 글 임시 파일 `.pi-ai-system-…`)의 이름이면 참,
/// 나머지는 거짓(ai.md 5차 7).
///
/// 서버가 비정상으로 꺼진 뒤 남은 그 파일 때문에 작업 폴더가 "비어 있지 않음"이 되지 않게
/// 조립(gate 8차)이 이 함수로 가른다. `name`은 경로가 아니라 파일 이름 하나다.
pub fn is_own_work_file(name: &std::ffi::OsStr) -> bool {
    name.to_str().is_some_and(command::is_system_file_name)
}

/// 공식 Claude Code CLI를 부르는 공급자. 구독은 이것으로 쓴다(D4).
pub struct ClaudeCliProvider {
    config: ClaudeCliConfig,
    /// 동시에 산 자식 수를 묶는 자리표.
    slots: Arc<Semaphore>,
    /// 살아 있는 자식 그룹의 명부. 끝내기가 본다.
    groups: Arc<Groups>,
    /// 설치 · 로그인 확인을 한 번에 하나만 돌리는 차례표.
    check_turn: Mutex<()>,
    /// 확인 프로세스가 아직 살아 있는가(앞 확인을 버렸어도). 살아 있으면 새로 띄우지 않는다.
    probe_alive: Arc<check::ProbeAlive>,
    /// 나가는 글의 가리기 자리(ai.md 7차 1). 기본은 [`crate::NoRedaction`].
    redactor: Arc<dyn crate::Redactor>,
}

impl ClaudeCliProvider {
    /// 설정으로 공급자를 만든다. 아무것도 띄우지 않는다.
    pub fn new(config: ClaudeCliConfig) -> Self {
        let slots = Arc::new(Semaphore::new(config.max_concurrency.max(1)));
        Self {
            config,
            slots,
            groups: Arc::new(Groups::default()),
            check_turn: Mutex::new(()),
            probe_alive: Arc::default(),
            redactor: Arc::new(crate::NoRedaction),
        }
    }

    /// 나가는 글의 가리기 자리를 끼운 공급자를 돌려준다(ai.md 7차 1, E38 · V47).
    ///
    /// 끼운 뒤에는 CLI에 주는 글 — 표준 입력(대화 글)과 시스템 글 임시 파일의 내용 — 이
    /// 쓰기 전에 `redactor`를 거친다. 인자(`--model` 등)와 환경은 바꾸지 않는다.
    /// 끼우지 않으면 [`crate::NoRedaction`]과 같다(글을 그대로 준다).
    #[must_use]
    pub fn with_redactor(self, redactor: Arc<dyn crate::Redactor>) -> Self {
        Self { redactor, ..self }
    }

    /// 설치(`--version`, 5초)와 로그인(`auth status --json`, 10초)을 차례로 확인한다.
    /// 모델을 부르지 않는다.
    ///
    /// 한 공급자에서 확인은 한 번에 하나만 돈다(두 번째는 첫 번째가 끝나기를 기다린다).
    /// 제한을 넘긴 확인 명령의 표준 출력은 뒤에서 끝까지 읽어 버려 그 명령이 스스로 끝날 수 있게 하되,
    /// 끝나지 않으면 프로세스 묶음째 끝내고 거둔다(ai.md 4차 7). 앞 확인의 프로세스가 아직
    /// 살아 있으면(이 함수를 부른 쪽이 기다리기를 그만둔 경우 등) 새로 띄우지 않고
    /// [`CliStatus::TimedOut`]을 돌려준다(4차 2). 그래서 확인 프로세스는 쌓이지 않는다.
    pub async fn check(&self) -> CliStatus {
        match self.check_detailed().await {
            CliCheck::Status(status) => status,
            CliCheck::PreviousProbeAlive => CliStatus::TimedOut,
        }
    }

    /// [`Self::check`]와 같은 확인을 하되, "앞 확인의 프로세스가 아직 살아 있어 새로 띄우지 않음"을
    /// [`CliCheck::PreviousProbeAlive`]로 가른다(ai.md 5차 5).
    ///
    /// 확인 제한이 아주 커도(`Duration::MAX`) 공황하지 않는다: 끝 시각을 넘치지 않게 세고 넘치면
    /// 제한 없음으로 다루며, 그 셈은 자식을 띄우기 전에 한다(5차 1).
    /// 확인이 정상으로 끝나도 그 프로세스 묶음에 남은 손주를 끝낸다(5차 4).
    /// 남은 손주가 끝나기를 기다리는 시간도 확인의 제한 시간 안에 든다: 그 기다림 때문에 제한을 넘으면
    /// 결과는 [`CliStatus::TimedOut`]이다(ai.md 7차 4).
    ///
    /// init(좀비를 거두는 1번 프로세스)이 없는 컨테이너에서는 끝난 손주가 좀비로 남아 그룹이 비지 않으므로,
    /// 매번 끝낼 때의 유예([`ClaudeCliConfig::kill_grace`])를 끝까지 기다린다. 상한이 있어 자원 문제는 아니다
    /// (ai.md 7차 6). 호출([`AiProvider::complete`])을 끝낼 때도 같다.
    pub async fn check_detailed(&self) -> CliCheck {
        let _turn = self.check_turn.lock().await;
        check::check(&self.config, &self.probe_alive).await
    }

    /// 공급자를 끝낸다. 서버가 꺼질 때 부른다.
    ///
    /// 새 호출을 받지 않게 하고(이후 [`AiProvider::complete`]는 띄우지 않고
    /// [`AiError::Connection`]), 살아 있는 모든 자식 그룹의 표준 입력을 닫고
    /// [`CLI_STDIN_CLOSE_WAIT`] 동안 스스로 끝나기를 기다린 뒤, 남은 그룹에
    /// `SIGTERM` → 유예([`ClaudeCliConfig::kill_grace`]) → `SIGKILL`을 보내고, 자식이 거둬질 때까지 기다린다.
    ///
    /// 설치 · 로그인 확인 프로세스도 묶음째 끝내고 거둔 뒤에 돌아온다(ai.md 5차 3,
    /// `cli-backend.md` 5.6-3). 꺼지는 중이 아니면 확인을 도중에 죽이지 않는 것(5.6-5)은 그대로다.
    ///
    /// 끝내기와 겹쳐 띄우는 중이던 호출도 남기지 않는다(ai.md 4차 1): 자리표를 받은 호출은
    /// 명부에 세어 끝내기가 기다리거나, 띄운 뒤 등록이 거부되면 그 호출이 돌아가기 전에 자식을 끝내고 거둔다.
    pub async fn shutdown(&self) {
        // 자리를 기다리던 호출도 띄우지 않고 연결 실패로 끝난다.
        self.slots.close();
        tokio::join!(
            self.groups.shutdown(self.config.kill_grace),
            self.probe_alive.shutdown(),
        );
    }
}

/// 가리기 전 길이를 거절하는 상한은 [`CLI_MAX_STDIN_BYTES`]의 몇 배인가(ai.md 8차 3).
/// 가리기가 글을 이만큼 줄이는 일은 없으므로, 이보다 큰 글은 가리기 전에 거절해도 판단이 같다.
const PRE_REDACT_FACTOR: usize = 4;

fn too_large() -> AiError {
    AiError::InvalidRequest {
        message: format!("시스템 글과 대화 글이 합쳐 {CLI_MAX_STDIN_BYTES}바이트를 넘습니다"),
    }
}

#[async_trait]
impl AiProvider for ClaudeCliProvider {
    /// CLI를 한 번 띄워 응답을 받는다. 어떤 오류도 다시 부르지 않는다.
    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse, AiError> {
        if self.groups.is_closed() {
            return Err(AiError::Connection);
        }
        let Some(program) = self.config.command.clone() else {
            return Err(AiError::NotConfigured);
        };
        // 가리기 전 길이가 상한의 4배를 넘으면 가리지 않고 거절한다(복사를 줄인다, ai.md 8차 3).
        // 그 아래에서는 가린 뒤의 길이로 판단한다.
        let raw_len = request.messages.iter().fold(
            request.system.as_ref().map_or(0, String::len),
            |sum, message| sum.saturating_add(message.content.len()),
        );
        if raw_len > CLI_MAX_STDIN_BYTES.saturating_mul(PRE_REDACT_FACTOR) {
            return Err(too_large());
        }
        // 가리기는 크기 상한 검사 전에 한다: 상한은 실제로 나가는 가린 글에 건다(ai.md 7차 1).
        // 표준 입력과 시스템 글 임시 파일은 모두 이 가린 요청에서 만든다.
        let request = crate::redact::redact_request(self.redactor.as_ref(), request);
        let input = command::stdin_text(&request.messages);
        let system_len = request.system.as_ref().map_or(0, String::len);
        if input.len().saturating_add(system_len) > CLI_MAX_STDIN_BYTES {
            return Err(too_large());
        }
        // 작업 폴더가 없으면 띄우지 않는다. 띄울 때의 `NotFound`는 실행 파일이 없다는 뜻만 남는다.
        if !self.config.work_dir.is_dir() {
            return Err(AiError::Connection);
        }
        // 끝 시각은 띄우기 전에 센다. 넘치면 제한 없음(ai.md 5차 1).
        let deadline = tokio::time::Instant::now().checked_add(self.config.timeout);
        let call = process::run_once(
            &self.config,
            &self.slots,
            &self.groups,
            &program,
            &request,
            input,
        );
        match deadline {
            Some(deadline) => tokio::time::timeout_at(deadline, call)
                .await
                .unwrap_or(Err(AiError::Timeout)),
            None => call.await,
        }
    }
}
