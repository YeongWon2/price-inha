//! 자료 변화 사건의 중복 표 · 출처마다 속도 제한 · 묶음 · 장 밖 보관(6차).
//!
//! 원본(Hermes webhook): 중복 키 표는 메모리에만 두고 1시간, 경로마다 60초에 30번, 조용한 시간 30초 ·
//! 첫 사건에서 300초면 보낸다. 원본은 묶음의 마지막 사건 하나만 보내지만 우리는 모두(상한) 싣는다
//! (결정 2026-11-21 13번). 무엇도 끝없이 쌓이지 않게 표마다 상한과 지우기를 둔다.

use std::collections::{HashMap, VecDeque};
use std::time::Duration;

use chrono::{DateTime, Utc};
use pi_market::{Market, MarketCalendar, SessionKind, Symbol};

use crate::{DataChange, DataPush, DataTriggerConfig, TriggerEvent};

/// 중복 표의 크기. 넘으면 가장 오래된 것부터 지운다.
pub(crate) const DEDUPE_CAPACITY: usize = 4096;
/// 동시에 열어 두는 묶음(`(시장, 종목)`)의 수 상한. 넘으면 가장 오래된 묶음을 버린다.
pub(crate) const MAX_BATCHES: usize = 1024;
/// 속도 제한 창을 두는 출처 수 상한. 넘으면 가장 오래 조용한 출처의 창을 지운다.
pub(crate) const MAX_SOURCES: usize = 256;
/// 사건 글의 길이 상한(글자).
pub(crate) const TITLE_LIMIT: usize = 200;

type DedupeKey = (String, String, Option<String>);
type BatchKey = (Market, Option<Symbol>);

/// 받은 사건 하나와 받은 순서.
#[derive(Debug, Clone)]
struct Pending {
    change: DataChange,
    /// 엔진 시계로 받은 시각(장 밖 보관 판단).
    pushed_at: DateTime<Utc>,
    seq: u64,
}

impl Pending {
    fn order(&self) -> (DateTime<Utc>, u64) {
        (self.change.published_at, self.seq)
    }
}

/// 묶음 하나(같은 `(시장, 종목)`).
#[derive(Debug)]
struct Batch {
    key: BatchKey,
    first_at: DateTime<Utc>,
    last_at: DateTime<Utc>,
    events: Vec<Pending>,
}

/// 그 시장이 지금 정규장인가.
enum Status {
    /// 정규장 밖.
    Closed,
    /// 정규장 안(시작 시각). 달력이 없으면 시작 시각을 모른다.
    Open(Option<DateTime<Utc>>),
}

/// 자료 사건의 메모리 상태. 저장소에 쓰지 않는다.
#[derive(Debug, Default)]
pub(crate) struct DataState {
    seen: HashMap<DedupeKey, DateTime<Utc>>,
    seen_order: VecDeque<(DedupeKey, DateTime<Utc>)>,
    sources: HashMap<String, VecDeque<DateTime<Utc>>>,
    batches: Vec<Batch>,
    held: Vec<(Market, Vec<Pending>)>,
    dropped: usize,
    seq: u64,
}

impl DataState {
    /// 사건 하나를 받는다. 순서: 종류 → 중복 → 속도 제한 → 묶음.
    pub(crate) fn push(
        &mut self,
        mut change: DataChange,
        now: DateTime<Utc>,
        config: &DataTriggerConfig,
    ) -> DataPush {
        if !config.enabled.contains(&change.kind) {
            return DataPush::Disabled;
        }
        self.prune_seen(now, config.dedupe_for);
        let key = (
            change.source.clone(),
            change.id.clone(),
            change.revision.clone(),
        );
        if let Some(seen) = self.seen.get(&key)
            && after(*seen, config.dedupe_for).is_none_or(|end| now < end)
        {
            return DataPush::Duplicate;
        }
        if !self.admit(&change.source, now, config) {
            return DataPush::RateLimited;
        }
        self.seen.insert(key.clone(), now);
        self.seen_order.push_back((key, now));
        self.prune_seen(now, config.dedupe_for);
        truncate_title(&mut change.title);
        self.enqueue(change, now, config.batch_limit);
        DataPush::Queued
    }

    /// 지난 중복 기간이 끝난 것과 상한을 넘은 것을 앞에서부터 지운다.
    fn prune_seen(&mut self, now: DateTime<Utc>, ttl: Duration) {
        while let Some((key, at)) = self.seen_order.front() {
            let current = self.seen.get(key) == Some(at);
            let expired = after(*at, ttl).is_some_and(|end| end <= now);
            let over = self.seen_order.len() > DEDUPE_CAPACITY;
            if current && !expired && !over {
                break;
            }
            if let Some((key, at)) = self.seen_order.pop_front()
                && self.seen.get(&key) == Some(&at)
            {
                self.seen.remove(&key);
            }
        }
    }

    /// 출처의 창에 자리가 있으면 받는다고 적고 참.
    fn admit(&mut self, source: &str, now: DateTime<Utc>, config: &DataTriggerConfig) -> bool {
        let window = config.per_source_window;
        for times in self.sources.values_mut() {
            times.retain(|at| after(*at, window).is_none_or(|end| now < end));
        }
        self.sources.retain(|_, times| !times.is_empty());
        if !self.sources.contains_key(source) && self.sources.len() >= MAX_SOURCES {
            let quietest = self
                .sources
                .iter()
                .min_by_key(|(_, times)| times.back().copied())
                .map(|(name, _)| name.clone());
            if let Some(name) = quietest {
                self.sources.remove(&name);
            }
        }
        let limit = usize::try_from(config.per_source_limit).unwrap_or(usize::MAX);
        let times = self.sources.entry(source.to_string()).or_default();
        if times.len() >= limit {
            return false;
        }
        times.push_back(now);
        true
    }

    /// 묶음 `(시장, 종목)`에 넣는다. 묶음이 상한을 넘으면 오래된 것부터 버린다.
    fn enqueue(&mut self, change: DataChange, now: DateTime<Utc>, limit: usize) {
        let key = (change.market, change.symbol.clone());
        self.seq = self.seq.wrapping_add(1);
        let pending = Pending {
            change,
            pushed_at: now,
            seq: self.seq,
        };
        let index = match self.batches.iter().position(|batch| batch.key == key) {
            Some(index) => index,
            None => {
                if self.batches.len() >= MAX_BATCHES {
                    self.drop_oldest_batch();
                }
                self.batches.push(Batch {
                    key,
                    first_at: now,
                    last_at: now,
                    events: Vec::new(),
                });
                self.batches.len() - 1
            }
        };
        let batch = &mut self.batches[index];
        batch.last_at = now;
        let market = batch.key.0;
        insert_sorted(&mut batch.events, pending);
        let over = cap(&mut batch.events, limit);
        warn_dropped(over, &[market], "한 묶음의 사건이 한 깨우기 상한을 넘었다");
        self.dropped += over;
    }

    fn drop_oldest_batch(&mut self) {
        let oldest = self
            .batches
            .iter()
            .enumerate()
            .min_by_key(|(_, batch)| batch.first_at)
            .map(|(index, _)| index);
        if let Some(index) = oldest {
            let batch = self.batches.remove(index);
            let reason = "열어 둔 묶음 수가 상한을 넘어 가장 오래된 묶음을 버린다";
            warn_dropped(batch.events.len(), &[batch.key.0], reason);
            self.dropped += batch.events.len();
        }
    }

    /// 자료 사건이 걸린 시장들(달력을 읽을 곳).
    pub(crate) fn markets(&self) -> Vec<Market> {
        let mut markets: Vec<Market> = Vec::new();
        let all = self
            .batches
            .iter()
            .map(|batch| batch.key.0)
            .chain(self.held.iter().map(|(market, _)| *market));
        for market in all {
            if !markets.contains(&market) {
                markets.push(market);
            }
        }
        markets
    }

    /// 전체 정지: 모인 사건을 모두 버리고 센다.
    pub(crate) fn drop_all(&mut self) {
        let markets = self.markets();
        let batched: usize = self.batches.drain(..).map(|batch| batch.events.len()).sum();
        let held: usize = self.held.drain(..).map(|(_, events)| events.len()).sum();
        let dropped = batched + held;
        let reason = "전체 정지 중이라 모인 사건을 깨우지 않고 버린다";
        warn_dropped(dropped, &markets, reason);
        self.dropped += dropped;
    }

    /// 지난 보고 뒤로 버린 수를 가져간다.
    pub(crate) fn take_dropped(&mut self) -> usize {
        std::mem::take(&mut self.dropped)
    }

    /// 이번 tick에 보낼 사건들(이른 순). 장 밖 사건은 보관으로 옮기고, 장이 열린 시장의 보관은 `held: true`로
    /// 모두 낸다. 대상이 실행 중(`busy`)이면 아무것도 내지 않고 남긴다.
    pub(crate) fn collect(
        &mut self,
        now: DateTime<Utc>,
        calendars: &HashMap<Market, MarketCalendar>,
        config: &DataTriggerConfig,
        busy: bool,
    ) -> Vec<TriggerEvent> {
        self.hold_off_hours(now, calendars, config.batch_limit);
        if busy {
            return Vec::new();
        }
        let mut out: Vec<(Pending, bool)> = Vec::new();
        let mut kept = Vec::new();
        for (market, events) in self.held.drain(..) {
            match status(calendars.get(&market), now) {
                Status::Open(_) => out.extend(events.into_iter().map(|p| (p, true))),
                Status::Closed => kept.push((market, events)),
            }
        }
        self.held = kept;
        let mut waiting = Vec::new();
        for batch in self.batches.drain(..) {
            if is_due(&batch, now, config) {
                out.extend(batch.events.into_iter().map(|p| (p, false)));
            } else {
                waiting.push(batch);
            }
        }
        self.batches = waiting;
        out.sort_by_key(|(pending, _)| pending.order());
        let over = out.len().saturating_sub(config.batch_limit);
        let mut markets: Vec<Market> = Vec::new();
        for (pending, _) in out.iter().take(over) {
            if !markets.contains(&pending.change.market) {
                markets.push(pending.change.market);
            }
        }
        let reason = "한 tick에 보낼 사건의 합이 한 깨우기 상한을 넘었다";
        warn_dropped(over, &markets, reason);
        self.dropped += over;
        out.into_iter()
            .skip(over)
            .map(|(pending, held)| TriggerEvent::Data {
                change: pending.change,
                held,
            })
            .collect()
    }

    /// 정규장 밖 시장의 묶음과, 이번 정규장 시작 전에 받은 사건을 보관으로 옮긴다(보관도 상한).
    fn hold_off_hours(
        &mut self,
        now: DateTime<Utc>,
        calendars: &HashMap<Market, MarketCalendar>,
        limit: usize,
    ) {
        let mut moved: Vec<(Market, Vec<Pending>)> = Vec::new();
        for batch in &mut self.batches {
            let market = batch.key.0;
            let early: Vec<Pending> = match status(calendars.get(&market), now) {
                Status::Closed => std::mem::take(&mut batch.events),
                Status::Open(Some(start)) => {
                    let (early, rest) = std::mem::take(&mut batch.events)
                        .into_iter()
                        .partition(|pending| pending.pushed_at < start);
                    batch.events = rest;
                    early
                }
                Status::Open(None) => Vec::new(),
            };
            if !early.is_empty() {
                moved.push((market, early));
            }
        }
        self.batches.retain(|batch| !batch.events.is_empty());
        for (market, events) in moved {
            let index = match self.held.iter().position(|(m, _)| *m == market) {
                Some(index) => index,
                None => {
                    self.held.push((market, Vec::new()));
                    self.held.len() - 1
                }
            };
            let held = &mut self.held[index].1;
            for pending in events {
                insert_sorted(held, pending);
            }
            let over = cap(held, limit);
            warn_dropped(over, &[market], "장 밖 보관이 상한을 넘었다");
            self.dropped += over;
        }
    }

    /// 다음에 볼 때까지: 장이 열린 시장의 묶음은 조용한 시간 · 최대 기다림, 보관은 정규장 시작.
    pub(crate) fn next_due(
        &self,
        now: DateTime<Utc>,
        calendars: &HashMap<Market, MarketCalendar>,
        config: &DataTriggerConfig,
    ) -> Option<Duration> {
        let mut soonest: Option<DateTime<Utc>> = None;
        let mut consider = |at: Option<DateTime<Utc>>| {
            if let Some(at) = at {
                soonest = Some(soonest.map_or(at, |s| s.min(at)));
            }
        };
        for batch in &self.batches {
            match status(calendars.get(&batch.key.0), now) {
                Status::Open(_) => consider(Some(due_at(batch, now, config))),
                Status::Closed => consider(next_open(calendars.get(&batch.key.0), now)),
            }
        }
        for (market, _) in &self.held {
            match status(calendars.get(market), now) {
                Status::Open(_) => consider(Some(now)),
                Status::Closed => consider(next_open(calendars.get(market), now)),
            }
        }
        soonest.map(|at| at.signed_duration_since(now).to_std().unwrap_or_default())
    }
}

/// 묶음을 보낼 때: 마지막 사건 뒤 조용한 시간, 첫 사건 뒤 최대 기다림 가운데 이른 것.
/// 시계가 첫 사건보다 뒤로 갔으면 굶기지 않게 지금 보낸다.
fn due_at(batch: &Batch, now: DateTime<Utc>, config: &DataTriggerConfig) -> DateTime<Utc> {
    if now < batch.first_at {
        return now;
    }
    let quiet = after(batch.last_at, config.quiet);
    let max = after(batch.first_at, config.max_wait);
    match (quiet, max) {
        (Some(a), Some(b)) => a.min(b),
        (Some(a), None) | (None, Some(a)) => a,
        (None, None) => DateTime::<Utc>::MAX_UTC,
    }
}

fn is_due(batch: &Batch, now: DateTime<Utc>, config: &DataTriggerConfig) -> bool {
    due_at(batch, now, config) <= now
}

/// 그 시장의 정규장 여부. 달력이 없는 시장은 장 시간을 몰라 열린 것으로 본다(보관하면 끝없이 쌓인다).
fn status(calendar: Option<&MarketCalendar>, now: DateTime<Utc>) -> Status {
    let Some(calendar) = calendar else {
        return Status::Open(None);
    };
    match calendar.session_at(now) {
        Some(session) if session.kind == SessionKind::Regular => Status::Open(Some(session.start)),
        _ => Status::Closed,
    }
}

/// 지금 뒤의 첫 정규장 시작.
fn next_open(calendar: Option<&MarketCalendar>, now: DateTime<Utc>) -> Option<DateTime<Utc>> {
    let calendar = calendar?;
    [
        &calendar.previous_business_day,
        &calendar.today,
        &calendar.next_business_day,
    ]
    .into_iter()
    .flat_map(|day| day.sessions.iter())
    .filter(|session| session.kind == SessionKind::Regular && session.start > now)
    .map(|session| session.start)
    .min()
}

/// 이른 순(낸 시각, 받은 순)을 지키며 넣는다.
fn insert_sorted(events: &mut Vec<Pending>, pending: Pending) {
    let order = pending.order();
    let index = events.partition_point(|event| event.order() <= order);
    events.insert(index, pending);
}

/// 상한을 넘으면 가장 오래된 것부터 버리고 버린 수를 돌려준다.
fn cap(events: &mut Vec<Pending>, limit: usize) -> usize {
    let over = events.len().saturating_sub(limit);
    events.drain(..over);
    over
}

/// 버린 자료 사건을 기록에 보인다: 수 · 시장 · 까닭만(글 · 제목은 싣지 않는다). 가장 오래된 것부터 버린다.
pub(crate) fn warn_dropped(dropped: usize, markets: &[Market], reason: &'static str) {
    if dropped > 0 {
        tracing::warn!(dropped, ?markets, reason, "자료 사건을 버렸다");
    }
}

/// `at` 뒤 `span`. 넘치면 없다(끝나지 않는다).
fn after(at: DateTime<Utc>, span: Duration) -> Option<DateTime<Utc>> {
    chrono::Duration::from_std(span)
        .ok()
        .and_then(|span| at.checked_add_signed(span))
}

fn truncate_title(title: &mut String) {
    if let Some((index, _)) = title.char_indices().nth(TITLE_LIMIT) {
        title.truncate(index);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DataKind;

    fn config() -> DataTriggerConfig {
        crate::TriggerConfig::new(vec![], vec![]).data
    }

    fn at(seconds: i64) -> DateTime<Utc> {
        DateTime::<Utc>::from_timestamp(1_800_000_000 + seconds, 0).unwrap_or_default()
    }

    fn change(source: &str, id: &str, symbol: &str) -> DataChange {
        DataChange {
            kind: DataKind::News,
            source: source.to_string(),
            id: id.to_string(),
            revision: None,
            market: Market::Kr,
            symbol: Some(Symbol::new(symbol)),
            title: "제".repeat(300),
            published_at: at(0),
            learned_at: at(0),
        }
    }

    #[test]
    fn 글은_200자에서_자른다() {
        let mut state = DataState::default();
        state.push(change("naver", "a", "005930"), at(0), &config());
        let events = state.collect(at(30), &HashMap::new(), &config(), false);
        let [TriggerEvent::Data { change, .. }] = &events[..] else {
            panic!("{events:?}");
        };
        assert_eq!(change.title.chars().count(), TITLE_LIMIT);
    }

    #[test]
    fn 중복_표는_상한을_넘지_않는다() {
        let mut state = DataState::default();
        let mut cfg = config();
        cfg.per_source_limit = u32::MAX;
        for n in 0..(DEDUPE_CAPACITY + 100) {
            state.push(change("naver", &format!("{n}"), "005930"), at(0), &cfg);
        }
        assert!(state.seen.len() <= DEDUPE_CAPACITY);
        assert!(state.seen_order.len() <= DEDUPE_CAPACITY);
        // 한 시간이 지나면 지워진다.
        state.push(change("naver", "late", "005930"), at(3600), &cfg);
        assert_eq!(state.seen.len(), 1);
    }

    #[test]
    fn 묶음_수와_출처_수는_상한을_넘지_않는다() {
        let mut state = DataState::default();
        let mut cfg = config();
        cfg.per_source_limit = u32::MAX;
        for n in 0..(MAX_BATCHES + 10) {
            state.push(
                change("naver", &format!("{n}"), &format!("{n:06}")),
                at(0),
                &cfg,
            );
        }
        assert_eq!(state.batches.len(), MAX_BATCHES);
        assert_eq!(state.take_dropped(), 10);
        for n in 0..(MAX_SOURCES + 10) {
            state.push(change(&format!("s{n}"), "x", "005930"), at(1), &cfg);
        }
        assert!(state.sources.len() <= MAX_SOURCES);
        // 창이 지나면 출처 표도 비워진다.
        state.push(change("last", "y", "005930"), at(100), &cfg);
        assert_eq!(state.sources.len(), 1);
    }

    #[test]
    fn 대상이_실행_중이면_내지_않고_남긴다() {
        let mut state = DataState::default();
        state.push(change("naver", "a", "005930"), at(0), &config());
        assert!(
            state
                .collect(at(30), &HashMap::new(), &config(), true)
                .is_empty()
        );
        assert_eq!(
            state
                .collect(at(31), &HashMap::new(), &config(), false)
                .len(),
            1
        );
    }

    #[test]
    fn 시계가_뒤로_가면_굶기지_않고_보낸다() {
        let mut state = DataState::default();
        state.push(change("naver", "a", "005930"), at(100), &config());
        assert_eq!(
            state
                .collect(at(0), &HashMap::new(), &config(), false)
                .len(),
            1
        );
    }
}
