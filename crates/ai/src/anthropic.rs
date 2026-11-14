//! Anthropic Messages API를 `reqwest`로 직접 부르는 공급자.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use reqwest::header::{AUTHORIZATION, HeaderName, HeaderValue, RETRY_AFTER};
use reqwest::{Method, RequestBuilder};

use crate::credential::{Credential, CredentialSource};
use crate::error::AiError;
use crate::types::{AiProvider, CompletionRequest, CompletionResponse};
use crate::{ANTHROPIC_DEFAULT_BASE_URL, ANTHROPIC_VERSION, DEFAULT_MAX_RETRIES, retry, wire};

/// 시간 제한의 기본값. 스트리밍 없이 긴 답을 기다릴 수 있게 넉넉히 둔다.
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(600);

/// 첫 다시 시도까지 간격의 기본값.
const DEFAULT_RETRY_BASE_DELAY: Duration = Duration::from_secs(1);

const API_KEY_HEADER: HeaderName = HeaderName::from_static("x-api-key");
const VERSION_HEADER: HeaderName = HeaderName::from_static("anthropic-version");

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
        Self {
            base_url: ANTHROPIC_DEFAULT_BASE_URL.to_string(),
            timeout: DEFAULT_TIMEOUT,
            max_retries: DEFAULT_MAX_RETRIES,
            retry_base_delay: DEFAULT_RETRY_BASE_DELAY,
        }
    }
}

/// Anthropic Messages API를 직접 부르는 공급자.
pub struct AnthropicProvider {
    config: AnthropicConfig,
    credentials: Arc<dyn CredentialSource>,
    /// HTTP 클라이언트. 만들지 못했으면 `None`이고 요청은 연결 실패가 된다.
    client: Option<reqwest::Client>,
}

impl AnthropicProvider {
    /// 설정과 자격을 주는 곳으로 공급자를 만든다.
    pub fn new(config: AnthropicConfig, credentials: Arc<dyn CredentialSource>) -> Self {
        // 자격 헤더가 다른 주소로 따라가지 않게 리다이렉트를 끈다.
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .ok();
        Self {
            config,
            credentials,
            client,
        }
    }

    /// 이 자격으로 공급자에 닿는지 확인한다.
    ///
    /// 돈이 들지 않는 요청(`GET /v1/models`) 한 번만 보낸다. 다시 시도하지 않는다.
    /// 만들 때 받은 자격 자리는 보지 않고, 넘겨받은 `credential`만 쓴다.
    pub async fn verify(&self, credential: &Credential) -> Result<(), AiError> {
        let builder = self.request(Method::GET, "/v1/models?limit=1", credential)?;
        self.send(builder, credential).await.map(|_| ())
    }

    /// 버전 헤더와 인증 헤더가 붙은 요청을 만든다. 자격은 헤더에만 들어간다.
    fn request(
        &self,
        method: Method,
        path: &str,
        credential: &Credential,
    ) -> Result<RequestBuilder, AiError> {
        let client = self.client.as_ref().ok_or(AiError::Connection)?;
        let url = format!("{}{path}", self.config.base_url.trim_end_matches('/'));
        let (name, value) = auth_header(credential)?;
        Ok(client
            .request(method, url)
            .timeout(self.config.timeout)
            .header(VERSION_HEADER, ANTHROPIC_VERSION)
            .header(name, value))
    }

    /// 요청을 한 번 보내고, 성공이면 본문을, 아니면 구분한 오류를 돌려준다.
    async fn send(
        &self,
        builder: RequestBuilder,
        credential: &Credential,
    ) -> Result<Vec<u8>, AiError> {
        let response = builder.send().await.map_err(transport_error)?;
        let status = response.status();
        let retry_after = response
            .headers()
            .get(RETRY_AFTER)
            .and_then(|value| value.to_str().ok())
            .and_then(wire::parse_retry_after);
        if status.is_success() {
            let body = response.bytes().await.map_err(transport_error)?;
            return Ok(body.to_vec());
        }
        // 오류 본문을 다 읽지 못해도 상태 코드로 구분한다.
        let body = response.bytes().await.unwrap_or_default();
        Err(wire::error_from_status(
            status.as_u16(),
            retry_after,
            &body,
            credential,
        ))
    }
}

#[async_trait]
impl AiProvider for AnthropicProvider {
    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse, AiError> {
        // 자격이 없으면 바깥으로 아무것도 보내지 않는다(V31).
        let credential = self
            .credentials
            .current()
            .await
            .ok_or(AiError::NotConfigured)?;
        let body = wire::MessagesBody::from_request(&request);
        let mut attempt = 0;
        loop {
            let builder = self
                .request(Method::POST, "/v1/messages", &credential)?
                .json(&body);
            let err = match self.send(builder, &credential).await {
                Ok(bytes) => return wire::parse_completion(&bytes),
                Err(err) => err,
            };
            match retry::wait_before_retry(&self.config, attempt, &err) {
                Some(wait) => tokio::time::sleep(wait).await,
                None => return Err(err),
            }
            attempt += 1;
        }
    }
}

/// 자격 종류에 맞는 인증 헤더. API 키는 `x-api-key`, OAuth 토큰은 `Authorization: Bearer`.
///
/// 비어 있거나 헤더로 보낼 수 없는 글자가 든 자격은 쓸 수 있는 자격이 아니므로
/// 요청을 보내지 않고 [`AiError::NotConfigured`]를 낸다.
fn auth_header(credential: &Credential) -> Result<(HeaderName, HeaderValue), AiError> {
    let secret = credential.secret().expose();
    if secret.is_empty() {
        return Err(AiError::NotConfigured);
    }
    let (name, value) = match credential {
        Credential::ApiKey(_) => (API_KEY_HEADER, HeaderValue::from_str(secret)),
        Credential::OAuthToken(_) => (
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {secret}")),
        ),
    };
    let mut value = value.map_err(|_| AiError::NotConfigured)?;
    value.set_sensitive(true);
    Ok((name, value))
}

/// 응답을 받지 못한 까닭을 구분한다. 오류에는 종류만 남긴다.
fn transport_error(err: reqwest::Error) -> AiError {
    if err.is_timeout() {
        AiError::Timeout
    } else {
        AiError::Connection
    }
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
    fn 쓸_수_없는_자격은_설정_안_됨이다() {
        for bad in ["", "줄\n바꿈"] {
            for credential in [
                Credential::ApiKey(Secret::new(bad)),
                Credential::OAuthToken(Secret::new(bad)),
            ] {
                assert_eq!(auth_header(&credential), Err(AiError::NotConfigured));
            }
        }
    }
}
