//! 주문 길의 바탕: 여는 일, 달력, 시세 기억, 통화별 장부(평가액 · 그날 시작 · 최고), 팀 상태 바꾸기,
//! 지금 쓰는 템플릿, 하루 기록 읽기.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError};
use std::time::Duration;

use chrono::{DateTime, NaiveDate, Utc};
use pi_approval::{Approvals, WaitTimes};
use pi_exec::Executor;
use pi_gate::Publisher;
use pi_market::{Clock, Currency, Market, MarketCalendar, Quote, StalePolicy};
use pi_paper::{
    Broker, InitialCash, LimitFillPrice, MarketRules, MaxFill, PaperConfig, ProceedsUsable,
    RoundingMode, UnverifiedRules,
};
use pi_rules::{Book, Position, RulesConfig};
use pi_store::{Store, StoreError, TeamCause, TeamChange, TeamState, TemplateState, UnixMillis};
use pi_template::RiskParams;
use rust_decimal::Decimal;
use serde_json::Value;

use super::marks::{KIND_LOSS_BASIS, Marks};
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
}

impl DayLog {
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
            self.basis = Some(payload.clone());
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
            days: Mutex::new(days),
            waiting: Mutex::new(HashMap::new()),
            team_turn: Mutex::new(()),
            publisher: OnceLock::new(),
        });
        let now = trading.clock.now();
        trading.books(now).await?;
        Ok(trading)
    }

    /// 접속 관문이 뜬 뒤 이벤트를 내보내는 손잡이를 붙인다.
    pub(crate) fn attach(&self, publisher: Publisher) {
        let _ = self.publisher.set(publisher);
    }

    /// 이벤트를 저장한다(접속 관문이 붙어 있으면 화면에도 보낸다).
    pub(super) async fn record(&self, kind: &str, payload: Value) -> Result<(), TradingError> {
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
    pub(super) fn date_of(&self, at: DateTime<Utc>) -> NaiveDate {
        crate::feed::market_date(self.market, at)
    }

    /// 오늘(시장 날짜)의 달력. 모르면 없음. 날짜가 바뀌면 모의계좌에도 넣는다.
    pub(super) fn calendar(&self, now: DateTime<Utc>) -> Option<MarketCalendar> {
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

    pub(super) fn last_quote(&self, symbol: &str) -> Option<Quote> {
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
        let paper = self.executor.broker();
        let holdings = Broker::holdings(paper, None, now)
            .await
            .map_err(exec_text)?;
        let quotes = lock(&self.quotes).clone();
        let today = self.date_of(now);
        let mut marks = lock(&self.marks);
        let mut changed = false;
        let mut books = Vec::with_capacity(CURRENCIES.len());
        for currency in &CURRENCIES {
            let cash = paper.cash(currency, now);
            let positions: Vec<Position> = holdings
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
        let payload = changed.then(|| marks.take_payload());
        drop(marks);
        if let Some(payload) = payload {
            self.save_basis(&payload)?;
        }
        Ok(books)
    }

    /// 손실 기준을 저장소에 남긴다(화면에는 보내지 않는다).
    fn save_basis(&self, payload: &Value) -> Result<(), TradingError> {
        self.store
            .append_at(KIND_LOSS_BASIS, payload, millis(self.clock.now()))?;
        Ok(())
    }

    /// 그날 시작 평가액을 지금 장부의 평가액으로 새로 잡는다(보유 종목이 있으면 바로 저장소에 남긴다).
    pub(super) fn reset_day_start(
        &self,
        books: &[Book],
        today: NaiveDate,
    ) -> Result<(), TradingError> {
        let payload = {
            let mut marks = lock(&self.marks);
            let now = marks.reset_day(books, today);
            now.then(|| marks.take_payload())
        };
        match payload {
            Some(payload) => self.save_basis(&payload),
            None => Ok(()),
        }
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

    /// 지금 쓰는 템플릿 값: 가장 나중에 고른 모의중 템플릿. 없거나 읽을 수 없으면 없음.
    pub(super) fn params(&self) -> Result<Option<RiskParams>, TradingError> {
        let chosen = self
            .store
            .templates_in_state(TemplateState::Simulating)?
            .into_iter()
            .max_by_key(|template| template.id);
        Ok(chosen.and_then(|template| {
            pi_template::Template::from_json(&template.value.to_string())
                .ok()
                .map(|parsed| parsed.params)
        }))
    }
}
