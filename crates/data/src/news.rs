//! 뉴스 값 · 뉴스 공급자 trait · 등록부 (4차, 결정 2026-11-21 2 · 3번).
//!
//! 뉴스는 [`crate::FactKind`] 밖의 따로 된 값이다(결정 13번 · 지시서 "잠긴 옛 테스트와 부딪히는 곳").
//! 받는 칸은 제목 · 요약 · 링크 · 시각뿐이다. 본문 · 언론사 · 썸네일은 받지 않는다.

use std::fmt;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use pi_secrets::ProviderKeys;

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
/// `pi_secrets::ProviderKeys`(secrets 8차)와 같은 모양의 이 모듈 쪽 선언이다.
/// [`NewsKeys::provider_keys`]로 보관소 선언으로 바꾸고, 보관소에서 읽기는
/// `impl NewsKeySource for SecretStore`(`read_provider`)가 한다.
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

    /// 같은 공급자 이름 · 칸 이름의 보관소 선언(`pi_secrets::ProviderKeys`).
    /// 이름 규칙 검사는 보관소가 읽거나 등록할 때 한다.
    pub fn provider_keys(&self) -> ProviderKeys {
        ProviderKeys::new(self.provider, self.fields)
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
    /// 제목([`clean_news_text`]를 거친 글, 300자에서 자른다).
    pub title: String,
    /// 요약([`clean_news_text`]를 거친 글, 2,000자에서 자른다).
    pub summary: String,
    /// 기사 링크(`http` · `https`, 주소 해석기가 정규화한 글).
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
    /// 받은 기사들. 비어 있지 않다. 버린 기사가 없을 때다.
    Items(Vec<NewsItem>),
    /// 링크 값이 이상한 기사(제어 문자 · 공백 · `"<>\`` ` `` · 사용자 정보 · 비정상 꼴 · http(s) 아님 ·
    /// 호스트 없음)를 그것만 버리고 남은 기사들과 버린 수(`dropped >= 1`).
    /// 남은 것이 없어도 `Missing`이 아니라 `items`가 빈 이 갈래다(버린 사실을 숨기지 않는다).
    /// 받는 쪽은 빈 `items`를 "뉴스 없음"으로 다루되 버린 수를 판단 기록에 남긴다(결정 2026-11-21 16번, V35 · E32).
    Partial {
        /// 남은 기사들(비어 있을 수 있다).
        items: Vec<NewsItem>,
        /// 링크가 이상해 버린 기사 수.
        dropped: usize,
    },
    /// 결과가 없다. 오류가 아니다(E32).
    Missing,
}

/// 공급자가 준 글(제목 · 요약)을 맨 글로 만든다(순수 함수).
///
/// 1. 태그(`<...>`)를 먼저 모두 지운다.
/// 2. 그다음 엔티티를 푼다: `&quot;` `&amp;` `&lt;` `&gt;` `&#39;` `&apos;` `&nbsp;`, `&#숫자;` · `&#x16진;`.
///    모르는 엔티티는 그대로 둔다. 한 번만 푼다.
/// 3. 공백이 아닌 제어 문자와 양방향 덮어쓰기 문자(U+202A~U+202E, U+2066~U+2069)를 지운다
///    (엔티티로 들어온 것도).
/// 4. 공백을 하나로 모으고 앞뒤를 자른다.
///
/// 순서 때문에 글로 쓴 `&lt;b&gt;`는 `<b>`로 남는다.
pub fn clean_news_text(text: &str) -> String {
    let untagged = strip_tags(text);
    let decoded = decode_entities(&untagged);
    let visible: String = decoded.chars().filter(|ch| !is_hidden_char(*ch)).collect();
    visible.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// 기사 제목 글자 수 상한. 넘으면 자른다.
pub(crate) const MAX_TITLE_CHARS: usize = 300;

/// 기사 요약 글자 수 상한. 넘으면 자른다.
pub(crate) const MAX_SUMMARY_CHARS: usize = 2_000;

/// 글을 `max_chars`자까지만 남긴다(끝 공백은 뺀다).
pub(crate) fn truncate_chars(text: String, max_chars: usize) -> String {
    match text.char_indices().nth(max_chars) {
        Some((at, _)) => text[..at].trim_end().to_owned(),
        None => text,
    }
}

/// 지울 글자: 공백이 아닌 제어 문자, 양방향 덮어쓰기 문자.
fn is_hidden_char(ch: char) -> bool {
    (ch.is_control() && !ch.is_whitespace())
        || ('\u{202A}'..='\u{202E}').contains(&ch)
        || ('\u{2066}'..='\u{2069}').contains(&ch)
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

/// 그 공급자의 키 칸 선언.
pub(crate) fn keys_of(id: NewsProviderId) -> NewsKeys {
    match id {
        NewsProviderId::Naver => crate::NAVER_NEWS_KEYS,
    }
}

/// 그 키 칸 선언을 쓰는 공급자. 아는 공급자 선언이 아니면 `None`.
pub(crate) fn provider_of(keys: NewsKeys) -> Option<NewsProviderId> {
    [NewsProviderId::Naver]
        .into_iter()
        .find(|id| keys_of(*id) == keys)
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
        keys_of(self.id)
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
    fn 제어_문자와_양방향_덮어쓰기_문자는_지운다() {
        assert_eq!(clean_news_text("a\u{0}b\u{7f}c\u{9b}d"), "abcd");
        assert_eq!(
            clean_news_text("삼성\u{202E}전자\u{2066}x\u{2069}"),
            "삼성전자x"
        );
        assert_eq!(clean_news_text("a&#x202E;b&#0;c&#8238;d"), "abcd");
        assert_eq!(clean_news_text("a \u{0} b"), "a b");
        assert_eq!(clean_news_text("&lt;b&gt;글"), "<b>글");
    }

    #[test]
    fn 길이_상한을_넘으면_자른다() {
        assert_eq!(truncate_chars("가나다라".to_owned(), 2), "가나");
        assert_eq!(truncate_chars("가나".to_owned(), 2), "가나");
        assert_eq!(truncate_chars("ab cd".to_owned(), 3), "ab");
        let long = "가".repeat(MAX_TITLE_CHARS + 5);
        assert_eq!(
            truncate_chars(long, MAX_TITLE_CHARS).chars().count(),
            MAX_TITLE_CHARS
        );
    }

    #[test]
    fn 키_칸_선언은_보관소_선언으로_바뀐다() {
        let keys = crate::NAVER_NEWS_KEYS.provider_keys();
        assert_eq!(keys.provider(), crate::NAVER_NEWS_KEYS.provider());
        assert_eq!(keys.fields(), crate::NAVER_NEWS_KEYS.fields());
        assert!(keys.check().is_ok());
        assert_eq!(
            provider_of(crate::NAVER_NEWS_KEYS),
            Some(NewsProviderId::Naver)
        );
        assert_eq!(provider_of(NewsKeys::new("other", &["a"])), None);
    }

    #[test]
    fn 검색어는_앞뒤_공백을_빼고_확인한다() {
        let request = NewsRequest::latest("  금리 ", 1);
        assert_eq!(request.checked_query(NewsProviderId::Naver), Ok("금리"));
    }
}
