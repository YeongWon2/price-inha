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
//! - (10b 보강 1번) 주기 가져오기(DART · 네이버 · 재무 · 거시)는 그 시장의 정규장과 그 앞뒤 30분 안에서만 돈다.
//!   장 밖 · 휴장일에 나온 공시는 다음 장 시작의 공시 보기가 맡는다. DART 공시 간격도 네이버와 같은 방식으로
//!   `하루 × 종목 수 ÷ 상한`보다 짧지 않게 늘리고, 요청마다 하루 수를 센다([`super::quota::DartQuota`]).
//! - (10b 보강 3번) 장 시작 뒤 하루 한 번 재무 · 거시 값을 받아 지난번 값과 다르면 넘긴다([`super::changes`]).
//! - (10b-2, 결정 19) 쓰는 템플릿의 구독([`crate::Trading::wake_subscriptions`])을 고리마다 읽어 건다: 시세가 없으면
//!   시세 조건을 비우고, 공시가 없으면 DART 공시 · 재무 · 거시를 부르지 않고(넘기지도 않는다), 뉴스가 없으면 네이버를
//!   부르지 않는다. 쓰는 템플릿이 없거나 멈춤이면 아무것도 걸지 않는다. 구독은 깨우기만 정한다(손절 감시 · 논의의
//!   자료 받기는 그대로).

use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, NaiveDate, TimeZone, Utc};
use pi_data::{FactValue, Fetched, NewsFetched, NewsItem};
use pi_market::SessionKind;
use pi_market::Symbol;
use pi_template::TriggerKind;
use pi_trigger::{
    ConditionJob, DEFAULT_WATCH_CHANGE_PERCENT, DataChange, DataKind, DataPush, Target, Trigger,
    watch_conditions,
};
use tokio::sync::watch;

use super::changes::Changes;
use super::demo_feed::DemoDisclosures;
use super::news::Searched;
use super::seen::Seen;
use super::{Crew, facts};

/// 고리를 다시 보는 간격(시계의 쉬기로 잰다).
const LOOP: Duration = Duration::from_secs(1);
/// 정규장 앞뒤로 주기 가져오기를 더 도는 시간(분).
const WINDOW_MARGIN_MINUTES: i64 = 30;
/// 깨우기에 싣는 짧은 글의 상한(글자).
const TITLE_MAX: usize = 200;
/// 하루(초).
const DAY_SECONDS: u64 = 86_400;
/// 자료 깨우기 · 시세 조건의 대상: 뉴스 공시 시세 담당(F2 1).
pub(super) const SCOUT: &str = "scout";

/// 짧게 자른 글(가린 뒤).
pub(super) fn short(crew: &Crew, text: &str) -> String {
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

/// (10b-2) 쓰는 템플릿이 구독하는 깨우기. 기본값은 아무것도 걸지 않음.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Subscribed {
    quote: bool,
    disclosure: bool,
    news: bool,
}

impl Subscribed {
    /// 쓰는 템플릿의 구독. 없거나(멈춤 포함) 읽지 못하면 아무것도 걸지 않는다.
    fn of(crew: &Crew) -> Subscribed {
        let kinds = match crew.trading.wake_subscriptions() {
            Ok(Some(kinds)) => kinds,
            Ok(None) => return Subscribed::default(),
            Err(error) => {
                tracing::warn!(%error, "쓰는 템플릿의 구독을 읽지 못했다");
                return Subscribed::default();
            }
        };
        Subscribed {
            quote: kinds.contains(&TriggerKind::Quote),
            disclosure: kinds.contains(&TriggerKind::Disclosure),
            news: kinds.contains(&TriggerKind::News),
        }
    }
}

/// 장 시작의 공시 보기가 깨우기를 만드는가: 쓰는 템플릿이 공시를 구독할 때만(10b-2).
pub(super) fn disclosure_subscribed(crew: &Crew) -> bool {
    Subscribed::of(crew).disclosure
}

/// 시세 조건 · 자료 가져오기 고리.
pub(super) struct Poller {
    crew: Arc<Crew>,
    trigger: Arc<Trigger>,
    seen: Seen,
    /// 마지막으로 건 시세 조건 목록.
    conditions: Option<Vec<ConditionJob>>,
    last_poll: Option<DateTime<Utc>>,
    last_dart: Option<DateTime<Utc>>,
    last_news: Option<DateTime<Utc>>,
    /// (10b 보강 3번) 하루 한 번 재무 · 거시 값 바뀜.
    changes: Changes,
    /// (24차 3번) 시연용 가짜 공시(실행 파일의 시연 조립만 켠다).
    demo: Option<DemoDisclosures>,
}

impl Poller {
    pub(super) fn new(crew: Arc<Crew>, trigger: Arc<Trigger>) -> Poller {
        Poller {
            crew,
            trigger,
            seen: Seen::default(),
            conditions: None,
            last_poll: None,
            last_dart: None,
            last_news: None,
            changes: Changes::default(),
            demo: None,
        }
    }

    /// (24차 3번) `every`가 있으면 그 간격(시계 기준)마다 시연용 가짜 공시를 넣는다.
    pub(super) fn with_demo(mut self, every: Option<Duration>) -> Poller {
        self.demo = every.map(DemoDisclosures::new);
        self
    }

    /// `stop`이 참이 될 때까지 돈다.
    pub(super) async fn run(mut self, mut stop: watch::Receiver<bool>) {
        loop {
            if *stop.borrow() {
                return;
            }
            let subscribed = Subscribed::of(&self.crew);
            self.refresh_conditions(subscribed.quote).await;
            let now = self.crew.clock.now();
            if let Some(open) = self.session_open(now) {
                if self.due(self.last_poll, self.crew.wiring.data_poll, now) {
                    self.last_poll = Some(now);
                    self.poll(now, subscribed).await;
                }
                // 재무 · 거시 바뀜은 공시 구독에 묶는다(결정 19).
                if subscribed.disclosure && now >= open {
                    self.changes.daily(&self.crew, &self.trigger, now).await;
                }
                if subscribed.disclosure {
                    self.demo_disclosure(now);
                }
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

    /// 지금이 그 시장의 정규장과 그 앞뒤 30분 안이면 그날 정규장이 열리는 시각. 휴장일 · 달력을 모르면 없음.
    fn session_open(&self, now: DateTime<Utc>) -> Option<DateTime<Utc>> {
        let trading = &self.crew.trading;
        let calendar = trading.calendar_on(trading.date_of(now))?;
        let regular = calendar
            .today
            .sessions
            .iter()
            .filter(|session| session.kind == SessionKind::Regular);
        let open = regular.clone().map(|session| session.start).min()?;
        let close = regular.map(|session| session.end).max()?;
        let margin = chrono::Duration::minutes(WINDOW_MARGIN_MINUTES);
        (now >= open - margin && now <= close + margin).then_some(open)
    }

    /// 지켜보는 종목 · 기준가로 시세 조건을 만들고, 바뀌었으면 엔진의 목록을 바꾼다. 시세를 구독하지 않으면
    /// 빈 목록이다(10b-2).
    async fn refresh_conditions(&mut self, quote: bool) {
        let trading = &self.crew.trading;
        let now = self.crew.clock.now();
        let symbols = if quote {
            self.crew.desk.symbols()
        } else {
            Vec::new()
        };
        let watched: Vec<(Symbol, rust_decimal::Decimal)> = symbols
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

    /// 지켜보는 종목마다 새 공시 · 새 뉴스를 가져와 넘긴다(구독한 것만).
    async fn poll(&mut self, now: DateTime<Utc>, subscribed: Subscribed) {
        let symbols = self.crew.desk.symbols();
        if symbols.is_empty() {
            return;
        }
        let wiring = &self.crew.wiring;
        let dart = subscribed.disclosure
            && self.due(
                self.last_dart,
                self.spread(wiring.dart_daily_limit, symbols.len()),
                now,
            );
        let news = subscribed.news
            && self.due(
                self.last_news,
                self.spread(wiring.news_daily_limit, symbols.len()),
                now,
            )
            && self.crew.news.has_key();
        if dart {
            self.last_dart = Some(now);
        }
        if news {
            self.last_news = Some(now);
        }
        for symbol in &symbols {
            if dart {
                self.disclosures(symbol, now).await;
            }
            if news {
                self.news(symbol, now).await;
            }
        }
    }

    /// 하루 상한을 지키는 간격: 자료 간격과 `하루 × 종목 수 ÷ 상한` 가운데 긴 것.
    fn spread(&self, limit: u32, symbols: usize) -> Duration {
        let limit = u64::from(limit.max(1));
        let count = u64::try_from(symbols).unwrap_or(u64::MAX);
        let spread = Duration::from_secs(DAY_SECONDS.saturating_mul(count) / limit);
        self.crew.wiring.data_poll.max(spread)
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
            Err(error) if super::quota::over_limit(&error) => return,
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

    /// (24차 3번) 간격이 지났으면 시연용 가짜 공시 하나를 넘긴다. 본 것 기억(처음 보는 종목 · 출처는 그때 것을
    /// 이미 본 것으로만 적는다)은 거치지 않는다 — 번호가 늘 새것이고 첫 건을 삼키지 않게.
    fn demo_disclosure(&mut self, now: DateTime<Utc>) {
        let Some(change) = self
            .demo
            .as_mut()
            .and_then(|demo| demo.next(&self.crew, now))
        else {
            return;
        };
        let pushed = self.trigger.push_data(change);
        tracing::debug!(?pushed, "시연용 가짜 공시를 넣었다");
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
