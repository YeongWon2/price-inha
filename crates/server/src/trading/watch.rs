//! (10차 10a 1 · 2번) 종목별 최근 시세로 거래 상태(VI · 거래정지)를 추정하고, 시세를 받은 뒤에 이상을 본다.
//!
//! - 추정(결정 2026-11-21 5 · 11 · 14번): 상태를 주는 공급자가 없으므로 종목별 최근 시세 버퍼와 지난 정규장
//!   마지막 시세(기준가)로 `pi_market::estimate_status` → `resolve_status` →
//!   `pi_rules::SymbolStatus::from_trading_status`를 만든다. 기준은 `StatusPolicy::for_market(달력의 시장)`뿐이다.
//!   버퍼에는 [`crate::Trading::on_quote`]로 받은 시세만 들어간다(다른 경로의 시세를 섞지 않는다).
//! - 이상 감지(결정 8 · 13번): 시세 흐름이 한 바퀴 돌 때마다(받았든 못 받았든) `pi_rules::evaluate_health`를
//!   부른다. 시세를 받은 **뒤에** 보므로 장 시작 순간의 밤사이 낡은 시세로 멈추지 않는다. 비정상이 한 번이라도
//!   보이면 "정상이 된 첫 시각"을 지우고, 풀리면 이상 감지 멈춤 → 쉬는 중으로 옮긴 뒤 그날 계획을 적용한다.
//! - 끝없이 쌓이지 않게 종목 수와 종목마다의 시세 수 · 나이에 상한을 둔다.

use std::collections::{HashMap, VecDeque};
use std::time::Duration;

use chrono::{DateTime, NaiveDate, Utc};
use pi_market::{
    MarketCalendar, Quote, SessionKind, StatusPolicy, Symbol, estimate_status, resolve_status,
};
use pi_rules::{
    Health, HealthTransition, HealthWatch, RecoveryPolicy, SymbolStatus, evaluate_health,
    is_abnormal,
};
use pi_store::{TeamCause, TeamState};
use rust_decimal::Decimal;
use serde_json::{Map, Value, json};

use super::base::{lock, millis};
use super::day::status;
use super::{DayPlan, OrderRequest, Trading};
use crate::TradingError;

/// 저장되는 이벤트 종류: 그 날 정규장의 종목별 마지막 시세(다음 거래일의 기준가). 화면에는 보내지 않는다.
/// 내용 `{"date": "YYYY-MM-DD", "prices": {종목: 십진수 글자}}`.
pub(crate) const KIND_REFERENCE_PRICES: &str = "reference_prices";

/// 종목 하나에 간직하는 최근 시세 수의 상한.
const RECENT_MAX: usize = 720;
/// 최근 시세를 간직하는 가장 긴 시간(지켜보기 60초를 넉넉히 덮는다).
const RECENT_KEEP: Duration = Duration::from_secs(180);
/// 간직하는 종목 수의 상한. 넘으면 가장 오래 시세가 없던 종목부터 버린다.
const SYMBOLS_MAX: usize = 512;
/// 이상 감지 멈춤을 푸는 정상 유지 시간의 최솟값(0으로 두어 멈춤과 풀림이 번갈아 생기지 않게).
const MIN_RECOVERY_HOLD: Duration = Duration::from_secs(1);
/// 시세 공급자의 실패가 이만큼 이어지면 API 오류로 본다(늦은 시세 기준 N6과 같은 30초).
const API_ERROR_AFTER: Duration = Duration::from_secs(30);
/// 자료 받기가 이만큼 이어서 모두 실패하면 자료 이상으로 본다.
const DATA_FAILURES_FOR_ANOMALY: u32 = 3;
/// 마지막 자료 받기 실패 뒤 이만큼 지나면 다음 논의 때까지 정상으로 본다(논의가 없어도 자료 이상이 풀린다).
const DATA_ANOMALY_KEEP: Duration = Duration::from_secs(600);
/// 비정상을 이만큼 이어서 봐야 멈춘다(시계가 건너뛴 순간의 한 번은 다음 시세에서 다시 본다).
const ABNORMAL_ROUNDS_TO_HALT: u32 = 2;

/// 정규장에서 본 마지막 시세 하나.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Close {
    date: NaiveDate,
    price: Decimal,
}

/// 추정과 이상 감지에 쓰는 기억.
#[derive(Default)]
pub(super) struct Watch {
    /// 종목 → 최근 시세(받은 순서).
    recent: HashMap<String, VecDeque<Quote>>,
    /// 종목 → 가장 최근 정규장에서 본 마지막 시세.
    closes: HashMap<String, Close>,
    /// 종목 → 그 앞 정규장의 마지막 시세.
    before: HashMap<String, Close>,
    /// 모든 것이 정상이 된 첫 시각(지금 정상이 아니면 없음).
    healthy_since: Option<DateTime<Utc>>,
    /// 이어서 본 비정상 횟수.
    abnormal_rounds: u32,
    /// 시세 공급자가 실패하기 시작한 시각(받으면 지운다).
    feed_failing_since: Option<DateTime<Utc>>,
    /// 이어진 자료 받기 실패 수(키가 있는 출처에 물어 모두 실패한 논의의 수).
    data_failures: u32,
    /// 마지막으로 자료 받기에 실패한 시각.
    data_failed_at: Option<DateTime<Utc>>,
}

impl Watch {
    /// 저장된 기준가([`KIND_REFERENCE_PRICES`])로 되살린다.
    pub(super) fn restore(saved: Option<&Value>) -> Watch {
        let mut watch = Watch::default();
        let Some(saved) = saved else {
            return watch;
        };
        let date = saved["date"]
            .as_str()
            .and_then(|text| NaiveDate::parse_from_str(text, "%Y-%m-%d").ok());
        let (Some(date), Some(prices)) = (date, saved["prices"].as_object()) else {
            return watch;
        };
        for (symbol, price) in prices.iter().take(SYMBOLS_MAX) {
            if let Some(price) = price.as_str().and_then(|text| text.parse().ok()) {
                watch.closes.insert(symbol.clone(), Close { date, price });
            }
        }
        watch
    }

    /// 받은 시세 하나를 기억한다. 오늘 정규장 안의 시세면 그 종목의 정규장 마지막 시세로도 둔다.
    fn note(&mut self, quote: &Quote, calendar: Option<&MarketCalendar>) {
        let symbol = quote.symbol.as_str();
        let recent = self.recent.entry(symbol.to_string()).or_default();
        recent.push_back(quote.clone());
        let oldest = chrono::Duration::from_std(RECENT_KEEP)
            .ok()
            .and_then(|keep| quote.received_at.checked_sub_signed(keep));
        while recent.len() > RECENT_MAX
            || recent
                .front()
                .is_some_and(|first| oldest.is_some_and(|oldest| first.received_at < oldest))
        {
            recent.pop_front();
        }
        if let Some(calendar) = calendar {
            let at = quote.received_at;
            let regular = calendar.today.sessions.iter().any(|session| {
                session.kind == SessionKind::Regular && session.start <= at && at < session.end
            });
            if regular {
                let close = Close {
                    date: calendar.today.date,
                    price: quote.price,
                };
                if let Some(old) = self.closes.insert(symbol.to_string(), close)
                    && old.date < close.date
                {
                    self.before.insert(symbol.to_string(), old);
                }
            }
        }
        self.shed(symbol);
    }

    /// 종목 수가 상한을 넘으면 가장 오래 시세가 없던 종목(방금 받은 종목은 빼고)을 버린다.
    fn shed(&mut self, keep: &str) {
        while self.recent.len() > SYMBOLS_MAX {
            let oldest = self
                .recent
                .iter()
                .filter(|(symbol, _)| symbol.as_str() != keep)
                .min_by_key(|(_, quotes)| quotes.back().map(|quote| quote.received_at))
                .map(|(symbol, _)| symbol.clone());
            let Some(symbol) = oldest else {
                return;
            };
            self.recent.remove(&symbol);
            self.closes.remove(&symbol);
            self.before.remove(&symbol);
        }
    }

    /// 그 종목의 기준가: 달력의 앞 거래일 정규장에서 본 마지막 시세. 없으면 없음(결정 11번: 그대로 막힌다).
    fn reference(&self, symbol: &str, calendar: &MarketCalendar) -> Option<Decimal> {
        let day = calendar.previous_business_day.date;
        [self.closes.get(symbol), self.before.get(symbol)]
            .into_iter()
            .flatten()
            .find(|close| close.date == day)
            .map(|close| close.price)
    }

    /// 자료 받기 한 번의 결과를 센다: 받았으면 처음부터, 실패했으면 하나 더하고 그 시각을 적는다.
    fn note_data(&mut self, ok: bool, now: DateTime<Utc>) {
        if ok {
            self.data_failures = 0;
            self.data_failed_at = None;
        } else {
            self.data_failures = self.data_failures.saturating_add(1);
            self.data_failed_at = Some(now);
        }
    }

    /// 자료 이상인가: 연속 실패가 기준을 넘었고 마지막 실패가 아직 오래되지 않았다. 마지막 실패 뒤
    /// [`DATA_ANOMALY_KEEP`]이 지나면 다음 논의(다음 자료 받기) 때까지 정상으로 본다.
    fn data_anomaly(&self, now: DateTime<Utc>) -> bool {
        let fresh = self.data_failed_at.is_some_and(|at| {
            now.signed_duration_since(at)
                .to_std()
                .is_ok_and(|since| since < DATA_ANOMALY_KEEP)
        });
        fresh && self.data_failures >= DATA_FAILURES_FOR_ANOMALY
    }

    /// 그 날 정규장의 마지막 시세들(저장할 내용). 하나도 없으면 없음.
    fn closes_on(&self, date: NaiveDate) -> Option<Value> {
        let prices: Map<String, Value> = self
            .closes
            .iter()
            .filter(|(_, close)| close.date == date)
            .map(|(symbol, close)| (symbol.clone(), json!(close.price.to_string())))
            .collect();
        (!prices.is_empty())
            .then(|| json!({ "date": date.format("%Y-%m-%d").to_string(), "prices": prices }))
    }
}

/// 이상 감지 멈춤을 푸는 기준: 기본값(60초)이되 최솟값 1초를 지킨다.
fn recovery() -> RecoveryPolicy {
    let policy = RecoveryPolicy::default();
    RecoveryPolicy {
        hold: policy.hold.max(MIN_RECOVERY_HOLD),
    }
}

impl Trading {
    /// 받은 시세를 추정용 버퍼에 넣는다([`Trading::on_quote`]의 길에서만 부른다).
    pub(super) fn observe(&self, quote: &Quote, calendar: Option<&MarketCalendar>) {
        lock(&self.watch).note(quote, calendar);
    }

    /// 그 종목의 거래 상태 추정(시연이 아닌 모드의 점검 입력). 달력을 모르면 '모름'이다.
    pub(crate) fn estimated_status(&self, symbol: &str, now: DateTime<Utc>) -> SymbolStatus {
        let unknown = SymbolStatus {
            vi: None,
            halted: None,
        };
        let Some(calendar) = self.calendar(now) else {
            return unknown;
        };
        // 시장과 정책이 어긋나지 않게 달력의 시장으로만 만든다.
        let policy = StatusPolicy::for_market(calendar.market);
        let (recent, reference) = {
            let watch = lock(&self.watch);
            let recent: Vec<Quote> = watch
                .recent
                .get(symbol)
                .map(|quotes| quotes.iter().cloned().collect())
                .unwrap_or_default();
            (recent, watch.reference(symbol, &calendar))
        };
        let estimate = estimate_status(&recent, reference, &calendar, now, &policy);
        // 상태를 주는 공급자가 없다: 추정을 쓴다.
        let (resolved, _source) = resolve_status(None, &estimate);
        SymbolStatus::from_trading_status(resolved)
    }

    /// 그 종목의 기준가(지난 정규장 마지막 시세). 모르면 없음.
    pub(crate) fn reference_price(&self, symbol: &str, now: DateTime<Utc>) -> Option<Decimal> {
        let calendar = self.calendar(now)?;
        lock(&self.watch).reference(symbol, &calendar)
    }

    /// 그 날 정규장의 마지막 시세들을 저장소에 남긴다(다음 거래일의 기준가, 다시 켜도 남는다).
    pub(super) fn save_references(&self, date: NaiveDate) -> Result<(), TradingError> {
        let payload = lock(&self.watch).closes_on(date);
        if let Some(payload) = payload {
            let at = millis(self.clock.now());
            self.store.append_at(KIND_REFERENCE_PRICES, &payload, at)?;
        }
        Ok(())
    }

    /// 자료와 연결의 상태(점검과 이상 감지에 함께 쓴다): 계좌 맞춤, 시세 공급자 오류, 자료 받기의 연속 실패.
    pub(super) fn health(&self, now: DateTime<Utc>) -> Health {
        let watch = lock(&self.watch);
        let api_error = watch.feed_failing_since.is_some_and(|since| {
            now.signed_duration_since(since)
                .to_std()
                .is_ok_and(|failing| failing > API_ERROR_AFTER)
        });
        Health {
            account_in_sync: self.executor.ready(),
            api_error,
            data_anomaly: watch.data_anomaly(now),
        }
    }

    /// 자료 받기 한 번의 결과를 받는다. `ok`가 거짓인 것은 키가 있는 출처에 물었는데 모두 실패했을 때뿐이다
    /// (키가 없어 묻지 못한 것은 실패가 아니고, 물은 것이 하나도 없으면 정상이다 — 10a 보강 3번).
    pub(crate) fn note_data(&self, ok: bool) {
        let now = self.clock.now();
        lock(&self.watch).note_data(ok, now);
    }

    /// 다시 점검할 주문 안의 상태 추정 · 기준가를 그 자리에서 새로 구한다(승인 답 · 답 없음, 10a 보강 1번).
    /// 시연 모드(VI · 거래정지가 없다고 안다)와 파는 주문(상태를 모른다고 막지 않는다)은 받은 그대로다.
    pub(super) fn refreshed(&self, mut request: OrderRequest, now: DateTime<Utc>) -> OrderRequest {
        if self.desk.demo() || !request.draft.purpose.is_buy() {
            return request;
        }
        let symbol = request.draft.symbol.as_str().to_string();
        request.status = self.estimated_status(&symbol, now);
        if let Some(reference) = self.reference_price(&symbol, now) {
            request.reference_price = reference;
        }
        request
    }

    /// 비정상이 생겼다(주문 실행기의 이상 감지 멈춤 등): 정상 시간을 처음부터 다시 센다.
    pub(super) fn unhealthy(&self) {
        lock(&self.watch).healthy_since = None;
    }

    /// 시세 흐름이 한 바퀴 돈 뒤 부른다: 공급자 신호를 받고 이상을 본다. `ok`는 시세를 받았는가,
    /// `watching`은 지금 받고 있는 종목(증권사가 거절한 종목은 뺀 것)이다. 실패는 기록만 한다.
    pub(crate) fn after_feed(&self, ok: bool, watching: &[Symbol]) {
        let now = self.clock.now();
        {
            let mut watch = lock(&self.watch);
            if ok {
                watch.feed_failing_since = None;
            } else if watch.feed_failing_since.is_none() {
                watch.feed_failing_since = Some(now);
            }
        }
        if let Err(error) = self.check_health(now, watching) {
            tracing::warn!(%error, "이상 감지를 보지 못했습니다");
        }
    }

    /// 시세 흐름이 받고 있는 종목의 마지막 시세로 `pi_rules::evaluate_health`를 부르고 요구한 전이를 한다.
    fn check_health(&self, now: DateTime<Utc>, watching: &[Symbol]) -> Result<(), TradingError> {
        let Some(calendar) = self.calendar(now) else {
            return Ok(());
        };
        // 시세 흐름이 지금 받고 있는 종목만 본다. 흐름이 받지 않는 보유 종목은 시세가 올 길이 없어, 넣으면 다음
        // 거래일 장 시작마다 낡은 시세로 멈춘다(잠긴 round9의 밤을 넘긴 보유). 보유 종목은 시세 흐름이 함께
        // 받게 된 뒤에(10b) 여기에 든다.
        let symbols: Vec<Symbol> = watching.to_vec();
        let quotes: Vec<(Symbol, Option<Quote>)> = symbols
            .into_iter()
            .map(|symbol| {
                let quote = self.last_quote(symbol.as_str());
                (symbol, quote)
            })
            .collect();
        let health = self.health(now);
        let _turn = lock(&self.team_turn);
        let team = self.store.team_state()?;
        let mut probe = HealthWatch {
            team: status(team),
            now,
            calendar: &calendar,
            quotes: &quotes,
            stale: self.paper_config.stale,
            health,
            healthy_since: None,
            recovery: recovery(),
        };
        let (since, rounds) = {
            let mut watch = lock(&self.watch);
            if is_abnormal(&probe) {
                // 비정상이 한 번이라도 생기면 정상 시간을 지운다.
                watch.healthy_since = None;
                watch.abnormal_rounds = watch.abnormal_rounds.saturating_add(1);
            } else {
                watch.abnormal_rounds = 0;
                watch.healthy_since.get_or_insert(now);
            }
            (watch.healthy_since, watch.abnormal_rounds)
        };
        probe.healthy_since = since;
        match evaluate_health(&probe) {
            Some(HealthTransition::AnomalyHalt) if rounds >= ABNORMAL_ROUNDS_TO_HALT => {
                let cause = TeamCause::AnomalyDetected;
                let detail = "시세가 늦거나 자료 · 연결에 이상이 있음";
                self.change_team(TeamState::AnomalyHalt, cause, false, detail)?;
            }
            Some(HealthTransition::Recovered) => self.recovered()?,
            Some(HealthTransition::AnomalyHalt) | None => {}
        }
        Ok(())
    }

    /// 이상이 풀렸다: 이상 감지 멈춤 → 쉬는 중으로 옮긴 뒤 그날 계획(매매면 운용 중)을 적용한다.
    /// 팀 상태 줄을 쥐고 부른다.
    fn recovered(&self) -> Result<(), TradingError> {
        let cause = TeamCause::AnomalyCleared;
        let detail = "이상이 풀리고 정상이 이어짐";
        let state = self.change_team(TeamState::Resting, cause, false, detail)?;
        if state == TeamState::Resting && self.plan_today() == Some(DayPlan::Trade) {
            match self.start_trading(state) {
                // 다시 멈춰야 하면(손실 한도) 그 상태로 둔다.
                Ok(_) | Err(TradingError::Halted(_)) => {}
                Err(error) => return Err(error),
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;
    use pi_market::{Currency, Market, Session, TradingDay};

    use super::*;

    fn at(day: u32, hour: u32, minute: u32, second: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 11, day, hour, minute, second)
            .single()
            .unwrap_or_default()
    }

    fn day(day: u32) -> TradingDay {
        TradingDay {
            date: NaiveDate::from_ymd_opt(2026, 11, day).unwrap_or_default(),
            sessions: vec![Session {
                kind: SessionKind::Regular,
                start: at(day, 0, 0, 0),
                end: at(day, 6, 30, 0),
            }],
        }
    }

    fn calendar(today: u32) -> MarketCalendar {
        MarketCalendar {
            market: Market::Kr,
            today: day(today),
            previous_business_day: day(today - 1),
            next_business_day: day(today + 1),
        }
    }

    fn quote(symbol: &str, price: i64, received_at: DateTime<Utc>) -> Quote {
        Quote {
            symbol: Symbol::new(symbol),
            price: Decimal::from(price),
            currency: Currency::Krw,
            exchange_time: Some(received_at),
            received_at,
        }
    }

    #[test]
    fn 기준가는_앞_거래일_정규장의_마지막_시세다() {
        let mut watch = Watch::default();
        watch.note(&quote("A", 100, at(17, 1, 0, 0)), Some(&calendar(17)));
        watch.note(&quote("A", 110, at(17, 2, 0, 0)), Some(&calendar(17)));
        // 정규장 밖의 시세는 기준가가 되지 않는다.
        watch.note(&quote("A", 999, at(17, 7, 0, 0)), Some(&calendar(17)));
        assert_eq!(watch.reference("A", &calendar(17)), None, "첫날은 없다");
        assert_eq!(
            watch.reference("A", &calendar(18)),
            Some(Decimal::from(110))
        );
        watch.note(&quote("A", 120, at(18, 1, 0, 0)), Some(&calendar(18)));
        assert_eq!(
            watch.reference("A", &calendar(18)),
            Some(Decimal::from(110))
        );
        assert_eq!(
            watch.reference("A", &calendar(20)),
            None,
            "더 오래된 것은 쓰지 않는다"
        );
    }

    #[test]
    fn 기준가는_저장했다가_되살린다() {
        let mut watch = Watch::default();
        watch.note(&quote("A", 100, at(17, 1, 0, 0)), Some(&calendar(17)));
        let date = NaiveDate::from_ymd_opt(2026, 11, 17).unwrap_or_default();
        let saved = watch.closes_on(date);
        let back = Watch::restore(saved.as_ref());
        assert_eq!(back.reference("A", &calendar(18)), Some(Decimal::from(100)));
        assert_eq!(Watch::default().closes_on(date), None);
    }

    #[test]
    fn 자료_이상은_연속_실패_세_번부터이고_마지막_실패_뒤_시간이_지나면_풀린다() {
        let mut watch = Watch::default();
        let start = at(17, 1, 0, 0);
        watch.note_data(false, start);
        watch.note_data(false, start);
        assert!(!watch.data_anomaly(start), "두 번까지는 아니다");
        watch.note_data(false, start);
        assert!(watch.data_anomaly(start));
        let keep = chrono::Duration::from_std(DATA_ANOMALY_KEEP).unwrap_or_default();
        assert!(watch.data_anomaly(start + keep - chrono::Duration::seconds(1)));
        // 논의가 없어도 풀린다.
        assert!(!watch.data_anomaly(start + keep));
        // 다음 논의에서 또 실패하면 다시 자료 이상이다.
        watch.note_data(false, start + keep);
        assert!(watch.data_anomaly(start + keep));
        // 한 번 받으면 처음부터 센다.
        watch.note_data(true, start + keep);
        assert!(!watch.data_anomaly(start + keep));
        watch.note_data(false, start + keep);
        assert!(!watch.data_anomaly(start + keep));
    }

    #[test]
    fn 시세_버퍼와_종목_수에_상한이_있다() {
        let mut watch = Watch::default();
        let start = at(17, 1, 0, 0);
        for step in 0..2_000 {
            let when = start + chrono::Duration::milliseconds(step * 10);
            watch.note(&quote("A", 100, when), None);
        }
        assert!(watch.recent["A"].len() <= RECENT_MAX);
        // 오래된 시세는 시간으로도 버린다.
        watch.note(&quote("A", 100, start + chrono::Duration::hours(1)), None);
        assert_eq!(watch.recent["A"].len(), 1);
        for n in 0..(SYMBOLS_MAX + 20) {
            let when = start
                + chrono::Duration::hours(2)
                + chrono::Duration::seconds(i64::try_from(n).unwrap_or(0));
            watch.note(&quote(&format!("S{n}"), 100, when), Some(&calendar(17)));
        }
        assert!(watch.recent.len() <= SYMBOLS_MAX);
        assert!(
            !watch.recent.contains_key("A"),
            "가장 오래된 종목부터 버린다"
        );
    }
}
