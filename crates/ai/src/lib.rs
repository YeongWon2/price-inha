//! pi-ai: docs/dev/modules/ai.md 를 따른다.
//!
//! AI 공급자 어댑터(A11). 에이전트 실행기는 [`AiProvider`]만 안다.
//! 자격(API 키 · OAuth 토큰)은 [`CredentialSource`]에서 호출마다 읽고,
//! 요청 본문 · 응답 · 오류 · `Debug` 어디에도 담지 않는다(D4 · E38).
//!
//! 이번 단계에서는 `pi-secrets`에 직접 기대지 않는다. `pi-secrets` 저장소를
//! [`CredentialSource`]로 잇는 일과 키 확인기 연결은 조립 단계에서 한다.

use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;

/// Anthropic API의 기본 주소. 테스트는 이 값 대신 로컬 가짜 서버 주소를 넘긴다.
pub const ANTHROPIC_DEFAULT_BASE_URL: &str = "https://api.anthropic.com";

/// `anthropic-version` 헤더로 보내는 API 버전.
pub const ANTHROPIC_VERSION: &str = "2026-10-21";

/// 다시 시도 횟수의 기본값(첫 요청은 세지 않는다).
pub const DEFAULT_MAX_RETRIES: u32 = 2;

/// 가려진 비밀 값. `Debug`로 찍어도 값이 나오지 않는다.
#[derive(Clone, PartialEq, Eq)]
pub struct Secret(String);

impl Secret {
    /// 비밀 값을 감싼다.
    pub fn new(value: impl Into<String>) -> Self {
        todo!()
    }

    /// 감싼 값을 꺼낸다. 인증 헤더를 만들 때만 쓴다.
    pub fn expose(&self) -> &str {
        todo!()
    }
}

impl fmt::Debug for Secret {
    /// 값 대신 가려진 글을 찍는다.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        todo!()
    }
}

/// AI 공급자에 내는 자격. API 키와 구독 OAuth 토큰이 같은 자리에 끼워진다(D4).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Credential {
    /// API 키. `x-api-key` 헤더로 나간다.
    ApiKey(Secret),
    /// 이미 발급된 OAuth 접근 토큰. `Authorization: Bearer` 헤더로 나간다.
    OAuthToken(Secret),
}

/// 지금 쓸 자격을 주는 곳. 등록된 자격이 없으면 `None`.
#[async_trait]
pub trait CredentialSource: Send + Sync {
    /// 지금 쓸 자격을 돌려준다.
    async fn current(&self) -> Option<Credential>;
}

/// 대화에서 말한 쪽.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    /// 사용자.
    User,
    /// AI.
    Assistant,
}

/// 대화 한 줄.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Message {
    /// 말한 쪽.
    pub role: Role,
    /// 글.
    pub content: String,
}

/// 공급자에 보내는 요청.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompletionRequest {
    /// 모델 이름. 설정에서 받은 값을 그대로 보낸다.
    pub model: String,
    /// 시스템 글. 없으면 보내지 않는다.
    pub system: Option<String>,
    /// 대화.
    pub messages: Vec<Message>,
    /// 최대 출력 토큰.
    pub max_tokens: u32,
}

/// 응답이 멈춘 이유.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StopReason {
    /// `end_turn`: 자연스럽게 끝났다.
    EndTurn,
    /// `max_tokens`: 최대 토큰에 닿았다.
    MaxTokens,
    /// `stop_sequence`: 멈춤 글자를 만났다.
    StopSequence,
    /// `tool_use`: 도구를 불렀다.
    ToolUse,
    /// `pause_turn`: 긴 턴이 잠시 멈췄다.
    PauseTurn,
    /// `refusal`: 거절했다.
    Refusal,
    /// `model_context_window_exceeded`: 문맥 창을 넘었다.
    ModelContextWindowExceeded,
    /// 이 라이브러리가 모르는 값. 받은 글자를 그대로 담는다.
    Other(String),
}

/// 쓴 토큰 수.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Usage {
    /// 입력 토큰.
    pub input_tokens: u64,
    /// 출력 토큰.
    pub output_tokens: u64,
}

/// 공급자의 응답.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompletionResponse {
    /// 응답의 글(`text` 블록만 순서대로 이어 붙인 것).
    pub text: String,
    /// 멈춘 이유.
    pub stop_reason: StopReason,
    /// 쓴 토큰 수.
    pub usage: Usage,
}

/// 이 모듈이 내는 오류. 문장과 `Debug`에 자격 글자가 들어가지 않는다(E38).
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum AiError {
    /// 자격이 등록되지 않았다. 바깥으로 요청을 보내지 않았다(V31).
    #[error("AI 키가 설정되지 않았습니다")]
    NotConfigured,
    /// 인증 실패(401 · 403). 다시 시도하지 않는다.
    #[error("AI 공급자 인증에 실패했습니다 (상태 {status})")]
    Auth {
        /// HTTP 상태 코드.
        status: u16,
    },
    /// 한도 초과(429). 다시 시도한 뒤에도 실패했다.
    #[error("AI 공급자 한도를 넘었습니다")]
    RateLimited {
        /// 응답의 `retry-after`(초)가 있으면 그 값.
        retry_after: Option<Duration>,
    },
    /// 공급자 장애(5xx · 529). 다시 시도한 뒤에도 실패했다.
    #[error("AI 공급자에 장애가 있습니다 (상태 {status})")]
    ProviderUnavailable {
        /// HTTP 상태 코드.
        status: u16,
    },
    /// 잘못된 요청(400). 다시 시도하지 않는다.
    #[error("AI 공급자가 요청을 받아들이지 않았습니다: {message}")]
    InvalidRequest {
        /// 공급자가 준 설명. 자격 글자는 가려져 있다.
        message: String,
    },
    /// 설정한 시간 안에 응답이 오지 않았다.
    #[error("AI 공급자 응답이 시간 안에 오지 않았습니다")]
    Timeout,
    /// 공급자에 연결하지 못했다.
    #[error("AI 공급자에 연결하지 못했습니다")]
    Connection,
    /// 성공 응답인데 읽을 수 없는 모양이다.
    #[error("AI 공급자 응답을 읽지 못했습니다")]
    InvalidResponse,
}

/// AI 공급자. 에이전트 실행기는 이 trait만 안다.
#[async_trait]
pub trait AiProvider: Send + Sync {
    /// 요청을 보내고 응답을 받는다.
    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse, AiError>;
}

/// Anthropic 공급자의 설정. 기본 모델은 없다(모델은 요청마다 받는다).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AnthropicConfig {
    /// 기본 주소(경로 없이). 예: [`ANTHROPIC_DEFAULT_BASE_URL`].
    pub base_url: String,
    /// 요청 한 번의 시간 제한.
    pub timeout: Duration,
    /// 한도 초과 · 장애일 때 다시 시도하는 횟수(첫 요청은 세지 않는다).
    pub max_retries: u32,
    /// 첫 다시 시도까지의 간격. 시도마다 늘어난다. 테스트는 0에 가깝게 준다.
    pub retry_base_delay: Duration,
}

impl Default for AnthropicConfig {
    /// 기본 주소 [`ANTHROPIC_DEFAULT_BASE_URL`], 다시 시도 [`DEFAULT_MAX_RETRIES`]번.
    fn default() -> Self {
        todo!()
    }
}

/// Anthropic Messages API를 직접 부르는 공급자.
pub struct AnthropicProvider {
    config: AnthropicConfig,
    credentials: Arc<dyn CredentialSource>,
}

impl AnthropicProvider {
    /// 설정과 자격을 주는 곳으로 공급자를 만든다.
    pub fn new(config: AnthropicConfig, credentials: Arc<dyn CredentialSource>) -> Self {
        todo!()
    }

    /// 이 자격으로 공급자에 닿는지 확인한다.
    ///
    /// 돈이 들지 않는 요청(`GET /v1/models`) 한 번만 보낸다. 다시 시도하지 않는다.
    /// 만들 때 받은 자격 자리는 보지 않고, 넘겨받은 `credential`만 쓴다.
    pub async fn verify(&self, credential: &Credential) -> Result<(), AiError> {
        todo!()
    }
}

#[async_trait]
impl AiProvider for AnthropicProvider {
    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse, AiError> {
        todo!()
    }
}
