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
    client: reqwest::Client,
    redirect: Option<Url>,
}

impl HttpTransport {
    /// 만든다. `request_timeout`은 요청 하나의 제한 시간이다.
    pub fn new(request_timeout: Duration) -> Result<HttpTransport, Error> {
        let client = reqwest::Client::builder()
            .timeout(request_timeout)
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|e| Error::Transport(e.without_url().to_string()))?;
        Ok(HttpTransport {
            client,
            redirect: None,
        })
    }

    /// [`HttpTransport::new`]와 같되 이름 해석기를 받는다(`new`는 [`crate::SystemResolver`]).
    /// 보낼 때마다 [`crate::pin_endpoint`]로 구독 주소를 검사하고, 내부망이면 보내지 않고
    /// [`Error::InternalAddress`]를 돌려준다. 접속은 검사한 주소로만 한다(reqwest `resolve_to_addrs`).
    /// 리다이렉트는 따라가지 않는다.
    pub fn with_resolver(
        request_timeout: Duration,
        resolver: std::sync::Arc<dyn crate::Resolver>,
    ) -> Result<HttpTransport, Error> {
        let _ = (request_timeout, resolver);
        todo!()
    }

    /// 시험용: 모든 요청의 scheme · host · port를 `base`로 바꿔 보낸다(경로 · 쿼리는 `endpoint` 것 그대로).
    /// 구독의 `endpoint`는 `https://`여야 하므로, 로컬 가짜 서버(http)로 보내려면 이 길이 필요하다.
    pub fn redirect_to(self, base: Url) -> HttpTransport {
        HttpTransport {
            redirect: Some(base),
            ..self
        }
    }
}

impl HttpTransport {
    /// 실제로 보낼 주소. `redirect_to`가 있으면 scheme · host · port만 바꾼다.
    fn target(&self, endpoint: &str) -> Result<Url, Error> {
        let url = Url::parse(endpoint)
            .map_err(|_| Error::Transport("구독 주소가 주소 모양이 아니다".to_owned()))?;
        let Some(base) = &self.redirect else {
            return Ok(url);
        };
        let mut target = base.clone();
        target.set_path(url.path());
        target.set_query(url.query());
        Ok(target)
    }
}

#[async_trait]
impl PushTransport for HttpTransport {
    async fn post(&self, request: PushRequest) -> Result<PushResponse, Error> {
        let target = self.target(&request.endpoint)?;
        let mut builder = self.client.post(target);
        for (name, value) in &request.headers {
            builder = builder.header(name.as_str(), value.as_str());
        }
        let response = builder
            .body(request.body)
            .send()
            .await
            .map_err(|e| Error::Transport(e.without_url().to_string()))?;
        Ok(PushResponse {
            status: response.status().as_u16(),
        })
    }
}
