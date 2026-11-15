//! pi-ai: docs/dev/modules/ai.md 를 따른다.
//!
//! AI 공급자 어댑터(A11). 에이전트 실행기는 [`AiProvider`]만 안다.
//! 구독은 자격이 아니라 공급자로 고른다: [`ClaudeCliProvider`]가 공식 Claude Code CLI를 부른다.
//! 자격(API 키)은 [`CredentialSource`]에서 호출마다 읽고,
//! 요청 본문 · 응답 · 오류 · `Debug` 어디에도 담지 않는다(D4 · E38).
//!
//! 이번 단계에서는 `pi-secrets`에 직접 기대지 않는다. `pi-secrets` 저장소를
//! [`CredentialSource`]로 잇는 일과 키 확인기 연결은 조립 단계에서 한다.

mod anthropic;
mod cli;
mod credential;
mod error;
mod redact;
mod retry;
mod types;
mod wire;

pub use anthropic::{
    ANTHROPIC_DEFAULT_CONNECT_TIMEOUT, ANTHROPIC_DEFAULT_MAX_RESPONSE_BYTES, AnthropicConfig,
    AnthropicLimits, AnthropicProvider,
};
pub use cli::{
    CLI_AUTH_TIMEOUT, CLI_DEFAULT_KILL_GRACE, CLI_DEFAULT_MAX_CONCURRENCY,
    CLI_DEFAULT_MAX_OUTPUT_BYTES, CLI_MAX_STDIN_BYTES, CLI_MIN_VERSION, CLI_RESTRICTED_SETTINGS,
    CLI_STDIN_CLOSE_WAIT, CLI_VERSION_TIMEOUT, ClaudeCliConfig, ClaudeCliProvider, CliCheck,
    CliStatus, CliVersion, is_own_work_file,
};
pub use credential::{Credential, CredentialSource, Secret};
pub use error::AiError;
pub use redact::{NoRedaction, Redactor};
pub use retry::{RETRY_AFTER_CAP, RETRY_MAX_DELAY, RetryBackoff, parse_retry_after};
pub use types::{
    AiProvider, CompletionRequest, CompletionResponse, Message, Role, StopReason, Usage,
};

/// Anthropic API의 기본 주소. 테스트는 이 값 대신 로컬 가짜 서버 주소를 넘긴다.
pub const ANTHROPIC_DEFAULT_BASE_URL: &str = "https://api.anthropic.com";

/// `anthropic-version` 헤더로 보내는 API 버전.
pub const ANTHROPIC_VERSION: &str = "2026-10-21";

/// 다시 시도 횟수의 기본값(첫 요청은 세지 않는다).
pub const DEFAULT_MAX_RETRIES: u32 = 2;
