//! 네이버 뉴스 검색 API 공급자(4차 3번).
//!
//! 공식 문서 "검색 > 뉴스"(developers.naver.com, 2026-11-21 확인)의 모양을 따른다.
//!
//! - 요청: `GET {base}/v1/search/news.json?query=<UTF-8 퍼센트 인코딩>&display=<max_items>&start=1&sort=<date|sim>`,
//!   머리글 `X-Naver-Client-Id` · `X-Naver-Client-Secret`. 키는 쿼리에 싣지 않는다.
//! - 응답: `items[]`의 `title` · `description` · `originallink` · `link` · `pubDate`만 읽는다.
//!   `pubDate`는 RFC 2822(`Mon, 26 Sep 2016 07:50:00 +0900`).
//! - 오류: 400 `SE01`~`SE04` · `SE06` → [`NewsError::Rejected`], 401 · 403 → [`NewsError::KeyRejected`],
//!   429 → [`NewsError::RateLimited`], 404 `SE05` · 3xx → [`NewsError::BadAddress`],
//!   500 `SE99` · 그 밖의 5xx · 연결 실패 → [`NewsError::Unreachable`].
//!   오류 본문은 `{"errorMessage": .., "errorCode": ..}`(공통 가이드).

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use pi_secrets::{Credentials, VerifyFailure};

use crate::{Clock, NewsError, NewsFetched, NewsKeys, NewsProvider, NewsProviderId, NewsRequest};

/// 네이버 오픈API 기본 주소.
pub const DEFAULT_NAVER_NEWS_BASE_URL: &str = "https://openapi.naver.com";

/// 응답 본문의 기본 상한(기존 자료 어댑터와 같은 16MiB).
pub const DEFAULT_NEWS_MAX_BODY_BYTES: usize = 16 * 1024 * 1024;

/// 네이버 뉴스 검색의 키 칸 선언: 공급자 `naver_news`, 칸 `client_id` · `client_secret`.
/// (보관소 줄 이름은 secrets 8차 규칙으로 `PRICE_INHA_KEY__NAVER_NEWS__CLIENT_ID` · `..__CLIENT_SECRET`.)
pub const NAVER_NEWS_KEYS: NewsKeys = NewsKeys::new("naver_news", &["client_id", "client_secret"]);

/// 네이버 뉴스 설정. 기본 주소는 인자로 받는다(테스트는 로컬 가짜 서버 주소를 준다).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NaverNewsConfig {
    /// 기본 주소.
    pub base_url: String,
    /// 요청 하나의 시간 한도. 기본은 [`crate::DEFAULT_TIMEOUT`].
    pub timeout: Duration,
    /// 응답 본문 상한(바이트). 넘으면 읽지 않고 [`NewsError::Format`].
    pub max_body_bytes: usize,
}

impl NaverNewsConfig {
    /// 그 기본 주소로 만든다. 시간 한도 · 본문 상한은 기본값.
    pub fn new(base_url: impl Into<String>) -> Self {
        let _ = base_url.into();
        todo!()
    }

    /// 공식 주소([`DEFAULT_NAVER_NEWS_BASE_URL`])로 만든다.
    pub fn official() -> Self {
        todo!()
    }

    /// 시간 한도를 바꾼다.
    #[must_use]
    pub fn with_timeout(self, timeout: Duration) -> Self {
        let _ = timeout;
        todo!()
    }

    /// 본문 상한을 바꾼다.
    #[must_use]
    pub fn with_max_body_bytes(self, max_body_bytes: usize) -> Self {
        let _ = max_body_bytes;
        todo!()
    }
}

/// 뉴스 공급자 키를 주는 곳. 요청할 때마다 묻는다(나중에 넣은 키도 바로 쓴다).
///
/// - 칸이 다 있으면 `Ok(Some(..))`, 하나라도 없으면 `Ok(None)` 또는 그 칸이 빠진 묶음
///   (공급자는 칸이 모두 있어야 요청한다, 아니면 [`NewsError::NoKey`]).
/// - 읽지 못했으면(입출력 · 손상) [`NewsError::KeyUnreadable`].
///
/// `SecretStore`(`read_provider`)에 잇는 구현은 secrets 8차의 `ProviderKeys`가 생긴 뒤 server/secrets 차수에서 한다.
pub trait NewsKeySource: Send + Sync {
    /// 그 선언의 칸 값들.
    fn read_news_keys(&self, keys: NewsKeys) -> Result<Option<Credentials>, NewsError>;
}

/// 네이버 뉴스 검색 공급자. reqwest(rustls)를 쓰고 리다이렉트를 따라가지 않는다.
pub struct NaverNews {
    config: NaverNewsConfig,
    keys: Arc<dyn NewsKeySource>,
    clock: Arc<dyn Clock>,
}

impl NaverNews {
    /// 만든다. 기본 주소가 `https`가 아니면(루프백 주소는 예외) [`NewsError::InsecureBaseUrl`].
    pub fn new(
        config: NaverNewsConfig,
        keys: Arc<dyn NewsKeySource>,
        clock: Arc<dyn Clock>,
    ) -> Result<Self, NewsError> {
        let _ = (config, keys, clock);
        todo!()
    }

    /// 만들 때 받은 설정.
    pub fn config(&self) -> &NaverNewsConfig {
        let _ = (&self.keys, &self.clock);
        &self.config
    }
}

#[async_trait]
impl NewsProvider for NaverNews {
    fn id(&self) -> NewsProviderId {
        todo!()
    }

    /// [`NAVER_NEWS_KEYS`].
    fn keys(&self) -> NewsKeys {
        todo!()
    }

    /// 검색한다. 받은 시각(시계)보다 뒤의 기사는 버린다(V36). 남은 것이 없으면 [`NewsFetched::Missing`].
    async fn search(&self, request: &NewsRequest) -> Result<NewsFetched, NewsError> {
        let _ = request;
        todo!()
    }
}

/// 확인기: 이 키로 한 번 검색(`display=1`)한다.
///
/// 200이면 `Ok`, 401 · 403이면 [`VerifyFailure::Rejected`], 그 밖(연결 실패 · 시간 초과 · 5xx · 404 · 3xx)과
/// 칸이 빠진 값은 [`VerifyFailure::Unreachable`](빠진 칸이면 요청하지 않는다). 리다이렉트는 따라가지 않는다.
/// server가 secrets의 `verify_provider`를 이것으로 구현한다(server 차수).
pub async fn verify_news_keys(
    config: &NaverNewsConfig,
    values: &Credentials,
) -> Result<(), VerifyFailure> {
    let _ = (config, values);
    todo!()
}
