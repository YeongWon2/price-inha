//! (10차 10b 9번) 뉴스 잇기: 네이버 뉴스를 `pi_data::NewsRegistry`에 등록하고(키는 비밀 보관소의
//! `NAVER_NEWS_KEYS` 칸, `NewsKeySource`), 논의 자료 · 판단 기록에 뉴스(제목 · 요약 · 링크 · 시각)와 출처를 싣는다.
//!
//! - 링크가 이상한 기사만 버린 결과(`NewsFetched::Partial`)는 남은 기사를 쓰고 버린 수를 자료 이름(판단 기록의
//!   자료 목록)과 방 로그에 남긴다. 남은 것이 없거나 결과가 없으면 "뉴스 없음"이다(결정 2026-11-21 16번, E32).
//! - 키가 없으면 묻지 않고 자료에도 넣지 않는다(뉴스 키는 선택이다). 받지 못한 것(오류)은 내용 없는 자료다.
//! - 네이버 하루 요청 상한(`TeamWiring::news_daily_limit`)을 논의 · 주기 가져오기가 함께 센다([`NewsDesk::take`]).
//!   센 수는 저장소에 남겨 다시 켜도 그날 수를 잇는다. 키는 머리글로만 보내고 주소에 싣지 않는다(`pi_data`).

use std::sync::{Arc, Mutex, PoisonError};

use chrono::{DateTime, NaiveDate, Utc};
use pi_agent::DataItem;
use pi_data::{
    NAVER_NEWS_KEYS, NaverNews, NaverNewsConfig, NewsError, NewsFetched, NewsItem, NewsKeySource,
    NewsRegistry, NewsRequest,
};
use pi_secrets::SecretStore;
use pi_store::{FactId, NewFact, Store, UnixMillis};

/// 한 번에 받을 기사 수.
const NEWS_ITEMS: u16 = 20;
/// 그날 쓴 네이버 요청 수를 남기는 저장소 칸.
const USED_KEY: &str = "data_poll.naver_used";
/// 저장하는 뉴스 자료의 종류 이름(기존 자료 종류 밖의 따로 된 값, 결정 13번).
pub(super) const NEWS_KIND: &str = "news";
/// 결과가 없을 때의 글.
const NO_NEWS: &str = "뉴스 없음";

/// 뉴스 검색 한 번의 결과.
pub(super) enum Searched {
    /// 키가 없다(묻지 않았다).
    NoKey,
    /// 오늘 요청 상한에 닿았다(묻지 않았다).
    OverLimit,
    /// 받았다.
    Fetched(NewsFetched),
    /// 물었는데 받지 못했다.
    Failed(NewsError),
}

/// 논의에 싣는 뉴스 자료 하나와 붙일 자료 번호들.
pub(super) struct NewsGathered {
    pub(super) item: DataItem,
    pub(super) facts: Vec<(FactId, UnixMillis)>,
    /// 링크가 이상해 버린 기사 수.
    pub(super) dropped: usize,
    /// 물었는데 받지 못했나(자료 이상 신호).
    pub(super) failed: bool,
}

/// 뉴스 공급자 등록부와 하루 상한.
pub(crate) struct NewsDesk {
    registry: NewsRegistry,
    keys: Arc<SecretStore>,
    store: Store,
    base_url: String,
    limit: u32,
    /// (날짜, 그날 쓴 수). 처음 쓸 때 저장소에서 읽는다.
    used: Mutex<Option<(NaiveDate, u32)>>,
}

fn millis(at: DateTime<Utc>) -> UnixMillis {
    UnixMillis(at.timestamp_millis())
}

/// 한국 날짜(하루 상한은 한국 날짜로 센다).
pub(super) fn kst_date(at: DateTime<Utc>) -> NaiveDate {
    (at + chrono::Duration::hours(9)).date_naive()
}

impl NewsDesk {
    /// 네이버를 등록한다. 기본 주소가 규칙에 맞지 않으면(https · 루프백 아님) 공급자 없이 둔다(묻지 않는다).
    pub(crate) fn open(
        config: NaverNewsConfig,
        limit: u32,
        keys: Arc<SecretStore>,
        store: Store,
        clock: Arc<dyn pi_data::Clock>,
    ) -> NewsDesk {
        let mut registry = NewsRegistry::new();
        let base_url = config.base_url.trim_end_matches('/').to_string();
        let source: Arc<dyn NewsKeySource> = Arc::clone(&keys) as Arc<dyn NewsKeySource>;
        match NaverNews::new(config, source, clock) {
            Ok(naver) => registry.register(Arc::new(naver)),
            Err(error) => tracing::warn!(%error, "네이버 뉴스 공급자를 만들지 못했습니다"),
        }
        NewsDesk {
            registry,
            keys,
            store,
            base_url,
            limit,
            used: Mutex::new(None),
        }
    }

    /// 뉴스 키가 등록돼 있나.
    pub(super) fn has_key(&self) -> bool {
        matches!(self.keys.read_news_keys(NAVER_NEWS_KEYS), Ok(Some(_)))
    }

    /// 오늘(한국 날짜) 요청 하나를 쓴다. 상한에 닿았으면 거짓. 센 수는 저장소에 남긴다.
    pub(super) fn take(&self, now: DateTime<Utc>) -> bool {
        let today = kst_date(now);
        let mut used = self.used.lock().unwrap_or_else(PoisonError::into_inner);
        let count = match *used {
            Some((date, count)) if date == today => count,
            Some(_) => 0,
            None => self.saved_count(today),
        };
        if count >= self.limit {
            *used = Some((today, count));
            return false;
        }
        let count = count + 1;
        *used = Some((today, count));
        let text = format!("{} {count}", today.format("%Y-%m-%d"));
        if let Err(error) = self.store.set_knowledge_meta(USED_KEY, &text) {
            tracing::warn!(%error, "뉴스 요청 수를 남기지 못했습니다");
        }
        true
    }

    /// 저장소에 남긴 그날의 수(다른 날이거나 없으면 0).
    fn saved_count(&self, today: NaiveDate) -> u32 {
        let saved = self.store.knowledge_meta(USED_KEY).ok().flatten();
        let Some(saved) = saved else {
            return 0;
        };
        let mut parts = saved.split(' ');
        let date = parts
            .next()
            .and_then(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").ok());
        let count = parts.next().and_then(|c| c.parse::<u32>().ok());
        match (date, count) {
            (Some(date), Some(count)) if date == today => count,
            _ => 0,
        }
    }

    /// 그 종목의 뉴스를 찾는다(최신순). 키가 없거나 상한이면 묻지 않는다.
    pub(super) async fn search(&self, symbol: &str, now: DateTime<Utc>) -> Searched {
        if !self.has_key() {
            return Searched::NoKey;
        }
        if !self.take(now) {
            return Searched::OverLimit;
        }
        match self
            .registry
            .search(&NewsRequest::latest(symbol, NEWS_ITEMS))
            .await
        {
            Ok(fetched) => Searched::Fetched(fetched),
            Err(NewsError::NoKey { .. } | NewsError::NoProvider { .. }) => Searched::NoKey,
            Err(error) => Searched::Failed(error),
        }
    }

    /// 출처 주소(키 없음): 검색 경로와 검색어.
    pub(super) fn source_url(&self, symbol: &str) -> String {
        format!("{}/v1/search/news.json?query={symbol}", self.base_url)
    }

    /// 논의 자료와 저장할 뉴스 자료를 만든다. 키가 없으면 없음(자료에 넣지 않는다).
    pub(super) async fn gather(
        &self,
        symbol: &str,
        now: DateTime<Utc>,
    ) -> Result<Option<NewsGathered>, pi_store::StoreError> {
        let url = self.source_url(symbol);
        let name = format!("{NEWS_KIND} · naver · {url}");
        let (items, dropped) = match self.search(symbol, now).await {
            Searched::NoKey => return Ok(None),
            Searched::OverLimit => {
                let item = DataItem {
                    name,
                    content: Some(format!(
                        "{NO_NEWS}(오늘 뉴스 검색 한도에 닿아 묻지 않았습니다)"
                    )),
                    known_at: millis(now),
                };
                return Ok(Some(empty(item, false)));
            }
            Searched::Failed(error) => {
                tracing::warn!(%error, "뉴스를 받지 못했습니다");
                let item = DataItem {
                    name,
                    content: None,
                    known_at: millis(now),
                };
                return Ok(Some(empty(item, true)));
            }
            Searched::Fetched(NewsFetched::Items(items)) => (items, 0),
            Searched::Fetched(NewsFetched::Partial { items, dropped }) => (items, dropped),
            Searched::Fetched(_) => (Vec::new(), 0),
        };
        let saved = self.store.save_facts(
            &items
                .iter()
                .map(|item| news_fact(symbol, &url, item))
                .collect::<Vec<_>>(),
        )?;
        let known = items.iter().map(|item| item.known_at).max().unwrap_or(now);
        let mut content = if items.is_empty() {
            NO_NEWS.to_string()
        } else {
            items.iter().map(describe).collect::<Vec<_>>().join("; ")
        };
        let mut name = name;
        if dropped > 0 {
            let note = format!("링크가 이상해 버린 기사 {dropped}건");
            content.push_str(&format!(" ({note})"));
            name.push_str(&format!(" · {note}"));
        }
        Ok(Some(NewsGathered {
            item: DataItem {
                name,
                content: Some(content),
                known_at: millis(known),
            },
            facts: saved.iter().map(|fact| (fact.id, fact.known_at)).collect(),
            dropped,
            failed: false,
        }))
    }
}

fn empty(item: DataItem, failed: bool) -> NewsGathered {
    NewsGathered {
        item,
        facts: Vec::new(),
        dropped: 0,
        failed,
    }
}

/// 기사 하나를 논의에 보일 글로(제목 · 요약 · 링크 · 시각).
fn describe(item: &NewsItem) -> String {
    format!(
        "제목: {} · 요약: {} · 링크: {} · 시각: {}",
        item.title,
        item.summary,
        item.link,
        item.published_at.to_rfc3339()
    )
}

/// 기사 하나를 저장할 모양으로(값은 요약, 참조는 링크, 공표일은 기사 시각의 한국 날짜).
fn news_fact(symbol: &str, url: &str, item: &NewsItem) -> NewFact {
    NewFact {
        kind: NEWS_KIND.to_string(),
        subject: symbol.to_string(),
        label: item.title.clone(),
        period: Some(item.published_at.to_rfc3339()),
        value: pi_store::FactValue::Text(item.summary.clone()),
        unit: None,
        reference: Some(item.link.clone()),
        source_name: item.provider.as_str().to_string(),
        source_url: url.to_string(),
        published_on: Some(kst_date(item.published_at).format("%Y-%m-%d").to_string()),
        known_at: millis(item.known_at),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixed(DateTime<Utc>);

    impl pi_data::Clock for Fixed {
        fn now(&self) -> DateTime<Utc> {
            self.0
        }
    }

    fn at(day: u32, hour: u32) -> DateTime<Utc> {
        use chrono::TimeZone;
        Utc.with_ymd_and_hms(2026, 11, day, hour, 0, 0)
            .single()
            .unwrap()
    }

    fn desk(dir: &std::path::Path, limit: u32) -> NewsDesk {
        let secrets = Arc::new(SecretStore::open(dir).unwrap());
        let store = Store::open(dir.join("news.db")).unwrap();
        let config = NaverNewsConfig::new("http://127.0.0.1:9");
        NewsDesk::open(config, limit, secrets, store, Arc::new(Fixed(at(16, 1))))
    }

    #[test]
    fn 하루_상한을_넘지_않고_날이_바뀌면_다시_센다() {
        let dir = tempfile::tempdir().unwrap();
        let news = desk(dir.path(), 2);
        assert!(news.take(at(16, 1)));
        assert!(news.take(at(16, 2)));
        assert!(!news.take(at(16, 3)), "그날 상한");
        // 한국 날짜로 센다: UTC 16일 15시는 한국 17일 0시다.
        assert!(news.take(at(16, 15)), "다음 날은 다시 센다");
    }

    #[test]
    fn 센_수는_다시_켜도_그날_이어진다() {
        let dir = tempfile::tempdir().unwrap();
        let first = desk(dir.path(), 2);
        assert!(first.take(at(16, 1)));
        drop(first);
        let again = desk(dir.path(), 2);
        assert!(again.take(at(16, 2)));
        assert!(!again.take(at(16, 3)), "다시 켜도 그날 수를 잇는다");
    }

    #[test]
    fn 키가_없으면_묻지_않는다() {
        let dir = tempfile::tempdir().unwrap();
        let news = desk(dir.path(), 5);
        assert!(!news.has_key());
        let searched = futures_lite(news.search("005930", at(16, 1)));
        assert!(matches!(searched, Searched::NoKey));
        assert!(news.take(at(16, 1)), "묻지 않은 것은 세지 않는다");
    }

    /// 짧은 future 하나를 끝까지 돌린다(바깥에 나가지 않는 길만).
    fn futures_lite<F: std::future::Future>(future: F) -> F::Output {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(future)
    }
}
