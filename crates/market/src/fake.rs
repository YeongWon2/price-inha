//! 가짜 어댑터. 다른 모듈의 테스트와 시연용이다.

use async_trait::async_trait;
use chrono::NaiveDate;

use crate::{Market, MarketAdapter, MarketCalendar, MarketError, Quote, Symbol};

/// 정해 준 시세와 달력을 돌려주는 어댑터. 바깥으로 요청을 보내지 않는다.
pub struct FakeAdapter {
    _private: (),
}

impl FakeAdapter {
    /// 시세도 달력도 정하지 않은 가짜 어댑터.
    pub fn new(market: Market) -> Self {
        todo!()
    }

    /// 종목의 시세를 정한다. 같은 종목을 다시 정하면 바꾼다.
    pub fn set_quote(&self, quote: Quote) {
        todo!()
    }

    /// 달력을 정한다. `calendar(날짜)`는 날짜와 상관없이 이것을 돌려준다.
    /// 정하지 않았으면 그 날짜가 기준일이고 세 날 모두 세션이 없는(휴장) 달력을 돌려준다.
    pub fn set_calendar(&self, calendar: MarketCalendar) {
        todo!()
    }

    /// `Some`이면 `quotes`가 그 오류로 실패한다. `None`이면 다시 성공한다.
    pub fn fail_quotes(&self, error: Option<MarketError>) {
        todo!()
    }

    /// 지금까지 `quotes`가 불린 횟수(실패한 것도 센다).
    pub fn quote_calls(&self) -> usize {
        todo!()
    }
}

#[async_trait]
impl MarketAdapter for FakeAdapter {
    fn market(&self) -> Market {
        todo!()
    }

    /// 달라는 순서대로 돌려준다. 정하지 않은 종목은 결과에서 뺀다.
    async fn quotes(&self, symbols: &[Symbol]) -> Result<Vec<Quote>, MarketError> {
        todo!()
    }

    async fn calendar(&self, date: NaiveDate) -> Result<MarketCalendar, MarketError> {
        todo!()
    }
}
