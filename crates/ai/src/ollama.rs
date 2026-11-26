//! 로컬 Ollama 임베딩 공급자(ai.md 10차 4~11, 결정 2026-11-21 15번).
//!
//! 요청 · 응답 모양은 Ollama 공식 API 문서(`docs/api.md` "Generate Embeddings" ·
//! "List Local Models", `docs/api/errors.mdx`)를 따른다. 주소는 루프백만 받는다(V23).

mod reach;
mod wire;

pub use reach::{OllamaReach, ollama_reach};

use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use reqwest::{Method, RequestBuilder};

use crate::DEFAULT_MAX_RETRIES;
use crate::anthropic::AnthropicLimits;
use crate::embedding::{
    EmbeddingLimits, EmbeddingProvider, EmbeddingRequest, EmbeddingResponse, EmbeddingUsage,
};
use crate::error::AiError;
use crate::http::{self, Failure, Retrier};
use crate::redact::{NoRedaction, Redactor};
use crate::retry::RetryBackoff;

/// 첫 다시 시도까지 간격의 기본값(Anthropic 공급자와 같다, A17).
const DEFAULT_RETRY_BASE_DELAY: Duration = Duration::from_secs(2);

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
        Self {
            base_url: OLLAMA_DEFAULT_BASE_URL.to_string(),
            timeout: OLLAMA_DEFAULT_TIMEOUT,
            max_retries: DEFAULT_MAX_RETRIES,
            retry_base_delay: DEFAULT_RETRY_BASE_DELAY,
            max_inputs_per_request: OLLAMA_DEFAULT_MAX_INPUTS_PER_REQUEST,
            max_input_chars: OLLAMA_DEFAULT_MAX_INPUT_CHARS,
        }
    }
}

/// 로컬 Ollama의 `POST /api/embed`를 부르는 임베딩 공급자.
pub struct OllamaEmbeddingProvider {
    config: OllamaConfig,
    limits: AnthropicLimits,
    redactor: Arc<dyn Redactor>,
    /// HTTP 클라이언트. 만들지 못했으면 `None`이고 요청은 연결 실패가 된다.
    client: Option<reqwest::Client>,
    /// 다시 시도(Anthropic 공급자와 같은 함수).
    retrier: Retrier,
}

impl OllamaEmbeddingProvider {
    /// 설정으로 공급자를 만든다. 연결 제한 · 응답 상한은 [`AnthropicLimits::default`]다.
    ///
    /// 주소가 루프백이 아니면 [`AiError::InsecureBaseUrl`]로 거부한다. 만들 때 아무것도 보내지 않는다.
    pub fn new(config: OllamaConfig) -> Result<Self, AiError> {
        Self::with_limits(config, AnthropicLimits::default())
    }

    /// 설정과 연결 제한 · 응답 크기 상한 · 다시 시도 간격 설정으로 공급자를 만든다
    /// ([`AnthropicLimits`]와 같은 값 · 같은 뜻). 주소 검사는 [`Self::new`]와 같다.
    pub fn with_limits(config: OllamaConfig, limits: AnthropicLimits) -> Result<Self, AiError> {
        if !is_loopback_base_url(&config.base_url) {
            return Err(AiError::InsecureBaseUrl);
        }
        // 글이 내 컴퓨터 밖으로 나갈 길을 막는다(V23): 리다이렉트를 따라가지 않고,
        // 프록시를 거치지 않으며, `localhost`는 이름 풀이에 맡기지 않고 루프백으로만 간다.
        let loopback = [
            SocketAddr::from((Ipv4Addr::LOCALHOST, 0)),
            SocketAddr::from((Ipv6Addr::LOCALHOST, 0)),
        ];
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .no_proxy()
            .resolve_to_addrs("localhost", &loopback)
            .connect_timeout(limits.connect_timeout)
            .build()
            .ok();
        Ok(Self::assemble(config, limits, client))
    }

    /// (결정 34) 같은 집 다른 기기의 Ollama도 받는 공급자. 주소가 루프백이거나 사설망([`ollama_reach`]가
    /// [`OllamaReach::PrivateNetwork`])일 때만 만들고, 공인 주소 · 도메인은 [`AiError::InsecureBaseUrl`]로 거부한다.
    /// 보내는 글은 [`Self::new`]처럼 가린다. 리다이렉트를 따라가지 않고 프록시를 거치지 않으며, 이름(`*.local`)은
    /// 사설망 · 루프백 주소로 풀릴 때만 잇는다. 만들 때 아무것도 보내지 않는다.
    pub fn on_private_network(
        config: OllamaConfig,
        limits: AnthropicLimits,
    ) -> Result<Self, AiError> {
        if ollama_reach(&config.base_url) != OllamaReach::Loopback {
            return Err(AiError::InsecureBaseUrl);
        }
        let loopback = [
            SocketAddr::from((Ipv4Addr::LOCALHOST, 0)),
            SocketAddr::from((Ipv6Addr::LOCALHOST, 0)),
        ];
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .no_proxy()
            .resolve_to_addrs("localhost", &loopback)
            .dns_resolver(reach::HomeOnlyResolver)
            .connect_timeout(limits.connect_timeout)
            .build()
            .ok();
        Ok(Self::assemble(config, limits, client))
    }

    fn assemble(
        config: OllamaConfig,
        limits: AnthropicLimits,
        client: Option<reqwest::Client>,
    ) -> Self {
        let seed = limits.jitter_seed.unwrap_or_else(http::random_seed);
        let backoff = RetryBackoff::new(config.retry_base_delay, limits.max_retry_delay, seed);
        Self {
            retrier: Retrier::new(config.max_retries, limits.retry_after_unit, backoff),
            config,
            limits,
            redactor: Arc::new(NoRedaction),
            client,
        }
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
    /// 요청은 한 번만 보낸다. 다시 시도하지 않는다.
    pub async fn check(&self, model: &str) -> Result<(), AiError> {
        let builder = self.request(Method::GET, "/api/tags")?;
        let body = self
            .send(builder, model)
            .await
            .map_err(|failure| failure.error)?;
        if wire::has_model(&body, model)? {
            Ok(())
        } else {
            Err(wire::model_missing(model, self.redactor.as_ref()))
        }
    }

    /// 인증 헤더 없는 요청을 만든다.
    fn request(&self, method: Method, path: &str) -> Result<RequestBuilder, AiError> {
        let client = self.client.as_ref().ok_or(AiError::Connection)?;
        let url = format!("{}{path}", self.config.base_url.trim_end_matches('/'));
        Ok(client.request(method, url).timeout(self.config.timeout))
    }

    /// 요청을 한 번 보내고, 성공이면 본문을, 아니면 구분한 오류와 기다릴 시간을 돌려준다.
    async fn send(&self, builder: RequestBuilder, model: &str) -> Result<Vec<u8>, Failure> {
        http::send_once(builder, self.limits.max_response_bytes, |reply| {
            let retry_after = reply.header_after;
            let error = wire::error_from_status(
                reply.status,
                retry_after,
                reply.body,
                model,
                self.redactor.as_ref(),
            );
            Failure { error, retry_after }
        })
        .await
    }

    /// 글마다 가리고, 가린 뒤 글이 비었거나 상한보다 길면 [`AiError::InvalidRequest`].
    /// 오류 문장에는 글을 담지 않고 차례와 길이만 담는다(E38).
    fn prepare_inputs(&self, inputs: &[String]) -> Result<Vec<String>, AiError> {
        let max = self.config.max_input_chars;
        let mut prepared = Vec::with_capacity(inputs.len());
        for (index, input) in inputs.iter().enumerate() {
            let text = self.redactor.redact(input);
            let order = index + 1;
            if text.is_empty() {
                return Err(AiError::InvalidRequest {
                    message: format!("임베딩 입력 {order}번째가 빈 글입니다"),
                });
            }
            let chars = text.chars().count();
            if chars > max {
                return Err(AiError::InvalidRequest {
                    message: format!(
                        "임베딩 입력 {order}번째가 {chars}자로 상한 {max}자를 넘습니다"
                    ),
                });
            }
            prepared.push(text);
        }
        Ok(prepared)
    }

    /// 한 조각을 보내고(다시 시도 포함) 응답을 읽는다.
    async fn embed_chunk(
        &self,
        model: &str,
        inputs: &[String],
        dimensions: Option<u32>,
    ) -> Result<wire::EmbedChunk, AiError> {
        let body = wire::EmbedBody {
            model,
            input: inputs,
            dimensions,
        };
        // 클라이언트 문제는 다시 보내도 같으니 다시 시도에 넣지 않고 먼저 돌려준다.
        drop(self.request(Method::POST, "/api/embed")?);
        let (this, body) = (self, &body);
        let bytes = self
            .retrier
            .run(move || async move {
                let builder = this.request(Method::POST, "/api/embed")?.json(body);
                this.send(builder, model).await
            })
            .await?;
        wire::parse_embed(&bytes, inputs.len(), dimensions)
    }
}

#[async_trait]
impl EmbeddingProvider for OllamaEmbeddingProvider {
    fn id(&self) -> &'static str {
        OLLAMA_PROVIDER_ID
    }

    fn key_slots(&self) -> &'static [&'static str] {
        &[]
    }

    fn limits(&self) -> EmbeddingLimits {
        EmbeddingLimits {
            max_inputs_per_request: self.config.max_inputs_per_request,
            max_input_chars: self.config.max_input_chars,
        }
    }

    async fn embed(&self, request: EmbeddingRequest) -> Result<EmbeddingResponse, AiError> {
        // `purpose`는 보지 않는다: Ollama는 색인 글과 질의를 같게 보낸다(ai.md 10차 2).
        let EmbeddingRequest {
            model,
            inputs,
            dimensions,
            ..
        } = request;
        if dimensions == Some(0) {
            return Err(AiError::InvalidRequest {
                message: "임베딩 벡터 길이(dimensions)는 1 이상이어야 합니다".to_string(),
            });
        }
        let inputs = self.prepare_inputs(&inputs)?;
        let mut vectors: Vec<Vec<f32>> = Vec::with_capacity(inputs.len());
        let mut answered_model = None;
        let mut total_tokens: u64 = 0;
        // 상한이 0이어도 하나씩은 보낸다.
        for chunk in inputs.chunks(self.config.max_inputs_per_request.max(1)) {
            let reply = self.embed_chunk(&model, chunk, dimensions).await?;
            // 조각마다 길이가 다르면 한 응답으로 이어 붙일 수 없다.
            let same_length = match (vectors.first(), reply.vectors.first()) {
                (Some(before), Some(now)) => before.len() == now.len(),
                _ => true,
            };
            if !same_length {
                return Err(AiError::InvalidResponse);
            }
            if answered_model.is_none() {
                answered_model = reply.model;
            }
            total_tokens = total_tokens.saturating_add(reply.total_tokens);
            vectors.extend(reply.vectors);
        }
        Ok(EmbeddingResponse {
            vectors,
            model: answered_model.unwrap_or(model),
            usage: EmbeddingUsage { total_tokens },
        })
    }
}

/// 내 컴퓨터를 가리키는 주소인가: `http` · `https`이고 호스트가 루프백일 때만.
fn is_loopback_base_url(base_url: &str) -> bool {
    let Ok(url) = reqwest::Url::parse(base_url) else {
        return false;
    };
    matches!(url.scheme(), "http" | "https") && http::is_loopback_host(&url)
}

#[cfg(test)]
mod tests;
