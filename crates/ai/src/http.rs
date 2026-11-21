//! HTTP 공급자들이 나눠 쓰는 것: 주소 검사 · 한 번 보내기 · 응답 크기 상한 · 다시 시도.
//!
//! Anthropic 공급자와 Ollama 공급자가 같은 함수를 쓴다(ai.md 3차 1~6, 10차 10).

use std::future::Future;
use std::hash::{BuildHasher, RandomState};
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, SystemTime};

use reqwest::header::RETRY_AFTER;
use reqwest::{RequestBuilder, Response};

use crate::error::AiError;
use crate::retry::{self, RetryBackoff};

/// 보낸 요청 하나가 실패한 까닭과, 공급자가 기다리라고 한 시간.
pub(crate) struct Failure {
    pub(crate) error: AiError,
    pub(crate) retry_after: Option<Duration>,
}

impl From<AiError> for Failure {
    fn from(error: AiError) -> Self {
        Self {
            error,
            retry_after: None,
        }
    }
}

/// 성공이 아닌 응답 하나. 공급자마다 자기 오류로 구분한다.
pub(crate) struct ErrorReply<'a> {
    /// HTTP 상태 코드.
    pub(crate) status: u16,
    /// 상한까지 읽은 본문. 다 읽지 못했으면 비어 있다.
    pub(crate) body: &'a [u8],
    /// `Retry-After` 헤더로 읽은 기다릴 시간.
    pub(crate) header_after: Option<Duration>,
    /// 응답을 받은 시각.
    pub(crate) now: SystemTime,
}

/// 기본 주소의 호스트가 루프백(`127.0.0.1` · `::1` · `localhost`)인가.
pub(crate) fn is_loopback_host(url: &reqwest::Url) -> bool {
    matches!(url.host_str(), Some("127.0.0.1" | "[::1]" | "localhost"))
}

/// 설정에 씨앗이 없을 때 쓰는 씨앗. 프로세스마다 다른 해시 열쇠와 지금 시각으로 만든다.
pub(crate) fn random_seed() -> u64 {
    RandomState::new().hash_one(SystemTime::now())
}

/// 응답을 받지 못한 까닭을 구분한다. 오류에는 종류만 남긴다.
pub(crate) fn transport_error(err: reqwest::Error) -> AiError {
    if err.is_timeout() {
        AiError::Timeout
    } else {
        AiError::Connection
    }
}

/// 요청을 한 번 보내고, 성공이면 본문을, 아니면 `classify`가 구분한 오류를 돌려준다.
///
/// 본문은 `max_response_bytes`까지만 읽는다. 오류 본문을 다 읽지 못해도 상태 코드로 구분한다.
pub(crate) async fn send_once<C>(
    builder: RequestBuilder,
    max_response_bytes: usize,
    classify: C,
) -> Result<Vec<u8>, Failure>
where
    C: FnOnce(ErrorReply<'_>) -> Failure,
{
    let response = builder.send().await.map_err(transport_error)?;
    let status = response.status();
    let now = SystemTime::now();
    let header_after = response
        .headers()
        .get(RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| retry::parse_retry_after(value, now));
    let body = read_capped(response, max_response_bytes).await;
    if status.is_success() {
        return body.map_err(Failure::from);
    }
    let body = body.unwrap_or_default();
    Err(classify(ErrorReply {
        status: status.as_u16(),
        body: &body,
        header_after,
        now,
    }))
}

/// 본문을 상한까지만 읽는다. 넘으면 끝까지 읽지 않고 [`AiError::InvalidResponse`].
async fn read_capped(mut response: Response, limit: usize) -> Result<Vec<u8>, AiError> {
    let limit_u64 = u64::try_from(limit).unwrap_or(u64::MAX);
    if response.content_length().is_some_and(|len| len > limit_u64) {
        return Err(AiError::InvalidResponse);
    }
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(transport_error)? {
        if body.len().saturating_add(chunk.len()) > limit {
            return Err(AiError::InvalidResponse);
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

/// 다시 시도(A15 · A17 · A18). 다시 보내면 나을 수 있는 오류만 정한 횟수까지 다시 보낸다.
pub(crate) struct Retrier {
    max_retries: u32,
    /// `Retry-After` 1초를 실제로 기다릴 시간.
    retry_after_unit: Duration,
    /// 다시 시도 간격의 무작위. 잠금은 간격 하나를 뽑는 동안만 쥔다.
    backoff: Mutex<RetryBackoff>,
}

impl Retrier {
    /// 다시 시도 횟수 `max_retries`, `Retry-After` 단위, 간격 계산기로 만든다.
    pub(crate) fn new(max_retries: u32, retry_after_unit: Duration, backoff: RetryBackoff) -> Self {
        Self {
            max_retries,
            retry_after_unit,
            backoff: Mutex::new(backoff),
        }
    }

    /// `attempt`를 성공하거나 다시 시도가 끝날 때까지 부른다. 마지막 오류를 돌려준다.
    pub(crate) async fn run<F, Fut>(&self, mut attempt: F) -> Result<Vec<u8>, AiError>
    where
        F: FnMut() -> Fut + Send,
        Fut: Future<Output = Result<Vec<u8>, Failure>> + Send,
    {
        let mut retries = 0;
        loop {
            let failure = match attempt().await {
                Ok(bytes) => return Ok(bytes),
                Err(failure) => failure,
            };
            if retries >= self.max_retries || !retry::is_retryable(&failure.error) {
                return Err(failure.error);
            }
            retries += 1;
            let wait = self.wait_before(retries, failure.retry_after);
            tokio::time::sleep(wait).await;
        }
    }

    /// `retry`번째(1부터) 다시 시도 전에 기다릴 시간. `Retry-After`가 있으면 그 값이 우선이다.
    fn wait_before(&self, retry: u32, retry_after: Option<Duration>) -> Duration {
        if let Some(after) = retry_after {
            return retry::scale_wait(self.retry_after_unit, after);
        }
        self.backoff
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .delay(retry)
    }
}
