//! Anthropic Messages API를 `reqwest`로 직접 부르는 공급자.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use reqwest::header::{HeaderName, HeaderValue};
use reqwest::{Method, RequestBuilder};

use crate::credential::{Credential, CredentialSource};
use crate::error::AiError;
use crate::http::{self, Failure, Retrier};
use crate::retry::{RETRY_MAX_DELAY, RetryBackoff};
use crate::types::{AiProvider, CompletionRequest, CompletionResponse};
use crate::{ANTHROPIC_DEFAULT_BASE_URL, ANTHROPIC_VERSION, DEFAULT_MAX_RETRIES, wire};

/// 시간 제한의 기본값. 스트리밍 없이 긴 답을 기다릴 수 있게 넉넉히 둔다.
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(600);

/// 첫 다시 시도까지 간격의 기본값(A17, Hermes `base_delay=2`).
const DEFAULT_RETRY_BASE_DELAY: Duration = Duration::from_secs(2);

const API_KEY_HEADER: HeaderName = HeaderName::from_static("x-api-key");
const VERSION_HEADER: HeaderName = HeaderName::from_static("anthropic-version");

/// Anthropic 공급자의 설정. 기본 모델은 없다(모델은 요청마다 받는다).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AnthropicConfig {
    /// 기본 주소(경로 없이). 예: [`ANTHROPIC_DEFAULT_BASE_URL`].
    pub base_url: String,
    /// 요청 한 번의 시간 제한.
    pub timeout: Duration,
    /// 한도 초과 · 장애 · 시간 초과 · 연결 실패일 때 다시 시도하는 횟수(첫 요청은 세지 않는다).
    ///
    /// 부르는 쪽이 알 것: 시간 초과 · 연결 실패는 공급자가 요청을 이미 받아 처리했는지 알 수 없는데도
    /// 다시 보낸다(원본 Hermes와 같다). 그래서 한 번의 [`AiProvider::complete`]가 공급자 쪽에서
    /// 같은 요청으로 여러 번 처리되고 **여러 번 과금될 수 있다**(최대 `1 + max_retries`번).
    /// 이것을 막으려면 0으로 둔다(ai.md 4차 6).
    pub max_retries: u32,
    /// 첫 다시 시도까지의 간격. 시도마다 늘어난다. 테스트는 0에 가깝게 준다.
    pub retry_base_delay: Duration,
}

impl Default for AnthropicConfig {
    /// 기본 주소 [`ANTHROPIC_DEFAULT_BASE_URL`], 다시 시도 [`DEFAULT_MAX_RETRIES`]번.
    fn default() -> Self {
        Self {
            base_url: ANTHROPIC_DEFAULT_BASE_URL.to_string(),
            timeout: DEFAULT_TIMEOUT,
            max_retries: DEFAULT_MAX_RETRIES,
            retry_base_delay: DEFAULT_RETRY_BASE_DELAY,
        }
    }
}

/// 연결 시간 제한의 기본값(A19, Hermes 연결 10초). 전체 요청 제한과 따로 든다.
pub const ANTHROPIC_DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// 응답 크기 상한의 기본값(10MB = 10,000,000바이트).
pub const ANTHROPIC_DEFAULT_MAX_RESPONSE_BYTES: usize = 10_000_000;

/// Anthropic 공급자의 3차 설정. [`AnthropicConfig`]의 모양을 바꾸지 않으려고 따로 둔다.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AnthropicLimits {
    /// 연결 시간 제한(A19). 전체 요청 제한([`AnthropicConfig::timeout`])은 그대로다.
    pub connect_timeout: Duration,
    /// 응답 본문의 상한(바이트). 넘으면 끝까지 읽지 않고 [`AiError::InvalidResponse`].
    pub max_response_bytes: usize,
    /// 늘어나는 다시 시도 간격의 위 끝(A17). 첫 간격은 [`AnthropicConfig::retry_base_delay`].
    pub max_retry_delay: Duration,
    /// `Retry-After` 1초를 실제로 기다릴 시간. 기본 1초. 테스트는 줄여서 쓴다.
    /// 상한([`crate::RETRY_AFTER_CAP`])은 줄이기 전의 초에 건다.
    pub retry_after_unit: Duration,
    /// jitter 무작위의 씨앗. `None`이면 공급자가 스스로 정한다.
    pub jitter_seed: Option<u64>,
}

impl Default for AnthropicLimits {
    /// 연결 [`ANTHROPIC_DEFAULT_CONNECT_TIMEOUT`], 응답 [`ANTHROPIC_DEFAULT_MAX_RESPONSE_BYTES`],
    /// 간격 위 끝 [`crate::RETRY_MAX_DELAY`], `Retry-After` 단위 1초, 씨앗 없음.
    fn default() -> Self {
        Self {
            connect_timeout: ANTHROPIC_DEFAULT_CONNECT_TIMEOUT,
            max_response_bytes: ANTHROPIC_DEFAULT_MAX_RESPONSE_BYTES,
            max_retry_delay: RETRY_MAX_DELAY,
            retry_after_unit: Duration::from_secs(1),
            jitter_seed: None,
        }
    }
}

/// Anthropic Messages API를 직접 부르는 공급자.
pub struct AnthropicProvider {
    config: AnthropicConfig,
    limits: AnthropicLimits,
    credentials: Arc<dyn CredentialSource>,
    /// HTTP 클라이언트. 만들지 못했으면 `None`이고 요청은 연결 실패가 된다.
    client: Option<reqwest::Client>,
    /// 기본 주소가 `https`도 루프백도 아니다. 참이면 아무것도 보내지 않는다.
    insecure: bool,
    /// 다시 시도(Ollama 공급자와 같은 함수, ai.md 10차 10).
    retrier: Retrier,
    /// 나가는 글의 가리기 자리(ai.md 7차 1). 기본은 [`crate::NoRedaction`].
    redactor: Arc<dyn crate::Redactor>,
}

impl AnthropicProvider {
    /// 설정과 자격을 주는 곳으로 공급자를 만든다. 3차 설정은 [`AnthropicLimits::default`]다.
    ///
    /// 기본 주소가 `https`가 아니면(루프백 예외) 만든 공급자는 아무것도 보내지 않고
    /// 모든 호출([`AiProvider::complete`] · [`AnthropicProvider::verify`])에
    /// [`AiError::InsecureBaseUrl`]을 돌려준다. 만들 때 거부하려면 [`AnthropicProvider::with_limits`].
    pub fn new(config: AnthropicConfig, credentials: Arc<dyn CredentialSource>) -> Self {
        Self::build(config, AnthropicLimits::default(), credentials)
    }

    /// 설정 · 3차 설정 · 자격을 주는 곳으로 공급자를 만든다.
    ///
    /// 기본 주소가 `https`가 아니면 [`AiError::InsecureBaseUrl`]로 거부한다.
    /// 루프백(`127.0.0.1` · `::1` · `localhost`)만 예외다. 만들 때 아무것도 보내지 않는다.
    pub fn with_limits(
        config: AnthropicConfig,
        limits: AnthropicLimits,
        credentials: Arc<dyn CredentialSource>,
    ) -> Result<Self, AiError> {
        if !is_allowed_base_url(&config.base_url) {
            return Err(AiError::InsecureBaseUrl);
        }
        Ok(Self::build(config, limits, credentials))
    }

    /// 나가는 글의 가리기 자리를 끼운 공급자를 돌려준다(ai.md 7차 1, E38 · V47).
    ///
    /// 끼운 뒤에는 요청의 시스템 글과 대화 글 모두가 보내기 전에 `redactor`를 거친다
    /// (다시 시도로 다시 보내는 요청도 같다). 모델 이름 · 최대 토큰 같은 칸은 바꾸지 않는다.
    /// 끼우지 않으면 [`crate::NoRedaction`]과 같다(글을 그대로 보낸다).
    #[must_use]
    pub fn with_redactor(self, redactor: Arc<dyn crate::Redactor>) -> Self {
        Self { redactor, ..self }
    }

    fn build(
        config: AnthropicConfig,
        limits: AnthropicLimits,
        credentials: Arc<dyn CredentialSource>,
    ) -> Self {
        // 자격 헤더가 다른 주소로 따라가지 않게 리다이렉트를 끈다.
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(limits.connect_timeout)
            .build()
            .ok();
        let seed = limits.jitter_seed.unwrap_or_else(http::random_seed);
        let backoff = RetryBackoff::new(config.retry_base_delay, limits.max_retry_delay, seed);
        Self {
            insecure: !is_allowed_base_url(&config.base_url),
            retrier: Retrier::new(config.max_retries, limits.retry_after_unit, backoff),
            config,
            limits,
            credentials,
            client,
            redactor: Arc::new(crate::NoRedaction),
        }
    }

    /// 이 자격으로 공급자에 닿는지 확인한다.
    ///
    /// 돈이 들지 않는 요청(`GET /v1/models`) 한 번만 보낸다. 다시 시도하지 않는다.
    /// 만들 때 받은 자격 자리는 보지 않고, 넘겨받은 `credential`만 쓴다.
    /// 2xx여도 본문이 공식 문서의 모델 목록 모양(`data` 배열)이 아니면
    /// [`AiError::InvalidResponse`]다.
    pub async fn verify(&self, credential: &Credential) -> Result<(), AiError> {
        if self.insecure {
            return Err(AiError::InsecureBaseUrl);
        }
        let builder = self.request(Method::GET, "/v1/models?limit=1", credential)?;
        let body = self
            .send(builder, credential)
            .await
            .map_err(|failure| failure.error)?;
        wire::check_model_list(&body)
    }

    /// 버전 헤더와 인증 헤더가 붙은 요청을 만든다. 자격은 헤더에만 들어간다.
    fn request(
        &self,
        method: Method,
        path: &str,
        credential: &Credential,
    ) -> Result<RequestBuilder, AiError> {
        let (name, value) = auth_header(credential)?;
        let client = self.client.as_ref().ok_or(AiError::Connection)?;
        let url = format!("{}{path}", self.config.base_url.trim_end_matches('/'));
        Ok(client
            .request(method, url)
            .timeout(self.config.timeout)
            .header(VERSION_HEADER, ANTHROPIC_VERSION)
            .header(name, value))
    }

    /// 요청을 한 번 보내고, 성공이면 본문을, 아니면 구분한 오류와 기다릴 시간을 돌려준다.
    async fn send(
        &self,
        builder: RequestBuilder,
        credential: &Credential,
    ) -> Result<Vec<u8>, Failure> {
        http::send_once(builder, self.limits.max_response_bytes, |reply| {
            let retry_after = reply
                .header_after
                .or_else(|| wire::body_retry_after(reply.body, reply.now));
            let error = wire::error_from_status(reply.status, retry_after, reply.body, credential);
            Failure { error, retry_after }
        })
        .await
    }
}

#[async_trait]
impl AiProvider for AnthropicProvider {
    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse, AiError> {
        if self.insecure {
            return Err(AiError::InsecureBaseUrl);
        }
        // 자격이 없으면 바깥으로 아무것도 보내지 않는다(V31).
        let credential = self
            .credentials
            .current()
            .await
            .ok_or(AiError::NotConfigured)?;
        // 가린 요청을 한 번 만들어 다시 시도에도 같은 본문을 보낸다(ai.md 7차 1).
        let request = crate::redact::redact_request(self.redactor.as_ref(), request);
        let body = wire::MessagesBody::from_request(&request);
        // 자격 · 클라이언트 문제는 다시 보내도 같으니 다시 시도에 넣지 않고 먼저 돌려준다.
        drop(self.request(Method::POST, "/v1/messages", &credential)?);
        let (this, credential, body) = (self, &credential, &body);
        let bytes = self
            .retrier
            .run(move || async move {
                let builder = this
                    .request(Method::POST, "/v1/messages", credential)?
                    .json(body);
                this.send(builder, credential).await
            })
            .await?;
        wire::parse_completion(&bytes)
    }
}

/// 평문으로 보내도 되는 주소인가: `https`이거나, `http`면 루프백만.
fn is_allowed_base_url(base_url: &str) -> bool {
    let Ok(url) = reqwest::Url::parse(base_url) else {
        return false;
    };
    match url.scheme() {
        "https" => url.host_str().is_some(),
        "http" => http::is_loopback_host(&url),
        _ => false,
    }
}

/// 자격의 인증 헤더. API 키는 `x-api-key`로 나간다.
///
/// 비어 있는 자격은 [`AiError::NotConfigured`], 줄바꿈 · 제어 문자처럼 헤더로 보낼 수 없는
/// 글자가 든 자격은 [`AiError::InvalidCredential`]이다. 어느 쪽이든 요청을 보내지 않는다.
fn auth_header(credential: &Credential) -> Result<(HeaderName, HeaderValue), AiError> {
    let secret = credential.secret().expose();
    if secret.is_empty() {
        return Err(AiError::NotConfigured);
    }
    if secret.chars().any(char::is_control) {
        return Err(AiError::InvalidCredential);
    }
    let mut value = HeaderValue::from_str(secret).map_err(|_| AiError::InvalidCredential)?;
    value.set_sensitive(true);
    Ok((API_KEY_HEADER, value))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::credential::Secret;

    #[test]
    fn 인증_헤더는_기록에_찍히지_않게_표시한다() {
        let (_, value) =
            auth_header(&Credential::ApiKey(Secret::new("SIHEOM-DANWI-0003"))).unwrap();
        assert!(value.is_sensitive());
        assert!(!format!("{value:?}").contains("SIHEOM-DANWI-0003"));
    }

    #[test]
    fn 빈_자격은_설정_안_됨이고_헤더로_못_보내는_자격은_모양이_잘못됨이다() {
        let empty = Credential::ApiKey(Secret::new(""));
        assert_eq!(auth_header(&empty), Err(AiError::NotConfigured));
        for bad in ["줄\n바꿈", "KEY\u{1b}", "KEY\u{7f}"] {
            let credential = Credential::ApiKey(Secret::new(bad));
            assert_eq!(auth_header(&credential), Err(AiError::InvalidCredential));
        }
    }

    #[test]
    fn 루프백만_평문을_받아들인다() {
        assert!(is_allowed_base_url("https://example.test"));
        assert!(is_allowed_base_url("http://LOCALHOST:1"));
        assert!(!is_allowed_base_url("https://"));
        assert!(!is_allowed_base_url("http://127.0.0.2:1"));
        assert!(!is_allowed_base_url("http://[::2]:1"));
    }
}
