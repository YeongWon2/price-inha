//! 구독용 공급자: 공식 Claude Code CLI를 "도구 없는 한 번 호출"로 부른다(ai.md 2차).
//!
//! 원본은 OpenClaw `claude-cli`의 곁가지 한 번 호출이다(`docs/research/cli-backend.md` 2.5 · 5장).
//! 구독 토큰을 직접 다루지 않는다. 로그인은 CLI가 쥔다. 세션 이어 가기 · 오래 사는 프로세스 ·
//! MCP 도구 넘기기는 하지 않는다.

use std::fmt;
use std::path::PathBuf;
use std::time::Duration;

use async_trait::async_trait;

use crate::error::AiError;
use crate::types::{AiProvider, CompletionRequest, CompletionResponse};

/// 동시에 띄우는 CLI 프로세스 수의 기본값.
pub const CLI_DEFAULT_MAX_CONCURRENCY: usize = 2;

/// 표준 입력 상한(바이트). 넘으면 띄우지 않고 [`AiError::InvalidRequest`]를 낸다.
pub const CLI_MAX_STDIN_BYTES: usize = 10_000_000;

/// 표준 출력 · 표준 오류를 읽는 상한의 기본값(바이트).
pub const CLI_DEFAULT_MAX_OUTPUT_BYTES: usize = 8 * 1024 * 1024;

/// 끝낼 때 `SIGTERM` 뒤 `SIGKILL`까지 기다리는 유예의 기본값.
pub const CLI_DEFAULT_KILL_GRACE: Duration = Duration::from_secs(1);

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
        let _ = (command, work_dir, base_env, timeout);
        todo!()
    }
}

impl fmt::Debug for ClaudeCliConfig {
    /// `base_env`는 이름만 찍고 값은 찍지 않는다.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let _ = f;
        todo!()
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
    /// 확인이 제한 안에 끝나지 않았다. 확인 프로세스는 죽이지 않았다.
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

/// 공식 Claude Code CLI를 부르는 공급자. 구독은 이것으로 쓴다(D4).
pub struct ClaudeCliProvider {
    config: ClaudeCliConfig,
}

impl ClaudeCliProvider {
    /// 설정으로 공급자를 만든다. 아무것도 띄우지 않는다.
    pub fn new(config: ClaudeCliConfig) -> Self {
        let _ = config;
        todo!()
    }

    /// 설치(`--version`, 5초)와 로그인(`auth status --json`, 10초)을 차례로 확인한다.
    /// 모델을 부르지 않는다. 로그인 확인이 제한을 넘겨도 그 프로세스를 죽이지 않는다.
    pub async fn check(&self) -> CliStatus {
        let _ = &self.config;
        todo!()
    }
}

#[async_trait]
impl AiProvider for ClaudeCliProvider {
    /// CLI를 한 번 띄워 응답을 받는다. 어떤 오류도 다시 부르지 않는다.
    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse, AiError> {
        let _ = (&self.config, request);
        todo!()
    }
}
