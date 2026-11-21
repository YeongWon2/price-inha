//! 로컬 Ollama 임베딩 공급자(ai.md 10차 4~11, 결정 2026-11-21 15번).
//!
//! 요청 · 응답 모양은 Ollama 공식 API 문서(`docs/api.md` "Generate Embeddings" ·
//! "List Local Models", `docs/api/errors.mdx`)를 따른다. 주소는 루프백만 받는다(V23).

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;

use crate::anthropic::AnthropicLimits;
use crate::embedding::{EmbeddingLimits, EmbeddingProvider, EmbeddingRequest, EmbeddingResponse};
use crate::error::AiError;
use crate::redact::Redactor;

/// Ollama 공급자 ID(등록부의 열쇠).
pub const OLLAMA_PROVIDER_ID: &str = "ollama";

/// Ollama의 기본 주소(내 컴퓨터).
pub const OLLAMA_DEFAULT_BASE_URL: &str = "http://127.0.0.1:11434";

/// 한 요청 시간 제한의 기본값. 첫 요청은 모델을 메모리에 올리느라 느릴 수 있다.
pub const OLLAMA_DEFAULT_TIMEOUT: Duration = Duration::from_secs(60);

/// 한 요청에 담는 입력 개수의 기본 상한.
pub const OLLAMA_DEFAULT_MAX_INPUTS_PER_REQUEST: usize = 64;

/// 입력 하나 글자 수의 기본 상한(knowledge의 `INDEX_PREFIX_CHARS`와 같다).
pub const OLLAMA_DEFAULT_MAX_INPUT_CHARS: usize = 8_192;

/// Ollama 공급자의 설정. 기본 모델은 없다(모델은 요청마다 받는다).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OllamaConfig {
    /// 기본 주소(경로 없이). 루프백(`127.0.0.1` · `::1` · `localhost`)만 받는다.
    pub base_url: String,
    /// 요청 한 번의 시간 제한.
    pub timeout: Duration,
    /// 장애 · 시간 초과 · 연결 실패일 때 다시 시도하는 횟수(첫 요청은 세지 않는다).
    pub max_retries: u32,
    /// 첫 다시 시도까지의 간격. 시도마다 늘어난다.
    pub retry_base_delay: Duration,
    /// 한 요청에 담는 입력의 최대 개수. 넘으면 나눠 차례로 보낸다.
    pub max_inputs_per_request: usize,
    /// 입력 하나의 최대 글자 수(가린 뒤 글로 센다).
    pub max_input_chars: usize,
}

impl Default for OllamaConfig {
    /// 주소 [`OLLAMA_DEFAULT_BASE_URL`], 시간 [`OLLAMA_DEFAULT_TIMEOUT`],
    /// 다시 시도 [`crate::DEFAULT_MAX_RETRIES`]번, 한 요청 64개, 입력 하나 8,192자.
    fn default() -> Self {
        todo!()
    }
}

/// 로컬 Ollama의 `POST /api/embed`를 부르는 임베딩 공급자.
pub struct OllamaEmbeddingProvider {
    config: OllamaConfig,
    limits: AnthropicLimits,
    redactor: Arc<dyn Redactor>,
}

impl OllamaEmbeddingProvider {
    /// 설정으로 공급자를 만든다. 연결 제한 · 응답 상한은 [`AnthropicLimits::default`]다.
    ///
    /// 주소가 루프백이 아니면 [`AiError::InsecureBaseUrl`]로 거부한다. 만들 때 아무것도 보내지 않는다.
    pub fn new(config: OllamaConfig) -> Result<Self, AiError> {
        let _ = config;
        todo!()
    }

    /// 설정과 연결 제한 · 응답 크기 상한 · 다시 시도 간격 설정으로 공급자를 만든다
    /// ([`AnthropicLimits`]와 같은 값 · 같은 뜻). 주소 검사는 [`Self::new`]와 같다.
    pub fn with_limits(config: OllamaConfig, limits: AnthropicLimits) -> Result<Self, AiError> {
        let _ = (config, limits);
        todo!()
    }

    /// 보내는 글마다 거칠 가리기 자리를 끼운다(E38 · V47). 길이 판단은 가린 뒤 글로 한다.
    #[must_use]
    pub fn with_redactor(self, redactor: Arc<dyn Redactor>) -> Self {
        Self { redactor, ..self }
    }

    /// Ollama가 떠 있고 `model`을 받아 두었는지 `GET /api/tags`로 확인한다.
    ///
    /// 목록의 `name`이 `model` 또는 `model:latest`면 성공, 없으면 [`AiError::InvalidRequest`]
    /// (문장에 `ollama pull <모델>`), 연결 실패면 [`AiError::Connection`].
    pub async fn check(&self, model: &str) -> Result<(), AiError> {
        let _ = (&self.config, &self.limits, &self.redactor, model);
        todo!()
    }
}

#[async_trait]
impl EmbeddingProvider for OllamaEmbeddingProvider {
    fn id(&self) -> &'static str {
        todo!()
    }

    fn key_slots(&self) -> &'static [&'static str] {
        todo!()
    }

    fn limits(&self) -> EmbeddingLimits {
        todo!()
    }

    async fn embed(&self, request: EmbeddingRequest) -> Result<EmbeddingResponse, AiError> {
        let _ = request;
        todo!()
    }
}
