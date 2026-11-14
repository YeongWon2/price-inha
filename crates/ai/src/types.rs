//! 공급자와 주고받는 값과 공급자 trait. 에이전트 실행기는 이것만 안다.

use async_trait::async_trait;

use crate::error::AiError;

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

/// AI 공급자. 에이전트 실행기는 이 trait만 안다.
#[async_trait]
pub trait AiProvider: Send + Sync {
    /// 요청을 보내고 응답을 받는다.
    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse, AiError>;
}
