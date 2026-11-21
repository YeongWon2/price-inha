//! (10차 10b 8번) 깨우기 엔진에 시세 조건과 자료 변화를 넣는다(trigger 6차).
//!
//! - 시세 조건: 지켜보는 종목마다 `pi_trigger::watch_conditions`(기준가 = 지난 정규장 마지막 시세, 기본 3%)를
//!   만들어, 목록이 바뀌면(종목 · 기준가) `Trigger::replace_conditions`로 바꾼다. 기준가를 아직 모르는 종목은
//!   빠진다(다음 장이 지나 기준가가 생기면 걸린다).
//! - 자료 변화: [`TeamWiring::data_poll`](시계 기준)마다 지켜보는 종목의 DART 공시와 네이버 뉴스를 가져와, 처음
//!   보는 것만 `Trigger::push_data`로 넘긴다. 본 것은 저장소에 기억해(종목 · 출처마다 끝이 있는 수) 다시 켠
//!   뒤에도 같은 것으로 두 번 깨우지 않는다. 처음 보는 종목 · 출처는 그때 있던 것을 "이미 본 것"으로만 적는다
//!   (설치한 날의 옛 공시 · 뉴스로 깨우지 않는다. 아직 논의하지 않은 공시는 장 시작의 공시 보기가 맡는다).
//! - 네이버 하루 상한: 뉴스 간격을 `하루 ÷ (상한 ÷ 종목 수)`보다 짧지 않게 늘리고, 요청마다 남은 수를 센다
//!   ([`super::news::NewsDesk::take`], 논의와 함께 센다). 키는 머리글로만 보낸다(주소에 싣지 않는다).
//! - 가져오기 실패는 기록만 한다(자료 이상 신호는 논의의 자료 받기만 센다 — 앞 차수 동작 그대로).

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, NaiveDate, TimeZone, Utc};
use pi_data::{FactValue, Fetched, NewsFetched, NewsItem};
use pi_market::Symbol;
use pi_store::Store;
use pi_trigger::{
    ConditionJob, DEFAULT_WATCH_CHANGE_PERCENT, DataChange, DataKind, DataPush, Target, Trigger,
    watch_conditions,
};
use tokio::sync::watch;

use super::news::Searched;
use super::{Crew, facts};

/// 고리를 다시 보는 간격(시계의 쉬기로 잰다).
const LOOP: Duration = Duration::from_secs(1);
/// 종목 · 출처마다 기억하는 본 것의 수.
const SEEN_MAX: usize = 512;
/// 본 것을 남기는 저장소 칸의 머리.
const SEEN_KEY: &str = "data_poll.seen";
/// 깨우기에 싣는 짧은 글의 상한(글자).
const TITLE_MAX: usize = 200;
/// 하루(초).
const DAY_SECONDS: u64 = 86_400;
/// 자료 깨우기 · 시세 조건의 대상: 뉴스 공시 시세 담당(F2 1).
pub(super) const SCOUT: &str = "scout";

/// 종목 · 출처마다 본 것(처음 본 순서). 저장소에서 처음 읽은 뒤 바뀔 때마다 남긴다.
#[derive(Default)]
struct Seen {
    lists: HashMap<String, VecDeque<String>>,
}

impl Seen {
    fn key(source: &str, symbol: &str) -> String {
        format!("{SEEN_KEY}.{source}.{symbol}")
    }

    /// 그 칸을 (처음이면 저장소에서 읽어) 준다. 저장소에 없으면 `None`(아직 한 번도 보지 않았다).
    fn load(&mut self, store: &Store, key: &str) -> Option<&mut VecDeque<String>> {
        if !self.lists.contains_key(key) {
            let saved = store.knowledge_meta(key).ok().flatten()?;
            let ids: Vec<String> = serde_json::from_str(&saved).unwrap_or_default();
            self.lists
                .insert(key.to_string(), ids.into_iter().collect());
        }
        self.lists.get_mut(key)
    }

    /// 받은 것들 가운데 처음 보는 것을 돌려주고 본 것으로 적는다. 처음 보는 칸이면 적기만 하고 빈 목록이다.
    /// `fresh`는 (번호, 값)이다.
    fn take_new<T>(
        &mut self,
        store: &Store,
        key: &str,
        fresh: Vec<(String, T)>,
    ) -> Vec<(String, T)> {
        let first = self.load(store, key).is_none();
        let list = self.lists.entry(key.to_string()).or_default();
        let mut new = Vec::new();
        let mut changed = first;
        for (id, value) in fresh {
            if list.contains(&id) {
                continue;
            }
            list.push_back(id.clone());
            changed = true;
            if !first {
                new.push((id, value));
            }
        }
        while list.len() > SEEN_MAX {
            list.pop_front();
        }
        if changed {
            let ids: Vec<&String> = list.iter().collect();
            let text = serde_json::to_string(&ids).unwrap_or_else(|_| "[]".to_string());
            if let Err(error) = store.set_knowledge_meta(key, &text) {
                tracing::warn!(%error, "본 자료를 저장소에 남기지 못했다");
            }
        }
        new
    }

    /// 깨우기 엔진이 받지 않은 것(속도 제한)은 본 것에서 다시 뺀다(다음에 다시 넘긴다).
    fn forget(&mut self, store: &Store, key: &str, id: &str) {
        if let Some(list) = self.lists.get_mut(key) {
            list.retain(|seen| seen != id);
            let ids: Vec<&String> = list.iter().collect();
            let text = serde_json::to_string(&ids).unwrap_or_else(|_| "[]".to_string());
            if let Err(error) = store.set_knowledge_meta(key, &text) {
                tracing::warn!(%error, "본 자료를 저장소에 남기지 못했다");
            }
        }
    }
}

/// 짧게 자른 글(가린 뒤).
fn short(crew: &Crew, text: &str) -> String {
    let hidden = pi_ai::Redactor::redact(crew.redactor.as_ref(), text);
    hidden.chars().take(TITLE_MAX).collect()
}

/// 날짜의 한국 자정.
fn midnight(date: NaiveDate) -> Option<DateTime<Utc>> {
    let kst = chrono::FixedOffset::east_opt(9 * 3600)?;
    kst.from_local_datetime(&date.and_hms_opt(0, 0, 0)?)
        .single()
        .map(|at| at.with_timezone(&Utc))
}

/// 시세 조건 · 자료 가져오기 고리.
pub(super) struct Poller {
    crew: Arc<Crew>,
    trigger: Arc<Trigger>,
    seen: Seen,
    /// 마지막으로 건 시세 조건 목록.
    conditions: Option<Vec<ConditionJob>>,
    last_poll: Option<DateTime<Utc>>,
    last_news: Option<DateTime<Utc>>,
}

impl Poller {
    pub(super) fn new(crew: Arc<Crew>, trigger: Arc<Trigger>) -> Poller {
        Poller {
            crew,
            trigger,
            seen: Seen::default(),
            conditions: None,
            last_poll: None,
            last_news: None,
        }
    }

    /// `stop`이 참이 될 때까지 돈다.
    pub(super) async fn run(mut self, mut stop: watch::Receiver<bool>) {
        loop {
            if *stop.borrow() {
                return;
            }
            self.refresh_conditions().await;
            let now = self.crew.clock.now();
            if self.due(self.last_poll, self.crew.wiring.data_poll, now) {
                self.last_poll = Some(now);
                self.poll(now).await;
            }
            tokio::select! {
                _ = stop.wait_for(|stop| *stop) => return,
                () = self.crew.clock.sleep(LOOP) => {}
            }
        }
    }

    /// 지난번에서 `every`가 지났나(처음이면 참). 시계가 뒤로 가면 지금부터 다시 센다.
    fn due(&self, last: Option<DateTime<Utc>>, every: Duration, now: DateTime<Utc>) -> bool {
        match last {
            None => true,
            Some(last) if now < last => true,
            Some(last) => now
                .signed_duration_since(last)
                .to_std()
                .is_ok_and(|gone| gone >= every),
        }
    }

    /// 지켜보는 종목 · 기준가로 시세 조건을 만들고, 바뀌었으면 엔진의 목록을 바꾼다.
    async fn refresh_conditions(&mut self) {
        let trading = &self.crew.trading;
        let now = self.crew.clock.now();
        let watched: Vec<(Symbol, rust_decimal::Decimal)> = self
            .crew
            .desk
            .symbols()
            .into_iter()
            .filter_map(|symbol| {
                let reference = trading.reference_price(symbol.as_str(), now)?;
                Some((symbol, reference))
            })
            .collect();
        let target = Target::Member(SCOUT.to_string());
        let wanted = watch_conditions(
            trading.market(),
            &watched,
            DEFAULT_WATCH_CHANGE_PERCENT,
            target,
        );
        if self.conditions.as_ref() == Some(&wanted) {
            return;
        }
        match self.trigger.replace_conditions(wanted.clone()).await {
            Ok(()) => self.conditions = Some(wanted),
            Err(error) => tracing::warn!(%error, "시세 조건을 바꾸지 못했다"),
        }
    }

    /// 지켜보는 종목마다 새 공시 · 새 뉴스를 가져와 넘긴다.
    async fn poll(&mut self, now: DateTime<Utc>) {
        let symbols = self.crew.desk.symbols();
        let news =
            !symbols.is_empty() && self.news_due(symbols.len(), now) && self.crew.news.has_key();
        if news {
            self.last_news = Some(now);
        }
        for symbol in &symbols {
            self.disclosures(symbol, now).await;
            if news {
                self.news(symbol, now).await;
            }
        }
    }

    /// 뉴스를 가져올 때가 됐나: 자료 간격과 하루 상한을 종목 수로 나눈 간격 가운데 긴 것.
    fn news_due(&self, symbols: usize, now: DateTime<Utc>) -> bool {
        let limit = u64::from(self.crew.wiring.news_daily_limit.max(1));
        let count = u64::try_from(symbols).unwrap_or(u64::MAX);
        let spread = Duration::from_secs(DAY_SECONDS.saturating_mul(count) / limit);
        let every = self.crew.wiring.data_poll.max(spread);
        self.due(self.last_news, every, now)
    }

    /// 그 종목의 공시(오늘을 포함한 기간)를 가져와 처음 보는 것을 넘긴다.
    async fn disclosures(&mut self, symbol: &Symbol, now: DateTime<Utc>) {
        let today = self.crew.trading.date_of(now);
        let Some(request) = facts::disclosures(&self.crew.wiring, symbol.as_str(), today) else {
            return;
        };
        let found = match self.crew.data.fetch(&request).await {
            Ok(Fetched::Facts(found)) => found,
            Ok(Fetched::Missing(_)) => Vec::new(),
            Err(pi_data::DataError::NoKey { .. }) => return,
            Err(error) => {
                tracing::warn!(%error, "공시를 가져오지 못했다");
                return;
            }
        };
        let fresh: Vec<(String, DataChange)> = found
            .iter()
            .filter_map(|fact| {
                let id = fact.reference.clone()?;
                let title = match &fact.value {
                    FactValue::Text(text) => text.clone(),
                    FactValue::Number(_) => fact.label.clone(),
                };
                let published = fact
                    .published_on
                    .and_then(midnight)
                    .unwrap_or(fact.known_at);
                let change = self.change(DataKind::Disclosure, "dart", symbol, &title);
                Some((
                    id.clone(),
                    DataChange {
                        id,
                        published_at: published,
                        learned_at: fact.known_at,
                        ..change
                    },
                ))
            })
            .collect();
        self.push(&Seen::key("dart", symbol.as_str()), fresh);
    }

    /// 그 종목의 뉴스를 가져와 처음 보는 것을 넘긴다(키가 없거나 상한이면 묻지 않는다).
    async fn news(&mut self, symbol: &Symbol, now: DateTime<Utc>) {
        let items: Vec<NewsItem> = match self.crew.news.search(symbol.as_str(), now).await {
            Searched::Fetched(NewsFetched::Items(items)) => items,
            Searched::Fetched(NewsFetched::Partial { items, .. }) => items,
            Searched::Fetched(_) => Vec::new(),
            Searched::NoKey | Searched::OverLimit => return,
            Searched::Failed(error) => {
                tracing::warn!(%error, "뉴스를 가져오지 못했다");
                return;
            }
        };
        let fresh: Vec<(String, DataChange)> = items
            .iter()
            .map(|item| {
                let change = self.change(DataKind::News, "naver", symbol, &item.title);
                let id = item.link.clone();
                let change = DataChange {
                    id: id.clone(),
                    published_at: item.published_at,
                    learned_at: item.known_at,
                    ..change
                };
                (id, change)
            })
            .collect();
        self.push(&Seen::key("naver", symbol.as_str()), fresh);
    }

    /// 자료 사건의 뼈대(번호 · 시각은 부르는 쪽이 채운다).
    fn change(&self, kind: DataKind, source: &str, symbol: &Symbol, title: &str) -> DataChange {
        let now = self.crew.clock.now();
        DataChange {
            kind,
            source: source.to_string(),
            id: String::new(),
            revision: None,
            market: self.crew.trading.market(),
            symbol: Some(symbol.clone()),
            title: short(&self.crew, title),
            published_at: now,
            learned_at: now,
        }
    }

    /// 처음 보는 것만 깨우기 엔진에 넘긴다. 속도 제한으로 받지 않은 것은 다음에 다시 넘긴다.
    fn push(&mut self, key: &str, fresh: Vec<(String, DataChange)>) {
        let store = self.crew.store.clone();
        for (id, change) in self.seen.take_new(&store, key, fresh) {
            if self.trigger.push_data(change) == DataPush::RateLimited {
                self.seen.forget(&store, key, &id);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> (tempfile::TempDir, Store) {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().join("poll.db")).unwrap();
        (dir, store)
    }

    fn ids(list: &[(String, ())]) -> Vec<&str> {
        list.iter().map(|(id, ())| id.as_str()).collect()
    }

    fn fresh(names: &[&str]) -> Vec<(String, ())> {
        names.iter().map(|name| ((*name).to_string(), ())).collect()
    }

    #[test]
    fn 처음_보는_칸은_적기만_하고_다음부터_새것만_준다() {
        let (_dir, store) = store();
        let mut seen = Seen::default();
        let key = Seen::key("dart", "005930");
        assert!(seen.take_new(&store, &key, fresh(&["a", "b"])).is_empty());
        let new = seen.take_new(&store, &key, fresh(&["a", "b", "c"]));
        assert_eq!(ids(&new), ["c"]);
        assert!(seen.take_new(&store, &key, fresh(&["c"])).is_empty());
    }

    #[test]
    fn 본_것은_다시_켜도_남는다() {
        let (_dir, store) = store();
        let key = Seen::key("naver", "000660");
        let mut first = Seen::default();
        assert!(
            first
                .take_new(&store, &key, Vec::<(String, ())>::new())
                .is_empty()
        );
        assert_eq!(ids(&first.take_new(&store, &key, fresh(&["x"]))), ["x"]);
        let mut again = Seen::default();
        assert!(again.take_new(&store, &key, fresh(&["x"])).is_empty());
        assert_eq!(ids(&again.take_new(&store, &key, fresh(&["y"]))), ["y"]);
    }

    #[test]
    fn 받지_않은_것은_잊어_다음에_다시_준다() {
        let (_dir, store) = store();
        let key = Seen::key("dart", "005930");
        let mut seen = Seen::default();
        let _ = seen.take_new(&store, &key, Vec::<(String, ())>::new());
        assert_eq!(ids(&seen.take_new(&store, &key, fresh(&["r"]))), ["r"]);
        seen.forget(&store, &key, "r");
        assert_eq!(ids(&seen.take_new(&store, &key, fresh(&["r"]))), ["r"]);
    }

    #[test]
    fn 기억하는_수에는_끝이_있다() {
        let (_dir, store) = store();
        let key = Seen::key("naver", "005930");
        let mut seen = Seen::default();
        let _ = seen.take_new(&store, &key, Vec::<(String, ())>::new());
        let many: Vec<String> = (0..SEEN_MAX + 10).map(|n| n.to_string()).collect();
        let names: Vec<&str> = many.iter().map(String::as_str).collect();
        let _ = seen.take_new(&store, &key, fresh(&names));
        assert_eq!(seen.lists.get(&key).map(VecDeque::len), Some(SEEN_MAX));
    }
}
