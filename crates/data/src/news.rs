//! 뉴스 값 · 뉴스 공급자 trait · 등록부 (4차, 결정 2026-11-21 2 · 3번).
//!
//! 뉴스는 [`crate::FactKind`] 밖의 따로 된 값이다(결정 13번 · 지시서 "잠긴 옛 테스트와 부딪히는 곳").
//! 받는 칸은 제목 · 요약 · 링크 · 시각뿐이다. 본문 · 언론사 · 썸네일은 받지 않는다.

use std::fmt;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::{DateTime, Utc};

use crate::NewsError;

/// 뉴스 공급자 이름.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum NewsProviderId {
    /// 네이버 뉴스 검색 API.
    Naver,
}

impl NewsProviderId {
    /// 기계용 이름(`"naver"`).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Naver => "naver",
        }
    }

    /// [`NewsProviderId::as_str`]의 글자에서 되읽는다. 모르는 이름이면 `None`.
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "naver" => Some(Self::Naver),
            _ => None,
        }
    }
}

impl fmt::Display for NewsProviderId {
    /// [`NewsProviderId::as_str`]와 같다.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// 공급자가 쓰는 키 칸의 선언(공급자 이름 + 칸 이름들).
///
/// `pi_secrets::ProviderKeys`(secrets 8차)와 같은 모양의 이 모듈 쪽 선언이다. secrets와의 연결
/// (보관소에서 읽기 · 화면 키 칸 목록)은 server/secrets 차수에서 잇는다.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NewsKeys {
    provider: &'static str,
    fields: &'static [&'static str],
}

impl NewsKeys {
    /// 선언한다(상수로 쓸 수 있게 검사하지 않는다).
    pub const fn new(provider: &'static str, fields: &'static [&'static str]) -> Self {
        Self { provider, fields }
    }

    /// 공급자 이름(예: `naver_news`).
    pub fn provider(&self) -> &'static str {
        self.provider
    }

    /// 칸 이름들(선언한 순서).
    pub fn fields(&self) -> &'static [&'static str] {
        self.fields
    }
}

/// 정렬.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum NewsSort {
    /// 최신순(네이버 `sort=date`).
    Latest,
    /// 정확도순(네이버 `sort=sim`).
    Relevance,
}

/// 뉴스 검색 요청.
///
/// 확인(요청을 보내기 전에): 앞뒤 공백을 뺀 검색어가 비었거나 100자를 넘거나 제어 문자가 있으면,
/// `max_items`가 1~100 밖이면 [`NewsError::InvalidRequest`]이고 바깥으로 요청하지 않는다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewsRequest {
    /// 검색어.
    pub query: String,
    /// 받을 기사 수의 상한(1~100).
    pub max_items: u16,
    /// 정렬.
    pub sort: NewsSort,
}

impl NewsRequest {
    /// 최신순 요청.
    pub fn latest(query: impl Into<String>, max_items: u16) -> Self {
        Self {
            query: query.into(),
            max_items,
            sort: NewsSort::Latest,
        }
    }

    /// 요청을 확인하고 앞뒤 공백을 뺀 검색어를 돌려준다. 틀렸으면 [`NewsError::InvalidRequest`].
    /// 오류에는 받은 검색어를 싣지 않는다.
    pub(crate) fn checked_query(&self, provider: NewsProviderId) -> Result<&str, NewsError> {
        let invalid = |detail: &str| NewsError::InvalidRequest {
            provider,
            detail: detail.to_owned(),
        };
        let query = self.query.trim();
        if query.is_empty() {
            return Err(invalid("검색어가 비었습니다"));
        }
        if query.chars().count() > MAX_QUERY_CHARS {
            return Err(invalid("검색어가 100자를 넘습니다"));
        }
        if query.chars().any(char::is_control) {
            return Err(invalid("검색어에 제어 문자가 있습니다"));
        }
        if !(1..=MAX_ITEMS).contains(&self.max_items) {
            return Err(invalid("기사 수는 1~100이어야 합니다"));
        }
        Ok(query)
    }
}

/// 검색어 글자 수 상한.
const MAX_QUERY_CHARS: usize = 100;

/// 한 번에 받을 기사 수 상한(네이버 `display` 최댓값).
const MAX_ITEMS: u16 = 100;

/// 받은 뉴스 기사 하나.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewsItem {
    /// 공급자.
    pub provider: NewsProviderId,
    /// 요청한 검색어.
    pub query: String,
    /// 제목([`clean_news_text`]를 거친 글).
    pub title: String,
    /// 요약([`clean_news_text`]를 거친 글).
    pub summary: String,
    /// 기사 링크(`http` · `https`).
    pub link: String,
    /// 공급자가 준 기사 시각(UTC).
    pub published_at: DateTime<Utc>,
    /// 알게 된 시각: 응답을 받은 시각(주입한 시계, P10).
    pub known_at: DateTime<Utc>,
}

impl NewsItem {
    /// 이 판단 시각에 이미 알 수 있던 기사인가: `known_at <= decision_at`
    /// ([`crate::Fact::known_by`]와 같은 뜻, V36 · P10).
    pub fn known_by(&self, decision_at: DateTime<Utc>) -> bool {
        self.known_at <= decision_at
    }
}

/// 뉴스 검색 결과.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NewsFetched {
    /// 받은 기사들. 비어 있지 않다.
    Items(Vec<NewsItem>),
    /// 결과가 없다. 오류가 아니다(E32).
    Missing,
}

/// 공급자가 준 글(제목 · 요약)을 맨 글로 만든다(순수 함수).
///
/// 1. 태그(`<...>`)를 먼저 모두 지운다.
/// 2. 그다음 엔티티를 푼다: `&quot;` `&amp;` `&lt;` `&gt;` `&#39;` `&apos;` `&nbsp;`, `&#숫자;` · `&#x16진;`.
///    모르는 엔티티는 그대로 둔다. 한 번만 푼다.
/// 3. 공백을 하나로 모으고 앞뒤를 자른다.
///
/// 순서 때문에 글로 쓴 `&lt;b&gt;`는 `<b>`로 남는다.
pub fn clean_news_text(text: &str) -> String {
    let untagged = strip_tags(text);
    let decoded = decode_entities(&untagged);
    decoded.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// `<`부터 다음 `>`까지를 지운다. 닫히지 않은 `<`는 글로 남긴다.
fn strip_tags(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find('<') {
        out.push_str(&rest[..start]);
        let after = &rest[start..];
        match after.find('>') {
            Some(end) => rest = &after[end + 1..],
            None => {
                out.push_str(after);
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

/// 엔티티 하나의 이름 길이 상한(`&#x10FFFF;`보다 넉넉하게).
const MAX_ENTITY_LEN: usize = 10;

/// 아는 엔티티를 한 번만 푼다. 모르는 것은 그대로 둔다.
fn decode_entities(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find('&') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let decoded = after
            .find(';')
            .filter(|end| *end <= MAX_ENTITY_LEN)
            .and_then(|end| entity(&after[..end]).map(|ch| (ch, end)));
        match decoded {
            Some((ch, end)) => {
                out.push(ch);
                rest = &after[end + 1..];
            }
            None => {
                out.push('&');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// 엔티티 이름(`&`와 `;` 사이)을 글자로.
fn entity(name: &str) -> Option<char> {
    match name {
        "quot" => Some('"'),
        "amp" => Some('&'),
        "lt" => Some('<'),
        "gt" => Some('>'),
        "#39" | "apos" => Some('\''),
        "nbsp" => Some(' '),
        _ => {
            let number = name.strip_prefix('#')?;
            let code = match number.strip_prefix(['x', 'X']) {
                Some(hex) if !hex.is_empty() && hex.chars().all(|c| c.is_ascii_hexdigit()) => {
                    u32::from_str_radix(hex, 16).ok()?
                }
                Some(_) => return None,
                None if !number.is_empty() && number.chars().all(|c| c.is_ascii_digit()) => {
                    number.parse::<u32>().ok()?
                }
                None => return None,
            };
            char::from_u32(code)
        }
    }
}

/// 뉴스 공급자. 새 공급자는 이것만 구현하고 [`NewsRegistry`]에 등록한다(결정 2번).
#[async_trait]
pub trait NewsProvider: Send + Sync {
    /// 공급자 이름.
    fn id(&self) -> NewsProviderId;

    /// 이 공급자가 쓰는 키 칸의 선언.
    fn keys(&self) -> NewsKeys;

    /// 검색한다. 결과가 비면 [`NewsFetched::Missing`].
    async fn search(&self, request: &NewsRequest) -> Result<NewsFetched, NewsError>;
}

/// 뉴스 공급자 등록부.
///
/// - 같은 이름으로 다시 등록하면 바꾼다(Hermes `ProviderProfile`의 last-writer-wins). 자리는 처음 자리다.
/// - 검색은 기본 공급자 → (기본을 정하지 않았으면) 처음 등록된 공급자에게 맡긴다.
/// - 공급자가 없거나 기본이 등록되지 않은 공급자면 요청 없이 [`NewsError::NoProvider`].
#[derive(Default)]
pub struct NewsRegistry {
    providers: Vec<Arc<dyn NewsProvider>>,
    default: Option<NewsProviderId>,
}

impl NewsRegistry {
    /// 빈 등록부.
    pub fn new() -> Self {
        Self::default()
    }

    /// 등록한다. 같은 이름이 있으면 바꾼다.
    pub fn register(&mut self, provider: Arc<dyn NewsProvider>) {
        let id = provider.id();
        match self.providers.iter_mut().find(|known| known.id() == id) {
            Some(slot) => *slot = provider,
            None => self.providers.push(provider),
        }
    }

    /// 기본 공급자를 정한다.
    pub fn set_default(&mut self, id: NewsProviderId) {
        self.default = Some(id);
    }

    /// 등록된 공급자 이름들(등록 순서).
    pub fn providers(&self) -> Vec<NewsProviderId> {
        self.providers
            .iter()
            .map(|provider| provider.id())
            .collect()
    }

    /// 고른 공급자로 검색한다.
    pub async fn search(&self, request: &NewsRequest) -> Result<NewsFetched, NewsError> {
        let chosen = match self.default {
            Some(id) => self.providers.iter().find(|provider| provider.id() == id),
            None => self.providers.first(),
        };
        let Some(provider) = chosen else {
            return Err(NewsError::NoProvider {
                provider: self.default.unwrap_or(NewsProviderId::Naver),
            });
        };
        provider.search(request).await
    }
}

/// 가짜 뉴스 공급자: 정해 준 결과(정하지 않았으면 [`NewsFetched::Missing`])와 오류를 돌려주고
/// 받은 요청을 기록한다. 공개 API다(판단 · 조립 쪽 테스트가 쓴다).
pub struct FakeNewsProvider {
    id: NewsProviderId,
    result: Mutex<Result<NewsFetched, NewsError>>,
    calls: Mutex<Vec<NewsRequest>>,
}

impl FakeNewsProvider {
    /// 그 이름의 가짜를 만든다. 키 칸 선언은 그 이름의 실제 공급자와 같다(네이버 → [`crate::NAVER_NEWS_KEYS`]).
    pub fn new(id: NewsProviderId) -> Self {
        Self {
            id,
            result: Mutex::new(Ok(NewsFetched::Missing)),
            calls: Mutex::new(Vec::new()),
        }
    }

    /// 다음 검색부터 돌려줄 결과를 정한다.
    pub fn set(&self, result: Result<NewsFetched, NewsError>) {
        *lock(&self.result) = result;
    }

    /// 받은 요청들(받은 순서).
    pub fn calls(&self) -> Vec<NewsRequest> {
        lock(&self.calls).clone()
    }
}

/// 잠금을 얻는다. 다른 곳이 잠근 채 멈췄어도 값은 그대로 쓴다(가짜는 값만 담는다).
fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[async_trait]
impl NewsProvider for FakeNewsProvider {
    fn id(&self) -> NewsProviderId {
        self.id
    }

    fn keys(&self) -> NewsKeys {
        match self.id {
            NewsProviderId::Naver => crate::NAVER_NEWS_KEYS,
        }
    }

    async fn search(&self, request: &NewsRequest) -> Result<NewsFetched, NewsError> {
        lock(&self.calls).push(request.clone());
        lock(&self.result).clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 닫히지_않은_꺾쇠와_깨진_엔티티는_글로_남는다() {
        assert_eq!(clean_news_text("a < b"), "a < b");
        assert_eq!(
            clean_news_text("&#; &#x; &#xZZ; &#99999999;"),
            "&#; &#x; &#xZZ; &#99999999;"
        );
        assert_eq!(clean_news_text("&amp"), "&amp");
        assert_eq!(clean_news_text("&#X41;"), "A");
    }

    #[test]
    fn 검색어는_앞뒤_공백을_빼고_확인한다() {
        let request = NewsRequest::latest("  금리 ", 1);
        assert_eq!(request.checked_query(NewsProviderId::Naver), Ok("금리"));
    }
}
