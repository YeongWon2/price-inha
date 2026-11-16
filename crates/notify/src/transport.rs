//! 푸시 서비스로 보내는 길. 시험은 로컬 가짜 서버로 한다.

use std::time::Duration;

use async_trait::async_trait;
use url::Url;

use crate::Error;

/// 푸시 서비스에 보낼 요청 하나(POST).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PushRequest {
    /// 구독의 `endpoint`(그대로).
    pub endpoint: String,
    /// 머리글(이름, 값).
    pub headers: Vec<(String, String)>,
    /// 본문(암호화된 바이트).
    pub body: Vec<u8>,
}

/// 푸시 서비스의 응답.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PushResponse {
    /// HTTP 상태 코드.
    pub status: u16,
}

/// 푸시 서비스로 요청을 보내는 자리.
#[async_trait]
pub trait PushTransport: Send + Sync {
    /// 요청을 보내고 상태 코드를 돌려준다. 연결 · 읽기 실패는 [`Error::Transport`]다
    /// (4xx · 5xx 응답은 오류가 아니라 `PushResponse`다).
    async fn post(&self, request: PushRequest) -> Result<PushResponse, Error>;
}

/// reqwest(rustls)로 보내는 실제 길.
#[derive(Debug, Clone)]
pub struct HttpTransport {
    _private: (),
}

impl HttpTransport {
    /// 만든다. `request_timeout`은 요청 하나의 제한 시간이다.
    pub fn new(request_timeout: Duration) -> Result<HttpTransport, Error> {
        let _ = request_timeout;
        todo!()
    }

    /// 시험용: 모든 요청의 scheme · host · port를 `base`로 바꿔 보낸다(경로 · 쿼리는 `endpoint` 것 그대로).
    /// 구독의 `endpoint`는 `https://`여야 하므로, 로컬 가짜 서버(http)로 보내려면 이 길이 필요하다.
    pub fn redirect_to(self, base: Url) -> HttpTransport {
        let _ = base;
        todo!()
    }
}

#[async_trait]
impl PushTransport for HttpTransport {
    async fn post(&self, request: PushRequest) -> Result<PushResponse, Error> {
        let _ = request;
        todo!()
    }
}
