//! 푸시 서비스로 보내는 길. 시험은 로컬 가짜 서버로 한다.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use url::Url;

use crate::{Error, Resolver, SystemResolver, pin_endpoint};

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
#[derive(Clone)]
pub struct HttpTransport {
    request_timeout: Duration,
    resolver: Arc<dyn Resolver>,
    redirect: Option<(reqwest::Client, Url)>,
}

impl std::fmt::Debug for HttpTransport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HttpTransport")
            .field("request_timeout", &self.request_timeout)
            .field(
                "redirect",
                &self.redirect.as_ref().map(|(_, base)| base.as_str()),
            )
            .finish_non_exhaustive()
    }
}

impl HttpTransport {
    /// 만든다. `request_timeout`은 요청 하나의 제한 시간이다. 이름 해석은 [`SystemResolver`]다.
    pub fn new(request_timeout: Duration) -> Result<HttpTransport, Error> {
        Self::with_resolver(request_timeout, Arc::new(SystemResolver))
    }

    /// [`HttpTransport::new`]와 같되 이름 해석기를 받는다(`new`는 [`crate::SystemResolver`]).
    /// 보낼 때마다 [`crate::pin_endpoint`]로 구독 주소를 검사하고, 내부망이면 보내지 않고
    /// [`Error::InternalAddress`]를 돌려준다. 접속은 검사한 주소로만 한다(reqwest `resolve_to_addrs`).
    /// 리다이렉트는 따라가지 않고, 프록시를 쓰지 않는다.
    pub fn with_resolver(
        request_timeout: Duration,
        resolver: Arc<dyn Resolver>,
    ) -> Result<HttpTransport, Error> {
        // 설정이 맞는지 미리 한 번 만들어 본다.
        base_builder(request_timeout)
            .build()
            .map_err(transport_error)?;
        Ok(HttpTransport {
            request_timeout,
            resolver,
            redirect: None,
        })
    }

    #[cfg(feature = "test-redirect")]
    /// 시험용: 모든 요청의 scheme · host · port를 `base`로 바꿔 보낸다(경로 · 쿼리는 `endpoint` 것 그대로).
    /// 구독의 `endpoint`는 `https://`여야 하므로, 로컬 가짜 서버(http)로 보내려면 이 길이 필요하다.
    /// 이 길은 내부망 검사를 하지 않는다(시험 서버가 127.0.0.1이다).
    pub fn redirect_to(self, base: Url) -> HttpTransport {
        let client = base_builder(self.request_timeout)
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        HttpTransport {
            redirect: Some((client, base)),
            ..self
        }
    }

    /// 검사한 주소로만 접속하는 클라이언트와 보낼 주소.
    async fn pinned(&self, endpoint: &str) -> Result<(reqwest::Client, Url), Error> {
        let url = Url::parse(endpoint)
            .map_err(|_| Error::Transport("구독 주소가 주소 모양이 아니다".to_owned()))?;
        let pinned = pin_endpoint(endpoint, self.resolver.as_ref()).await?;
        let client = base_builder(self.request_timeout)
            .resolve_to_addrs(&pinned.host, &pinned.addrs)
            .build()
            .map_err(transport_error)?;
        Ok((client, url))
    }
}

/// 모든 길이 같이 쓰는 설정: 제한 시간, 리다이렉트 안 따라감, 프록시 안 씀.
fn base_builder(request_timeout: Duration) -> reqwest::ClientBuilder {
    reqwest::Client::builder()
        .timeout(request_timeout)
        .redirect(reqwest::redirect::Policy::none())
        .no_proxy()
}

fn transport_error(e: reqwest::Error) -> Error {
    Error::Transport(e.without_url().to_string())
}

#[async_trait]
impl PushTransport for HttpTransport {
    async fn post(&self, request: PushRequest) -> Result<PushResponse, Error> {
        let (client, target) = match &self.redirect {
            Some((client, base)) => (client.clone(), redirected(base, &request.endpoint)?),
            None => self.pinned(&request.endpoint).await?,
        };
        let mut builder = client.post(target);
        for (name, value) in &request.headers {
            builder = builder.header(name.as_str(), value.as_str());
        }
        let response = builder
            .body(request.body)
            .send()
            .await
            .map_err(transport_error)?;
        Ok(PushResponse {
            status: response.status().as_u16(),
        })
    }
}

/// 시험용 길: `endpoint`의 경로 · 쿼리를 `base`에 붙인다.
fn redirected(base: &Url, endpoint: &str) -> Result<Url, Error> {
    let url = Url::parse(endpoint)
        .map_err(|_| Error::Transport("구독 주소가 주소 모양이 아니다".to_owned()))?;
    let mut target = base.clone();
    target.set_path(url.path());
    target.set_query(url.query());
    Ok(target)
}
