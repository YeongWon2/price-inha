//! 가짜 어댑터. 다른 모듈의 테스트와 시연용이다.

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, MutexGuard, PoisonError};

use async_trait::async_trait;
use chrono::NaiveDate;
use rust_decimal::Decimal;

use crate::{
    Market, MarketAdapter, MarketCalendar, MarketError, Quote, Symbol, TradingDay, TradingStatus,
};

/// 정해 준 값들.
#[derive(Default)]
struct State {
    quotes: HashMap<Symbol, Quote>,
    calendar: Option<MarketCalendar>,
    quote_failure: Option<MarketError>,
    statuses: HashMap<Symbol, TradingStatus>,
    /// (V32) 종목마다 정한 날짜와 그날 거래량.
    volumes: HashMap<Symbol, (NaiveDate, Decimal)>,
}

/// 정해 준 시세와 달력을 돌려주는 어댑터. 바깥으로 요청을 보내지 않는다.
pub struct FakeAdapter {
    market: Market,
    state: Mutex<State>,
    quote_calls: AtomicUsize,
}

impl FakeAdapter {
    /// 시세도 달력도 정하지 않은 가짜 어댑터.
    pub fn new(market: Market) -> Self {
        Self {
            market,
            state: Mutex::new(State::default()),
            quote_calls: AtomicUsize::new(0),
        }
    }

    /// 종목의 시세를 정한다. 같은 종목을 다시 정하면 바꾼다.
    pub fn set_quote(&self, quote: Quote) {
        self.state().quotes.insert(quote.symbol.clone(), quote);
    }

    /// 달력을 정한다. `calendar(날짜)`는 날짜와 상관없이 이것을 돌려준다.
    /// 정하지 않았으면 그 날짜가 기준일이고 세 날 모두 세션이 없는(휴장) 달력을 돌려준다.
    pub fn set_calendar(&self, calendar: MarketCalendar) {
        self.state().calendar = Some(calendar);
    }

    /// `Some`이면 `quotes`가 그 오류로 실패한다. `None`이면 다시 성공한다.
    pub fn fail_quotes(&self, error: Option<MarketError>) {
        self.state().quote_failure = error;
    }

    /// 지금까지 `quotes`가 불린 횟수(실패한 것도 센다).
    pub fn quote_calls(&self) -> usize {
        self.quote_calls.load(Ordering::SeqCst)
    }

    /// 종목의 거래 상태를 정한다. 하나라도 정하면 `trading_statuses`가 `Some`(정한 종목만,
    /// 넘긴 순서)을 준다. 하나도 정하지 않았으면 `None`이다.
    pub fn set_status(&self, symbol: Symbol, status: TradingStatus) {
        self.state().statuses.insert(symbol, status);
    }

    /// (V32) 종목의 그날 누적 거래량을 정한다. `daily_volume`은 같은 날짜면 이 값, 그보다 뒤 날짜면 0(그날 아직
    /// 체결 없음), 앞 날짜이거나 정하지 않은 종목이면 `None`(모름)이다.
    pub fn set_volume(&self, symbol: Symbol, date: NaiveDate, volume: Decimal) {
        self.state().volumes.insert(symbol, (date, volume));
    }

    /// 잠금 안의 값은 한 번에 하나씩만 바꾸므로, 다른 스레드가 잠금을 쥔 채 죽었어도
    /// 값은 온전하다. 그래서 그대로 이어 쓴다.
    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

#[async_trait]
impl MarketAdapter for FakeAdapter {
    fn market(&self) -> Market {
        self.market
    }

    /// 달라는 순서대로 돌려준다. 정하지 않은 종목은 결과에서 뺀다.
    async fn quotes(&self, symbols: &[Symbol]) -> Result<Vec<Quote>, MarketError> {
        self.quote_calls.fetch_add(1, Ordering::SeqCst);
        let state = self.state();
        if let Some(error) = &state.quote_failure {
            return Err(error.clone());
        }
        Ok(symbols
            .iter()
            .filter_map(|symbol| state.quotes.get(symbol).cloned())
            .collect())
    }

    async fn trading_statuses(
        &self,
        symbols: &[Symbol],
    ) -> Result<Option<Vec<(Symbol, TradingStatus)>>, MarketError> {
        let state = self.state();
        if state.statuses.is_empty() {
            return Ok(None);
        }
        Ok(Some(
            symbols
                .iter()
                .filter_map(|symbol| {
                    state
                        .statuses
                        .get(symbol)
                        .map(|status| (symbol.clone(), *status))
                })
                .collect(),
        ))
    }

    async fn daily_volume(
        &self,
        symbol: &Symbol,
        date: NaiveDate,
    ) -> Result<Option<Decimal>, MarketError> {
        Ok(match self.state().volumes.get(symbol) {
            Some((day, volume)) if *day == date => Some(*volume),
            Some((day, _)) if *day < date => Some(Decimal::ZERO),
            _ => None,
        })
    }

    async fn calendar(&self, date: NaiveDate) -> Result<MarketCalendar, MarketError> {
        if let Some(calendar) = &self.state().calendar {
            return Ok(calendar.clone());
        }
        let closed = || TradingDay {
            date,
            sessions: Vec::new(),
        };
        Ok(MarketCalendar {
            market: self.market,
            today: closed(),
            previous_business_day: closed(),
            next_business_day: closed(),
        })
    }
}
