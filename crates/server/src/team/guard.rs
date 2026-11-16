//! 에이전트 실행기에 넘기는 AI 공급자: 고른 연결(또는 시연 모드의 가짜)을 감싸 받은 답 · 오류 글을 가린다.
//!
//! 실행기는 나가는 글만 가린다(`Ports::redactor`). 받은 답과 공급자 오류 글은 방 로그 · 세션 · 복기 기록에
//! 그대로 저장되므로(agent 1차 검토 참고), 여기서 비밀 보관소로 가린 뒤에 넘긴다.

use std::sync::Arc;

use async_trait::async_trait;
use pi_ai::{AiError, AiProvider, CompletionRequest, CompletionResponse, Redactor};

use super::DemoAi;
use crate::SecretsRedactor;
use crate::desk::Desk;

/// 답을 내는 곳.
pub(super) enum Source {
    /// 시연 모드의 가짜.
    Demo(Arc<DemoAi>),
    /// 고른 AI 연결. 쓸 때마다 다시 받는다(화면에서 다시 고를 수 있다).
    Desk(Arc<Desk>),
}

/// 가리는 공급자.
pub(super) struct Guarded {
    pub(super) source: Source,
    pub(super) redactor: Arc<SecretsRedactor>,
}

impl Guarded {
    fn provider(&self) -> Arc<dyn AiProvider> {
        match &self.source {
            Source::Demo(demo) => Arc::clone(demo) as Arc<dyn AiProvider>,
            Source::Desk(desk) => desk.ai_provider(),
        }
    }
}

#[async_trait]
impl AiProvider for Guarded {
    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse, AiError> {
        match self.provider().complete(request).await {
            Ok(mut response) => {
                response.text = self.redactor.redact(&response.text);
                Ok(response)
            }
            Err(AiError::InvalidRequest { message }) => Err(AiError::InvalidRequest {
                message: self.redactor.redact(&message),
            }),
            Err(other) => Err(other),
        }
    }
}
