//! 6차 작업: AI 연결을 "API 키 또는 Claude Code CLI(구독)" 가운데 사용자가 고른 하나로 잇는다.
//!
//! 근거: `docs/dev/modules/gate.md` "6차 작업", `docs/dev/decisions.md` 2026-11-15, `dev.md` D4,
//! `docs/research/cli-backend.md` 5.7.
//!
//! - 구독 OAuth 토큰을 받는 칸 · 단추 · 등록 길은 없다. 로그인은 사용자가 터미널에서 공식 CLI로 한다.
//! - **사용자가 고른 하나만 쓴다.** 둘 다 있어도 넘어가기는 없다(원본 OpenClaw 2.8).
//!   고른 것이 없으면(예전 보관 폴더) API 키를 고른 것으로 본다.
//! - 고른 방식 · 실행 파일 경로 · 전용 로그인 폴더는 비밀이 아니다. 종목처럼 보관 폴더의
//!   [`AI_CHOICE_FILE`]에 원자 쓰기로 둔다(`.env` · `auth.json`에 넣지 않는다).
//!
//! # AI 연결 고르기 (`POST` [`SETUP_AI_PATH`], 세션 + 같은 출처 + 그 컴퓨터)
//!
//! 본문(JSON) 둘 가운데 하나:
//!
//! - `{"method": "api_key"}`: API 키를 쓴다. 저장하고, API 키가 등록돼 있으면 200 `{"registered": true}`,
//!   아니면 200 `{"registered": false}`(API 키는 `POST` [`crate::SETUP_KEYS_PATH`]의
//!   [`crate::KEY_AI_API_KEY`]로 따로 등록한다).
//! - `{"method": "cli", "command": "<절대 경로>", "config_dir": "<절대 경로>" | "" | null}`:
//!   CLI를 쓴다. `config_dir`은 빠져도 되고, 비었거나 `null`이면 평소 로그인이다.
//!
//! 순서(CLI): 입력 검사(실패하면 [`SetupRefusal::InvalidInput`], 아무것도 띄우지 않고 저장하지 않는다) →
//! 시도 횟수(키 등록과 같은 제한, 넘으면 [`SetupRefusal::TooManyAttempts`], 띄우지 않는다) →
//! 고른 것을 저장 → `pi_ai::ClaudeCliProvider::check`(버전 → 로그인, 모델을 부르지 않는다).
//!
//! - [`pi_ai::CliStatus::LoggedIn`](구독 `claude.ai`)이면 200 `{"registered": true}`이고 AI 연결이 갖춰진다.
//! - 아니면 [`cli_refusal`]의 까닭으로 거부한다. 고른 것은 저장돼 있고 AI 연결은 갖춰지지 않은 것이다
//!   (앞서 갖춰졌어도 지금 확인이 아니면 갖춰지지 않은 것이다. 시작했으면 멈추고 설정이 필요하다고 알린다).
//! - 띄울 때(`crate::App::start`) 고른 것이 CLI면 한 번 확인하고 그 결과를 쓴다(모델을 부르지 않는다).
//! - 확인 결과는 마지막 것을 쥔다. 설정 상태를 읽는 것은 CLI를 띄우지 않는다.
//!
//! # 설정 상태의 `ai` 칸 (`GET` [`crate::SETUP_PATH`])
//!
//! ```json
//! "ai": {
//!   "method": "cli",
//!   "command": "/opt/homebrew/bin/claude",
//!   "config_dir": null,
//!   "cli": "logged_in",
//!   "cli_version": "2.1.316",
//!   "subscription": "max",
//!   "legacy_auth_file": false
//! }
//! ```
//!
//! - `method`: [`AiMethod::as_str`].
//! - `command`: CLI를 골랐으면 저장된 경로. 아니면 서버가 찾은 경로([`find_claude`], 화면이 칸을 채운다).
//!   못 찾으면 `null`.
//! - `config_dir`: 저장된 전용 로그인 폴더. 없으면 `null`.
//! - `cli`: 마지막 확인의 [`cli_state`]. CLI를 고르지 않았으면 `null`.
//! - `cli_version` · `subscription`: 로그인됐을 때만 글자(버전은 `주.부.수` 글자), 아니면 `null`.
//! - `legacy_auth_file`: 보관 폴더에 옛 [`LEGACY_AUTH_FILE`]이 있는가([`legacy_auth_file`]).
//! - 숫자 칸은 없다.

use std::path::{Path, PathBuf};
use std::time::Duration;

use pi_ai::{ClaudeCliConfig, CliStatus};
use serde_json::Value;

use crate::{AppError, SetupRefusal};

/// AI 연결을 고르는 경로(`POST`). 세션 · 같은 출처 · 그 컴퓨터여야 한다.
pub const SETUP_AI_PATH: &str = "/api/setup/ai";

/// 고른 AI 연결을 두는 파일 이름(보관 폴더 안). 비밀이 아니다.
/// 모양: `{"method": "api_key"}` 또는 `{"method": "cli", "command": "...", "config_dir": "..." | null}`.
pub const AI_CHOICE_FILE: &str = "ai.json";

/// 예전 구독 토큰 파일 이름(보관 폴더 안). 서버는 있는지만 보고 읽지도 · 고치지도 · 지우지도 않는다.
pub const LEGACY_AUTH_FILE: &str = "auth.json";

/// CLI 자식의 작업 폴더 이름(보관 폴더 안, 0700, 비어 있다).
pub const CLI_WORK_DIR: &str = "cli-work";

/// CLI에 넘기는 환경 변수의 허용 목록. 이것만 골라 `base_env`로 준다(이 순서).
pub const CLI_ENV_ALLOWLIST: [&str; 7] = [
    "HOME", "PATH", "LANG", "LC_ALL", "TMPDIR", "USER", "LOGNAME",
];

/// `PATH`에 없을 때 사용자 홈 아래에서 찾는 자리(홈에 이어 붙인다).
pub const CLI_USER_LOCATION: &str = ".local/bin/claude";

/// 사용자 홈 다음에 찾는 흔한 설치 위치(이 순서).
pub const CLI_COMMON_LOCATIONS: [&str; 2] = ["/opt/homebrew/bin/claude", "/usr/local/bin/claude"];

/// CLI 공급자의 동시 실행 수.
pub const CLI_CONCURRENCY: usize = 2;

/// CLI를 고르면 화면이 보여 주는 한 줄 알림.
pub const CLI_TERMS_NOTICE: &str = "구독 약관은 자동화된 이용과 증권 매매 목적 이용을 제한할 수 있습니다. 판단은 사용자에게 있습니다.";

/// 옛 [`LEGACY_AUTH_FILE`]이 있으면 화면이 AI 줄 아래에 보여 주는 한 줄 알림.
pub const LEGACY_AUTH_NOTICE: &str =
    "예전 구독 토큰 파일(auth.json)은 더 쓰지 않습니다. 필요 없으면 지워도 됩니다.";

/// 설정 상태 `ai.cli`: 설치돼 있고 구독으로 로그인돼 있다.
pub const CLI_LOGGED_IN: &str = "logged_in";
/// 설정 상태 `ai.cli`: 설치돼 있지 않다.
pub const CLI_NOT_INSTALLED: &str = "not_installed";
/// 설정 상태 `ai.cli`: 버전이 너무 낮다.
pub const CLI_TOO_OLD: &str = "too_old";
/// 설정 상태 `ai.cli`: 로그인돼 있지 않다(터미널에서 `claude auth login`).
pub const CLI_NOT_LOGGED_IN: &str = "not_logged_in";
/// 설정 상태 `ai.cli`: 로그인은 됐지만 구독이 아니다(API 키로 로그인돼 있다).
pub const CLI_NOT_SUBSCRIPTION: &str = "not_subscription";
/// 설정 상태 `ai.cli`: 확인하지 못했다(시간 초과 · 읽지 못함).
pub const CLI_UNCHECKED: &str = "unchecked";

/// AI 연결 방식.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AiMethod {
    /// API 키(`pi_ai::AnthropicProvider`).
    ApiKey,
    /// 공식 Claude Code CLI(`pi_ai::ClaudeCliProvider`, 구독).
    Cli,
}

impl AiMethod {
    /// 본문 · 파일 · 설정 상태에 적는 글자: `api_key` · `cli`.
    pub const fn as_str(self) -> &'static str {
        match self {
            AiMethod::ApiKey => "api_key",
            AiMethod::Cli => "cli",
        }
    }
}

/// 사용자가 고른 AI 연결. 비밀이 아니다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiChoice {
    /// 방식.
    pub method: AiMethod,
    /// CLI 실행 파일의 절대 경로(CLI일 때만).
    pub command: Option<PathBuf>,
    /// 전용 로그인 폴더의 절대 경로(CLI일 때만, 없으면 평소 로그인).
    pub config_dir: Option<PathBuf>,
}

impl AiChoice {
    /// API 키를 고른 것. 고른 것이 없을 때의 값이다.
    pub fn api_key() -> Self {
        Self {
            method: AiMethod::ApiKey,
            command: None,
            config_dir: None,
        }
    }

    /// 고르기 요청 본문을 읽는다(모양은 모듈 문서). 거르는 것:
    /// JSON 객체가 아님 · 모르는 `method` · 모르는 칸 · `api_key`에 딸린 칸 ·
    /// CLI인데 `command`가 없거나 글자가 아니거나 비었거나 절대 경로가 아님 ·
    /// `config_dir`가 절대 경로가 아님(빈 글자와 `null`은 없는 것) · 제어 문자가 든 경로.
    /// 모두 [`SetupRefusal::InvalidInput`]이다.
    pub fn parse(body: &[u8]) -> Result<AiChoice, SetupRefusal> {
        let _ = body;
        todo!("6차: AI 연결 고르기 본문 읽기")
    }

    /// [`AI_CHOICE_FILE`]에 쓰는 모양.
    pub fn to_json(&self) -> Value {
        todo!("6차: 고른 AI 연결의 파일 모양")
    }
}

/// 설정 상태의 `ai` 칸(모양은 모듈 문서). 비밀 값은 없다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiStatus {
    /// 고른 방식.
    pub method: AiMethod,
    /// CLI를 골랐으면 저장된 경로, 아니면 서버가 찾은 경로([`find_claude`]). 없으면 `None`.
    pub command: Option<PathBuf>,
    /// 저장된 전용 로그인 폴더.
    pub config_dir: Option<PathBuf>,
    /// 마지막 CLI 확인 결과. CLI를 고르지 않았으면 `None`.
    pub cli: Option<CliStatus>,
    /// 보관 폴더에 옛 [`LEGACY_AUTH_FILE`]이 있는가.
    pub legacy_auth_file: bool,
}

impl AiStatus {
    /// 설정 상태의 `ai` 칸(모듈 문서의 모양 그대로). 숫자 칸은 없다.
    pub fn to_json(&self) -> Value {
        todo!("6차: 설정 상태의 ai 칸")
    }
}

/// 보관 폴더의 [`AI_CHOICE_FILE`]에서 고른 것을 읽는다. 파일이 없으면 `None`.
/// 읽을 수 없는 모양이면 [`AppError::Home`](`InvalidData`)이다.
pub fn load_ai_choice(home: &Path) -> Result<Option<AiChoice>, AppError> {
    let _ = home;
    todo!("6차: 고른 AI 연결 읽기")
}

/// 고른 것을 보관 폴더의 [`AI_CHOICE_FILE`]에 원자 쓰기로 둔다(임시 파일은 남지 않는다).
pub fn save_ai_choice(home: &Path, choice: &AiChoice) -> Result<(), AppError> {
    let _ = (home, choice);
    todo!("6차: 고른 AI 연결 저장")
}

/// 환경에서 [`CLI_ENV_ALLOWLIST`]의 이름만 골라 그 순서로 준다. 없는 이름은 뺀다.
pub fn cli_base_env(env: &dyn Fn(&str) -> Option<String>) -> Vec<(String, String)> {
    let _ = env;
    todo!("6차: CLI 환경 허용 목록")
}

/// CLI 실행 파일을 찾는다: `path_var`(`:`로 나눈 폴더들)의 `claude` →
/// `user_home`/[`CLI_USER_LOCATION`] → `fixed`(보통 [`CLI_COMMON_LOCATIONS`]) 순서.
/// 실행할 수 있는 파일만 본다. 없으면 `None`. 아무것도 띄우지 않는다.
pub fn find_claude(
    path_var: Option<&str>,
    user_home: Option<&Path>,
    fixed: &[PathBuf],
) -> Option<PathBuf> {
    let _ = (path_var, user_home, fixed);
    todo!("6차: CLI 실행 파일 찾기")
}

/// CLI 자식의 작업 폴더(보관 폴더/[`CLI_WORK_DIR`])를 준비한다: 없으면 만들고, 권한은 0700이다.
pub fn prepare_cli_work_dir(home: &Path) -> Result<PathBuf, AppError> {
    let _ = home;
    todo!("6차: CLI 작업 폴더")
}

/// 고른 CLI 연결로 공급자 설정을 만든다: 동시 실행 [`CLI_CONCURRENCY`],
/// 시간 제한은 `timeout`(AI 요청 시간 제한과 같은 값), 환경은 `base_env`뿐.
pub fn cli_config(
    choice: &AiChoice,
    work_dir: PathBuf,
    base_env: Vec<(String, String)>,
    timeout: Duration,
) -> ClaudeCliConfig {
    let _ = (choice, work_dir, base_env, timeout);
    todo!("6차: CLI 공급자 설정")
}

/// 확인 결과를 설정 상태의 `ai.cli` 글자로 옮긴다([`CLI_LOGGED_IN`] 등).
/// 시간 초과와 읽지 못함은 모두 [`CLI_UNCHECKED`]다.
pub fn cli_state(status: &CliStatus) -> &'static str {
    let _ = status;
    todo!("6차: CLI 상태 글자")
}

/// 확인 결과가 "등록됨"이 아니면 그 까닭. 로그인된 구독이면 `None`.
///
/// | 확인 결과 | 까닭 |
/// |---|---|
/// | `NotInstalled` | [`SetupRefusal::CliNotInstalled`] |
/// | `TooOld` | [`SetupRefusal::CliTooOld`] |
/// | `NotLoggedIn` | [`SetupRefusal::CliNotLoggedIn`] |
/// | `NotSubscription` | [`SetupRefusal::CliNotSubscription`] |
/// | `TimedOut` · `Unreadable` | [`SetupRefusal::CliUnchecked`] |
pub fn cli_refusal(status: &CliStatus) -> Option<SetupRefusal> {
    let _ = status;
    todo!("6차: CLI 확인 실패의 까닭")
}

/// 보관 폴더에 옛 [`LEGACY_AUTH_FILE`]이 있는가. 있는지만 본다(열지 · 읽지 · 고치지 · 지우지 않는다).
/// 읽을 수 없는 권한의 파일이어도 있으면 참이다.
pub fn legacy_auth_file(home: &Path) -> bool {
    let _ = home;
    todo!("6차: 옛 auth.json 있는지 보기")
}
