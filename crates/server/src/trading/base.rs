//! 주문 길의 바탕: 여는 일, 달력, 시세 기억, 통화별 장부(평가액 · 그날 시작 · 최고), 팀 상태 바꾸기,
//! 하루 기록 읽기. 쓰는 템플릿은 `chosen`이 고른다.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError};
use std::time::Duration;

use chrono::{DateTime, NaiveDate, Utc};
use pi_approval::{Approvals, WaitTimes};
use pi_exec::Executor;
use pi_gate::Publisher;
use pi_market::{Clock, Currency, Market, MarketCalendar, Quote, StalePolicy};
use pi_paper::{
    InitialCash, LimitFillPrice, MarketRules, MaxFill, PaperConfig, ProceedsUsable, RoundingMode,
    UnverifiedRules,
};
use pi_rules::{Book, Position, RulesConfig};
use pi_store::{Store, StoreError, TeamCause, TeamChange, TeamState, UnixMillis};
use rust_decimal::Decimal;
use serde_json::Value;

use super::marks::{KIND_LOSS_BASIS, Marks};
use super::watch::{KIND_REFERENCE_PRICES, Watch};
use super::{
    CalendarFn, DayPlan, KIND_DAY_CLOSE, KIND_DAY_PLAN, KIND_DAY_START, Trading, TradingWiring,
};
use crate::desk::Desk;
use crate::{AppError, TradingError};

/// 장부를 셈하는 통화(통화끼리 더하지 않는다, 결정 31번).
const CURRENCIES: [Currency; 2] = [Currency::Krw, Currency::Usd];
/// 하루 기록을 읽을 때 한 번에 읽는 이벤트 수.
const READ_PAGE: usize = 1_000;

/// [`Trading::open`]의 재료.
pub(crate) struct TradingParts {
    pub(crate) store: Store,
    pub(crate) clock: Arc<dyn Clock>,
    pub(crate) market: Market,
    pub(crate) desk: Arc<Desk>,
    pub(crate) wiring: TradingWiring,
}

/// [`crate::App::start`]가 쓰는 기본 주문 길 값: 원화 10,000,000 · 달러 10,000으로 여는 모의계좌,
/// 빈 금지 종목 · 업종표, 시세 흐름이 받은 달력.
///
/// 공식 출처로 확인되지 않은 모의계좌 값(수수료율 0.015% · 시장가 미끄러짐 0.1% · 내림 · 체결 수량 제한
/// 없음 · 결제 뒤에 판 돈을 씀)은 보수적인 쪽으로 잡은 추정값이다.
pub(crate) fn default_wiring() -> TradingWiring {
    let unverified = || UnverifiedRules {
        commission_rate: Decimal::new(15, 5),
        market_slippage_rate: Decimal::new(1, 3),
        rounding_mode: RoundingMode::Down,
        max_fill: MaxFill::Unlimited,
        proceeds_usable: ProceedsUsable::AfterSettlement,
    };
    let mut kr = MarketRules::kr(unverified());
    kr.limit_fill_price = LimitFillPrice::Limit;
    TradingWiring {
        paper: PaperConfig {
            initial_cash: InitialCash {
                krw: Decimal::from(10_000_000),
                usd: Decimal::from(10_000),
            },
            kr,
            us: MarketRules::us(unverified(), Decimal::ZERO),
            stale: StalePolicy {
                max_age: Duration::from_secs(30),
            },
            event_buffer: 256,
        },
        rules: RulesConfig::default(),
        calendar: None,
    }
}

impl Default for TradingWiring {
    /// [`default_wiring`]과 같다(실행 파일이 [`crate::App::start_with_secrets`]에 준다).
    fn default() -> Self {
        default_wiring()
    }
}

/// 달력이 오는 곳.
pub(super) enum Calendars {
    /// 조립하는 쪽이 준 것(날짜 → 달력).
    Fixed(CalendarFn),
    /// 시세 흐름이 받은 것(그 날짜의 것만 쓴다).
    Feed(Mutex<Option<MarketCalendar>>),
}

/// 하루 시작 · 마감을 남긴 날짜와 팀장의 마지막 판단.
#[derive(Default)]
pub(super) struct DayLog {
    pub(super) started: Option<NaiveDate>,
    pub(super) closed: Option<NaiveDate>,
    pub(super) plan: Option<(NaiveDate, DayPlan)>,
    /// (1차 보강) 마지막으로 남긴 손실 기준([`KIND_LOSS_BASIS`]).
    pub(super) basis: Option<Value>,
    /// (10차 10a) 마지막으로 남긴 정규장 마지막 시세들([`KIND_REFERENCE_PRICES`]).
    pub(super) references: Option<Value>,
}

/// 잠금 안에서 잡은 하루 일과 되돌릴 때 쓸 앞의 값.
#[derive(Clone, Copy)]
pub(super) enum Claim {
    /// 하루 시작(앞에 남긴 날짜).
    Start(Option<NaiveDate>),
    /// 하루 마감(앞에 남긴 날짜).
    Close(Option<NaiveDate>),
}

impl DayLog {
    /// 그 날에 지금 할 일(시작 · 마감)을 한 번만 잡는다. 잡으면 바로 "남겼다"로 적어, 겹쳐 부른 다른 쪽이
    /// 같은 날의 같은 일을 또 하지 않게 한다(V5: 그 날에 한 번만).
    pub(super) fn claim(&mut self, date: NaiveDate, open: bool, over: bool) -> Option<Claim> {
        let started = self.started == Some(date);
        if !started && open {
            let before = self.started.replace(date);
            Some(Claim::Start(before))
        } else if started && self.closed != Some(date) && over {
            let before = self.closed.replace(date);
            Some(Claim::Close(before))
        } else {
            None
        }
    }

    /// 잡은 일을 남기지 못했을 때 되돌린다(그 사이 다른 날짜가 적혔으면 그대로 둔다).
    pub(super) fn release(&mut self, date: NaiveDate, claim: Claim) {
        match claim {
            Claim::Start(before) if self.started == Some(date) => self.started = before,
            Claim::Close(before) if self.closed == Some(date) => self.closed = before,
            Claim::Start(_) | Claim::Close(_) => {}
        }
    }

    /// 저장된 이벤트에서 읽는다(다시 켜도 하루에 한 번씩만 남기게).
    fn read(store: &Store) -> Result<DayLog, StoreError> {
        let mut log = DayLog::default();
        let mut after = 0;
        loop {
            let events = store.read_after(after, READ_PAGE)?;
            let Some(last) = events.last() else {
                return Ok(log);
            };
            after = last.seq;
            for event in &events {
                log.absorb(&event.kind, &event.payload);
            }
        }
    }

    fn absorb(&mut self, kind: &str, payload: &Value) {
        if kind == KIND_LOSS_BASIS {
            self.basis = Some(merge_basis(self.basis.take(), payload));
            return;
        }
        if kind == KIND_REFERENCE_PRICES {
            self.references = Some(payload.clone());
            return;
        }
        let Some(date) = payload["date"]
            .as_str()
            .and_then(|text| NaiveDate::parse_from_str(text, "%Y-%m-%d").ok())
        else {
            return;
        };
        match kind {
            KIND_DAY_START => self.started = self.started.max(Some(date)),
            KIND_DAY_CLOSE => self.closed = self.closed.max(Some(date)),
            KIND_DAY_PLAN => {
                let plan = match payload["plan"].as_str() {
                    Some("trade") => DayPlan::Trade,
                    Some("rest") => DayPlan::Rest {
                        reason: payload["reason"].as_str().unwrap_or_default().to_string(),
                    },
                    _ => return,
                };
                self.plan = Some((date, plan));
            }
            _ => {}
        }
    }
}

/// (14차) 되읽을 때 손실 기준 기록을 합친다: 날짜 · 그날 시작은 마지막 기록의 것, 통화별 최고 평가액은
/// 지금까지 읽은 기록들의 `max`. 저장 순서가 뒤집혀 낮은 최고가 나중에 남아도 최고는 뒤로 가지 않는다.
fn merge_basis(before: Option<Value>, latest: &Value) -> Value {
    let mut merged = latest.clone();
    let Some(before) = before else { return merged };
    let (Some(old), Some(new)) = (before["peak"].as_object(), merged["peak"].as_object_mut())
    else {
        return merged;
    };
    for (code, old_text) in old {
        let old_peak = old_text.as_str().and_then(|t| t.parse::<Decimal>().ok());
        let new_peak = new
            .get(code)
            .and_then(Value::as_str)
            .and_then(|t| t.parse::<Decimal>().ok());
        if let Some(old_peak) = old_peak
            && new_peak.is_none_or(|n| old_peak > n)
        {
            new.insert(code.clone(), Value::String(old_peak.to_string()));
        }
    }
    merged
}

/// 승인 관리자의 시계 자리에 같은 시계를 끼운다.
struct ApprovalClock(Arc<dyn Clock>);

impl pi_approval::Clock for ApprovalClock {
    fn now(&self) -> UnixMillis {
        millis(self.0.now())
    }
}

/// 잠금이 독(poison)이어도 안의 값은 그대로 쓴다.
pub(super) fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

pub(super) fn millis(at: DateTime<Utc>) -> UnixMillis {
    UnixMillis(at.timestamp_millis())
}

/// 실행기 · 모의계좌 · 승인의 오류를 글로 옮긴다(비밀 값이 없다).
pub(super) fn exec_text(error: impl std::fmt::Display) -> TradingError {
    TradingError::Exec(error.to_string())
}

impl Trading {
    /// 주문 길을 연다: 저장소로 주문 실행기를 열고(계좌 스냅샷이 있으면 되살림) 맞춰 본 뒤,
    /// 같은 저장소 · 시계로 승인 관리자를 연다. 장부의 최고 평가액은 저장소에 남긴 손실 기준에서,
    /// 그날 시작 평가액은 남긴 날짜가 오늘이면 거기서 되읽는다(없으면 지금 평가액, 1차 보강).
    pub(crate) async fn open(parts: TradingParts) -> Result<Arc<Trading>, AppError> {
        let TradingParts {
            store,
            clock,
            market,
            desk,
            wiring,
        } = parts;
        let opened = Executor::open(wiring.paper.clone(), store.clone(), Arc::clone(&clock));
        let (executor, authority) = opened.map_err(exec_text)?;
        executor.reconcile().await.map_err(exec_text)?;
        let approval_clock = Arc::new(ApprovalClock(Arc::clone(&clock)));
        let approvals = Approvals::new(store.clone(), approval_clock, WaitTimes::default());
        let calendars = match wiring.calendar {
            Some(calendar) => Calendars::Fixed(calendar),
            None => Calendars::Feed(Mutex::new(None)),
        };
        let days = DayLog::read(&store)?;
        let today = crate::feed::market_date(market, clock.now());
        let marks = Marks::restore(
            days.basis.as_ref(),
            today,
            &wiring.paper.initial_cash,
            &CURRENCIES,
        );
        let watch = Watch::restore(days.references.as_ref());
        let trading = Arc::new(Trading {
            store,
            clock,
            market,
            desk,
            executor,
            authority,
            approvals,
            paper_config: wiring.paper,
            rules: wiring.rules,
            calendars,
            paper_day: Mutex::new(None),
            quotes: Mutex::new(HashMap::new()),
            marks: Mutex::new(marks),
            save_turn: Mutex::new(()),
            days: Mutex::new(days),
            waiting: Mutex::new(HashMap::new()),
            stops: Mutex::new(HashMap::new()),
            team_turn: Mutex::new(()),
            watch: Mutex::new(watch),
            due_turn: tokio::sync::Mutex::new(()),
            held_back: Mutex::new(HashMap::new()),
            publisher: OnceLock::new(),
            hooks: OnceLock::new(),
        });
        let now = trading.clock.now();
        trading.books(now).await?;
        // 꺼지기 직전에 승격대기로 남은 템플릿을 맞춰 본다(10a 보강 5번).
        trading.reconcile_promotions();
        Ok(trading)
    }

    /// 접속 관문이 뜬 뒤 이벤트를 내보내는 손잡이를 붙인다.
    pub(crate) fn attach(&self, publisher: Publisher) {
        let _ = self.publisher.set(publisher);
    }

    /// 이벤트를 저장한다(접속 관문이 붙어 있으면 화면에도 보낸다).
    pub(crate) async fn record(&self, kind: &str, payload: Value) -> Result<(), TradingError> {
        match self.publisher.get() {
            Some(publisher) => publisher
                .publish(kind, payload)
                .await
                .map(|_| ())
                .map_err(exec_text),
            None => {
                self.store
                    .append_at(kind, &payload, millis(self.clock.now()))?;
                Ok(())
            }
        }
    }

    /// 그 시각의 시장 날짜.
    pub(crate) fn date_of(&self, at: DateTime<Utc>) -> NaiveDate {
        crate::feed::market_date(self.market, at)
    }

    /// 오늘(시장 날짜)의 달력. 모르면 없음. 날짜가 바뀌면 모의계좌에도 넣는다.
    pub(crate) fn calendar(&self, now: DateTime<Utc>) -> Option<MarketCalendar> {
        let date = self.date_of(now);
        let calendar = match &self.calendars {
            Calendars::Fixed(calendar) => calendar(date),
            Calendars::Feed(held) => lock(held).clone().filter(|c| c.today.date == date)?,
        };
        let mut set = lock(&self.paper_day);
        if *set != Some(date) {
            self.executor.broker().set_calendar(calendar.clone());
            *set = Some(date);
        }
        Some(calendar)
    }

    /// 시세 흐름이 받은 달력을 받는다(조립하는 쪽이 달력을 주지 않았을 때만 쓴다).
    pub(crate) fn offer_calendar(&self, calendar: MarketCalendar) {
        if let Calendars::Feed(held) = &self.calendars
            && calendar.market == self.market
        {
            *lock(held) = Some(calendar);
            *lock(&self.paper_day) = None;
        }
    }

    pub(super) fn remember(&self, quote: &Quote) {
        lock(&self.quotes).insert(quote.symbol.as_str().to_string(), quote.clone());
    }

    pub(crate) fn last_quote(&self, symbol: &str) -> Option<Quote> {
        lock(&self.quotes).get(symbol).cloned()
    }

    /// 그 시장의 숫자 규칙.
    pub(super) fn market_rules(&self, market: Market) -> &MarketRules {
        match market {
            Market::Us => &self.paper_config.us,
            _ => &self.paper_config.kr,
        }
    }

    /// 통화별 장부를 모의계좌에서 셈한다. 평가에는 그 종목의 마지막 시세(없으면 평균 단가)를 쓰고,
    /// 최고 평가액을 올린다. 그날 시작 평가액이 아직 없으면 지금 평가액이다. 기준이 바뀌면 저장소에 남긴다.
    pub(super) async fn books(&self, now: DateTime<Utc>) -> Result<Vec<Book>, TradingError> {
        self.count_books(now, None)
    }

    /// 하루 시작의 장부: 셈하고 그날 시작 평가액을 **같은 `marks` 잠금 안에서** 새로 잡는다.
    /// 다른 `books()` 호출이 사이에 끼어 그날 시작 평가액이 어긋나지 않는다(13차 1번).
    pub(super) async fn books_for_day_start(
        &self,
        now: DateTime<Utc>,
        today: NaiveDate,
    ) -> Result<Vec<Book>, TradingError> {
        self.count_books(now, Some(today))
    }

    /// `books`의 몸통. 모의계좌 장부는 `marks` 잠금을 잡기 전에 한 번에 읽는다(보유와 현금이 섞이지 않는다).
    fn count_books(
        &self,
        now: DateTime<Utc>,
        reset_day: Option<NaiveDate>,
    ) -> Result<Vec<Book>, TradingError> {
        // (14차) 셈 · 꺼내기 · 저장을 저장 전용 잠금 안에서 한 줄로 세운다: 먼저 꺼낸 기록이 먼저 저장돼
        // 낮은 최고를 담은 기록이 나중에 남지 않는다. 잠금 순서는 언제나 저장 잠금 → `marks`(거꾸로 잡지 않는다).
        let _save_turn = lock(&self.save_turn);
        let snapshot = self.executor.broker().account_snapshot(now);
        let quotes = lock(&self.quotes).clone();
        let today = self.date_of(now);
        let mut marks = lock(&self.marks);
        let mut changed = false;
        let mut books = Vec::with_capacity(CURRENCIES.len());
        for currency in &CURRENCIES {
            let cash = match currency {
                Currency::Usd => &snapshot.usd,
                _ => &snapshot.krw,
            };
            let positions: Vec<Position> = snapshot
                .holdings
                .iter()
                .filter(|h| &h.currency == currency)
                .map(|h| Position {
                    market: h.market,
                    symbol: h.symbol.clone(),
                    quantity: h.quantity,
                    avg_price: h.average_purchase_price,
                    last_price: quotes
                        .get(h.symbol.as_str())
                        .map_or(h.average_purchase_price, |q| q.price),
                })
                .collect();
            let equity = positions
                .iter()
                .fold(cash.available + cash.held + cash.unsettled, |sum, p| {
                    sum.saturating_add(p.quantity.saturating_mul(p.last_price))
                });
            let (day_start, peak, moved) = marks.mark(currency, equity, today);
            changed |= moved;
            books.push(Book {
                currency: currency.clone(),
                equity,
                day_start_equity: day_start,
                peak_equity: peak,
                positions,
            });
        }
        marks.last.clone_from(&books);
        if let Some(day) = reset_day {
            changed |= marks.reset_day(&books, day);
            for book in &mut books {
                book.day_start_equity = book.equity;
            }
        }
        let payload = changed.then(|| marks.take_payload());
        drop(marks);
        if let Some(payload) = payload {
            self.save_basis(&payload)?;
        }
        Ok(books)
    }

    /// 손실 기준을 저장소에 남긴다(화면에는 보내지 않는다).
    fn save_basis(&self, payload: &Value) -> Result<(), TradingError> {
        #[cfg(test)]
        super::basis_order::before_save(self);
        self.store
            .append_at(KIND_LOSS_BASIS, payload, millis(self.clock.now()))?;
        Ok(())
    }

    /// 마지막으로 셈한 장부.
    pub(super) fn last_books(&self) -> Vec<Book> {
        lock(&self.marks).last.clone()
    }

    /// 팀 상태를 바꾼다. 저장소가 거부하면(허용되지 않는 전이) 바꾸지 않고 지금 상태를 돌려준다.
    pub(super) fn change_team(
        &self,
        to: TeamState,
        cause: TeamCause,
        owner_approved: bool,
        detail: &str,
    ) -> Result<TeamState, TradingError> {
        let change = TeamChange {
            to,
            at: millis(self.clock.now()),
            cause,
            detail: detail.to_string(),
            owner_approved,
        };
        match self.store.change_team_state(change) {
            Ok(record) => Ok(record.to),
            Err(StoreError::TransitionDenied { .. }) => Ok(self.store.team_state()?),
            Err(error) => Err(error.into()),
        }
    }

    /// 필수 설정(증권사 · AI 키, 종목)이 빠졌고 시연 모드가 아니면 [`TradingError::SetupRequired`](V31).
    pub(super) fn require_setup(&self) -> Result<(), TradingError> {
        if self.desk.demo() || self.desk.ready() {
            Ok(())
        } else {
            Err(TradingError::SetupRequired)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date(day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 11, day).unwrap_or_default()
    }

    // 결함(10차 10a 걸음 메모): 뒤에서 도는 tick과 하루 판단의 tick이 둘 다 "아직 시작을 남기지 않았다"를 본 뒤
    // 각자 남겨 같은 날짜의 `day_start`가 두 번 남았다. 잠금 안에서 먼저 잡으면 한쪽만 한다.
    #[test]
    fn 하루_시작과_마감은_겹쳐_불러도_그_날에_한_번만_잡힌다() {
        let mut log = DayLog::default();
        assert!(matches!(
            log.claim(date(17), true, false),
            Some(Claim::Start(None))
        ));
        assert!(
            log.claim(date(17), true, false).is_none(),
            "두 번째는 잡지 못한다"
        );
        assert!(matches!(
            log.claim(date(17), false, true),
            Some(Claim::Close(None))
        ));
        assert!(log.claim(date(17), false, true).is_none());
        // 다음 날도 한 번만.
        let next = log.claim(date(18), true, false);
        assert!(matches!(next, Some(Claim::Start(Some(day))) if day == date(17)));
        assert!(log.claim(date(18), true, false).is_none());
    }

    #[test]
    fn 남기지_못했으면_되돌려_다음_번에_다시_잡는다() {
        let mut log = DayLog::default();
        let Some(claim) = log.claim(date(17), true, false) else {
            return;
        };
        log.release(date(17), claim);
        assert_eq!(log.started, None);
        assert!(
            log.claim(date(17), true, false).is_some(),
            "다시 잡을 수 있다"
        );
        // 장이 열리지 않았으면 잡지 않고, 시작을 남기지 않은 날은 마감도 잡지 않는다.
        let mut idle = DayLog::default();
        assert!(idle.claim(date(17), false, false).is_none());
        assert!(idle.claim(date(17), false, true).is_none());
    }
}
