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
use chrono::{DateTime, Utc};
use pi_secrets::{Credentials, VerifyFailure};
use reqwest::header::{HeaderMap, HeaderValue};
use reqwest::{Client, StatusCode};
use serde_json::Value;
use url::Url;

use crate::http::{is_allowed_scheme, with_segments};
use crate::news::{MAX_SUMMARY_CHARS, MAX_TITLE_CHARS, NewsSort, truncate_chars};
use crate::{
    Clock, DEFAULT_TIMEOUT, NewsError, NewsFetched, NewsItem, NewsKeys, NewsProvider,
    NewsProviderId, NewsRequest, clean_news_text,
};

/// 이 공급자의 이름.
const PROVIDER: NewsProviderId = NewsProviderId::Naver;

/// 뉴스 검색 경로 조각(`/v1/search/news.json`).
const NEWS_SEGMENTS: [&str; 3] = ["v1", "search", "news.json"];

/// 클라이언트 ID 머리글.
const ID_HEADER: &str = "X-Naver-Client-Id";

/// 클라이언트 시크릿 머리글.
const SECRET_HEADER: &str = "X-Naver-Client-Secret";

/// 키 칸 이름들(선언 순서: 클라이언트 ID, 클라이언트 시크릿). [`NAVER_NEWS_KEYS`]와 머리글이 함께 쓴다.
const NAVER_NEWS_FIELDS: [&str; 2] = ["client_id", "client_secret"];

/// 클라이언트 ID 칸.
const ID_FIELD: &str = NAVER_NEWS_FIELDS[0];

/// 클라이언트 시크릿 칸.
const SECRET_FIELD: &str = NAVER_NEWS_FIELDS[1];

/// 오류 본문에서 받아들이는 오류 코드의 길이 상한(`SE01` · `024` 같은 짧은 코드만).
const MAX_ERROR_CODE_LEN: usize = 16;

/// 네이버 오픈API 기본 주소.
pub const DEFAULT_NAVER_NEWS_BASE_URL: &str = "https://openapi.naver.com";

/// 응답 본문의 기본 상한(기존 자료 어댑터와 같은 16MiB).
pub const DEFAULT_NEWS_MAX_BODY_BYTES: usize = 16 * 1024 * 1024;

/// 네이버 뉴스 검색의 키 칸 선언: 공급자 `naver_news`, 칸 `client_id` · `client_secret`.
/// (보관소 줄 이름은 secrets 8차 규칙으로 `PRICE_INHA_KEY__NAVER_NEWS__CLIENT_ID` · `..__CLIENT_SECRET`.)
/// 보관소 선언은 [`NewsKeys::provider_keys`]로 이 선언에서 만든다(이름 · 칸을 따로 적지 않는다).
pub const NAVER_NEWS_KEYS: NewsKeys = NewsKeys::new("naver_news", &NAVER_NEWS_FIELDS);

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
        Self {
            base_url: base_url.into(),
            timeout: DEFAULT_TIMEOUT,
            max_body_bytes: DEFAULT_NEWS_MAX_BODY_BYTES,
        }
    }

    /// 공식 주소([`DEFAULT_NAVER_NEWS_BASE_URL`])로 만든다.
    pub fn official() -> Self {
        Self::new(DEFAULT_NAVER_NEWS_BASE_URL)
    }

    /// 시간 한도를 바꾼다.
    #[must_use]
    pub fn with_timeout(self, timeout: Duration) -> Self {
        Self { timeout, ..self }
    }

    /// 본문 상한을 바꾼다.
    #[must_use]
    pub fn with_max_body_bytes(self, max_body_bytes: usize) -> Self {
        Self {
            max_body_bytes,
            ..self
        }
    }
}

/// 뉴스 공급자 키를 주는 곳. 요청할 때마다 묻는다(나중에 넣은 키도 바로 쓴다).
///
/// - 칸이 다 있으면 `Ok(Some(..))`, 하나라도 없으면 `Ok(None)` 또는 그 칸이 빠진 묶음
///   (공급자는 칸이 모두 있어야 요청한다, 아니면 [`NewsError::NoKey`]).
/// - 읽지 못했으면(입출력 · 손상) [`NewsError::KeyUnreadable`].
///
/// `pi_secrets::SecretStore`는 `read_provider`로 이것을 구현한다.
pub trait NewsKeySource: Send + Sync {
    /// 그 선언의 칸 값들.
    fn read_news_keys(&self, keys: NewsKeys) -> Result<Option<Credentials>, NewsError>;
}

/// 네이버 뉴스 검색 공급자. reqwest(rustls)를 쓰고 리다이렉트를 따라가지 않는다.
pub struct NaverNews {
    config: NaverNewsConfig,
    endpoint: Endpoint,
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
        let endpoint = Endpoint::new(&config)?;
        Ok(Self {
            config,
            endpoint,
            keys,
            clock,
        })
    }

    /// 만들 때 받은 설정.
    pub fn config(&self) -> &NaverNewsConfig {
        &self.config
    }
}

#[async_trait]
impl NewsProvider for NaverNews {
    fn id(&self) -> NewsProviderId {
        PROVIDER
    }

    /// [`NAVER_NEWS_KEYS`].
    fn keys(&self) -> NewsKeys {
        NAVER_NEWS_KEYS
    }

    /// 검색한다. 받은 시각(시계)보다 뒤의 기사는 버린다(V36). 남은 것이 없으면 [`NewsFetched::Missing`].
    async fn search(&self, request: &NewsRequest) -> Result<NewsFetched, NewsError> {
        let query = request.checked_query(PROVIDER)?;
        let credentials = self
            .keys
            .read_news_keys(NAVER_NEWS_KEYS)?
            .ok_or(NewsError::NoKey { provider: PROVIDER })?;
        let headers = key_headers(&credentials).ok_or(NewsError::NoKey { provider: PROVIDER })?;
        let body = self
            .endpoint
            .get(query, request.max_items, request.sort, headers)
            .await?;
        let known_at = self.clock.now();
        let items = parse_items(&body, query, known_at)?;
        if items.is_empty() {
            Ok(NewsFetched::Missing)
        } else {
            Ok(NewsFetched::Items(items))
        }
    }
}

/// 확인기: 이 키로 한 번 검색(`display=1`)한다.
///
/// 200이면 `Ok`(200이 아닌 2xx는 닿지 않음), 401 · 403이면 [`VerifyFailure::Rejected`], 그 밖(연결 실패 · 시간 초과 · 5xx · 404 · 3xx)과
/// 칸이 빠진 값은 [`VerifyFailure::Unreachable`](빠진 칸이면 요청하지 않는다). 리다이렉트는 따라가지 않는다.
/// server가 secrets의 `verify_provider`를 이것으로 구현한다(server 차수).
pub async fn verify_news_keys(
    config: &NaverNewsConfig,
    values: &Credentials,
) -> Result<(), VerifyFailure> {
    let headers = key_headers(values).ok_or(VerifyFailure::Unreachable)?;
    let endpoint = Endpoint::new(config).map_err(|_| VerifyFailure::Unreachable)?;
    match endpoint.get("주식", 1, NewsSort::Latest, headers).await {
        Ok(_) => Ok(()),
        Err(NewsError::KeyRejected { .. }) => Err(VerifyFailure::Rejected),
        Err(_) => Err(VerifyFailure::Unreachable),
    }
}

/// 두 칸의 값으로 머리글을 만든다. 칸이 빠졌거나 비었거나 머리글에 쓸 수 없는 글자면 `None`.
/// 머리글 값은 민감한 값으로 표시한다(기록 · `Debug`에 나오지 않게).
fn key_headers(credentials: &Credentials) -> Option<HeaderMap> {
    let header = |field: &str| {
        let value = credentials.get(field)?.expose().trim();
        if value.is_empty() {
            return None;
        }
        let mut header = HeaderValue::from_str(value).ok()?;
        header.set_sensitive(true);
        Some(header)
    };
    let mut headers = HeaderMap::new();
    headers.insert(ID_HEADER, header(ID_FIELD)?);
    headers.insert(SECRET_HEADER, header(SECRET_FIELD)?);
    Some(headers)
}

/// 네이버로 보내는 곳: 규칙을 지난 기본 주소와 시간 한도 · 리다이렉트 금지를 건 클라이언트.
struct Endpoint {
    base: Url,
    http: Client,
    max_body_bytes: usize,
}

impl Endpoint {
    /// 기본 주소가 `https`(루프백은 `http`도)가 아니면 [`NewsError::InsecureBaseUrl`].
    fn new(config: &NaverNewsConfig) -> Result<Self, NewsError> {
        let base = Url::parse(config.base_url.trim_end_matches('/'))
            .ok()
            .filter(is_allowed_scheme)
            .ok_or(NewsError::InsecureBaseUrl { provider: PROVIDER })?;
        let http = Client::builder()
            .timeout(config.timeout)
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|_| unreachable_error("HTTP 클라이언트를 만들 수 없습니다"))?;
        Ok(Self {
            base,
            http,
            max_body_bytes: config.max_body_bytes,
        })
    }

    /// 뉴스 검색을 한 번 보내고 200 본문을 글로 돌려준다. 다시 시도하지 않는다.
    /// 200이 아닌 2xx(204 등)는 성공으로 보지 않고 [`NewsError::Format`]이다.
    async fn get(
        &self,
        query: &str,
        display: u16,
        sort: NewsSort,
        headers: HeaderMap,
    ) -> Result<String, NewsError> {
        let mut url = with_segments(&self.base, &NEWS_SEGMENTS);
        url.query_pairs_mut()
            .append_pair("query", query)
            .append_pair("display", &display.to_string())
            .append_pair("start", "1")
            .append_pair("sort", sort_param(sort));
        let mut response = self
            .http
            .get(url)
            .headers(headers)
            .send()
            .await
            .map_err(|err| transport_error(&err))?;
        let status = response.status();
        let mut bytes: Vec<u8> = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|err| transport_error(&err))?
        {
            if bytes.len().saturating_add(chunk.len()) > self.max_body_bytes {
                if status.is_success() {
                    return Err(format_error("응답이 본문 상한보다 큽니다"));
                }
                return Err(status_error(status, &[]));
            }
            bytes.extend_from_slice(&chunk);
        }
        if status.is_success() && status != StatusCode::OK {
            return Err(format_error("응답 상태가 200이 아닙니다"));
        }
        if !status.is_success() {
            return Err(status_error(status, &bytes));
        }
        String::from_utf8(bytes).map_err(|_| format_error("응답이 UTF-8 글이 아닙니다"))
    }
}

/// 정렬을 네이버 `sort` 값으로.
fn sort_param(sort: NewsSort) -> &'static str {
    match sort {
        NewsSort::Latest => "date",
        NewsSort::Relevance => "sim",
    }
}

/// reqwest 오류를 고친 글로만 옮긴다(reqwest 글은 주소 · 검색어를 담을 수 있다).
fn transport_error(err: &reqwest::Error) -> NewsError {
    if err.is_timeout() {
        return NewsError::Timeout { provider: PROVIDER };
    }
    let detail = if err.is_connect() {
        "연결 실패"
    } else if err.is_body() || err.is_decode() {
        "응답을 받는 중 끊김"
    } else {
        "요청 실패"
    };
    unreachable_error(detail)
}

/// 공식 오류표의 갈래.
fn status_error(status: StatusCode, body: &[u8]) -> NewsError {
    let code = || error_code(body).unwrap_or_else(|| status.as_u16().to_string());
    if status == StatusCode::TOO_MANY_REQUESTS {
        return NewsError::RateLimited { provider: PROVIDER };
    }
    if status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN {
        return NewsError::KeyRejected {
            provider: PROVIDER,
            code: code(),
        };
    }
    if status == StatusCode::NOT_FOUND || status.is_redirection() {
        return NewsError::BadAddress { provider: PROVIDER };
    }
    if status.is_server_error() {
        return unreachable_error(&format!("HTTP {}", status.as_u16()));
    }
    NewsError::Rejected {
        provider: PROVIDER,
        code: code(),
    }
}

/// 오류 본문 `{"errorCode": ..}`의 코드. 짧은 영숫자 코드만 받는다(본문의 다른 글은 싣지 않는다).
fn error_code(body: &[u8]) -> Option<String> {
    let value: Value = serde_json::from_slice(body).ok()?;
    let code = value.get("errorCode")?.as_str()?;
    let fits = !code.is_empty()
        && code.len() <= MAX_ERROR_CODE_LEN
        && code.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    fits.then(|| code.to_owned())
}

fn unreachable_error(detail: &str) -> NewsError {
    NewsError::Unreachable {
        provider: PROVIDER,
        detail: detail.to_owned(),
    }
}

fn format_error(detail: &str) -> NewsError {
    NewsError::Format {
        provider: PROVIDER,
        detail: detail.to_owned(),
    }
}

/// 응답 본문을 기사들로 읽는다. 한 기사라도 칸이 없거나 깨졌으면 응답 전체가 형식 오류다.
/// `known_at`보다 뒤의 기사는 버린다(V36).
fn parse_items(
    body: &str,
    query: &str,
    known_at: DateTime<Utc>,
) -> Result<Vec<NewsItem>, NewsError> {
    let value: Value =
        serde_json::from_str(body).map_err(|_| format_error("응답이 JSON이 아닙니다"))?;
    let items = value
        .get("items")
        .and_then(Value::as_array)
        .ok_or_else(|| format_error("items 목록이 없습니다"))?;
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        let news = parse_item(item, query, known_at)?;
        if news.published_at <= known_at {
            out.push(news);
        }
    }
    Ok(out)
}

fn parse_item(item: &Value, query: &str, known_at: DateTime<Utc>) -> Result<NewsItem, NewsError> {
    let text = |name: &str| {
        item.get(name)
            .and_then(Value::as_str)
            .ok_or_else(|| format_error(&format!("기사에 {name} 글이 없습니다")))
    };
    let title = text("title")?;
    let description = text("description")?;
    let original = text("originallink")?;
    let naver = text("link")?;
    let pub_date = text("pubDate")?;
    let chosen = if original.trim().is_empty() {
        naver
    } else {
        original
    };
    let link = web_link(chosen)
        .ok_or_else(|| format_error("기사 링크가 쓸 수 있는 http(s) 주소가 아닙니다"))?;
    let published_at = DateTime::parse_from_rfc2822(pub_date.trim())
        .map_err(|_| format_error("기사 시각(pubDate)을 읽을 수 없습니다"))?
        .with_timezone(&Utc);
    Ok(NewsItem {
        provider: PROVIDER,
        query: query.to_owned(),
        title: truncate_chars(clean_news_text(title), MAX_TITLE_CHARS),
        summary: truncate_chars(clean_news_text(description), MAX_SUMMARY_CHARS),
        link,
        published_at,
        known_at,
    })
}

/// `http` · `https`의 절대 주소만 받고, 주소 해석기가 정규화한 글(`Url::as_str`)을 돌려준다.
///
/// 앞뒤 공백을 뺀 원문에 제어 문자 · 공백 · `"` `<` `>` `\\` `` ` ``가 있거나, 사용자 정보(`user@host`)가
/// 있거나, 호스트가 없으면 `None`(기사 전체가 형식 오류).
fn web_link(text: &str) -> Option<String> {
    let trimmed = text.trim();
    if trimmed.chars().any(is_forbidden_link_char) || !has_authority(trimmed) {
        return None;
    }
    let url = Url::parse(trimmed).ok()?;
    let web = matches!(url.scheme(), "http" | "https")
        && url.host_str().is_some_and(|host| !host.is_empty())
        && url.username().is_empty()
        && url.password().is_none();
    web.then(|| url.as_str().to_owned())
}

/// 원문이 `http://호스트` · `https://호스트` 꼴로 쓰였는가(`http:a` · `http:///a`처럼
/// 해석기가 고쳐 읽는 꼴은 받지 않는다).
fn has_authority(text: &str) -> bool {
    let lower = text.get(..8).unwrap_or(text).to_ascii_lowercase();
    let rest = if lower.starts_with("https://") {
        &text[8..]
    } else if lower.starts_with("http://") {
        &text[7..]
    } else {
        return false;
    };
    !rest.is_empty() && !rest.starts_with('/')
}

/// 링크 원문에 있으면 거절하는 글자.
fn is_forbidden_link_char(ch: char) -> bool {
    ch.is_control() || ch.is_whitespace() || matches!(ch, '"' | '<' | '>' | '\\' | '`')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 오류_코드는_짧은_영숫자만_싣는다() {
        assert_eq!(
            error_code(br#"{"errorCode":"SE01"}"#),
            Some("SE01".to_owned())
        );
        assert_eq!(error_code(br#"{"errorCode":"id secret"}"#), None);
        assert_eq!(
            error_code(br#"{"errorCode":"AAAAAAAAAAAAAAAAAAAAAAAA"}"#),
            None
        );
        assert_eq!(error_code(b"not json"), None);
        assert_eq!(
            status_error(StatusCode::BAD_REQUEST, b"x"),
            NewsError::Rejected {
                provider: PROVIDER,
                code: "400".to_owned()
            }
        );
    }

    #[test]
    fn 링크는_정규화한_글을_돌려준다() {
        assert_eq!(
            web_link(" https://News.Example.com/a?b=1 "),
            Some("https://news.example.com/a?b=1".to_owned())
        );
        assert_eq!(
            web_link("https://news.example.com"),
            Some("https://news.example.com/".to_owned())
        );
        assert_eq!(
            web_link("https://news.example.com/기사"),
            Some("https://news.example.com/%EA%B8%B0%EC%82%AC".to_owned())
        );
    }

    #[test]
    fn 위험한_글자나_사용자_정보가_있는_링크는_거절한다() {
        for bad in [
            "https://news.example.com/a\nb",
            "https://news.example.com/a\tb",
            "https://news.example.com/a\u{0}b",
            "https://news.example.com/a\rb",
            "https://news.example.com/a b",
            "https://news.example.com/\"x",
            "https://news.example.com/<x>",
            "https://news.example.com\\evil.example.com",
            "https://user@news.example.com/a",
            "https://user:pw@news.example.com/a",
            "https://:pw@news.example.com/a",
            "http:///a",
            "http:news.example.com/a",
            "https:/news.example.com/a",
            "https://",
            "javascript:alert(1)",
            "mailto:a@example.com",
        ] {
            assert_eq!(web_link(bad), None, "{bad:?}");
        }
    }

    #[tokio::test]
    async fn 이백이_아닌_2xx는_성공이_아니다() {
        use wiremock::matchers::method;
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(204))
            .mount(&server)
            .await;
        let config = NaverNewsConfig::new(server.uri());
        let endpoint = Endpoint::new(&config).unwrap();
        let headers = key_headers(
            &Credentials::new()
                .with(ID_FIELD, "fake-id-00000001")
                .with(SECRET_FIELD, "fake-secret-0001"),
        )
        .unwrap();
        let result = endpoint.get("주식", 1, NewsSort::Latest, headers).await;
        assert!(
            matches!(result, Err(NewsError::Format { .. })),
            "{result:?}"
        );
        let creds = Credentials::new()
            .with(ID_FIELD, "fake-id-00000001")
            .with(SECRET_FIELD, "fake-secret-0001");
        assert_eq!(
            verify_news_keys(&config, &creds).await,
            Err(VerifyFailure::Unreachable)
        );
    }

    #[test]
    fn 빈_칸_값은_키가_없는_것이다() {
        let creds = Credentials::new()
            .with(ID_FIELD, " ")
            .with(SECRET_FIELD, "s");
        assert!(key_headers(&creds).is_none());
    }
}
