//! 시장 어댑터 trait (D5).

use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;

use crate::{Market, MarketCalendar, MarketError, Quote, Session, Symbol, TradingStatus};

/// 시장 하나의 시세와 장 시간. 주문은 여기에 없다.
#[async_trait]
pub trait MarketAdapter: Send + Sync {
    /// 이 어댑터가 맡은 시장.
    fn market(&self) -> Market;

    /// 종목들의 시세. 종목이 비어 있으면 요청 없이 빈 목록을 돌려준다.
    async fn quotes(&self, symbols: &[Symbol]) -> Result<Vec<Quote>, MarketError>;

    /// 그 날짜와 앞뒤 영업일의 세션들.
    async fn calendar(&self, date: NaiveDate) -> Result<MarketCalendar, MarketError>;

    /// 그 시각에 열려 있는 세션. 없으면 장 닫힘. 기본은 [`MarketCalendar::session_at`]과 같다.
    fn session_at(&self, calendar: &MarketCalendar, at: DateTime<Utc>) -> Option<Session> {
        calendar.session_at(at)
    }

    /// 공급자가 주는 종목들의 거래 상태(VI · 거래정지).
    ///
    /// `Ok(None)`은 "이 공급자는 상태를 주지 않는다"는 뜻이다. 기본은 요청을 보내지 않고
    /// `Ok(None)`이다. 토스는 공식 명세의 현재가 응답에 상태 칸이 없어 기본 그대로다.
    async fn trading_statuses(
        &self,
        symbols: &[Symbol],
    ) -> Result<Option<Vec<(Symbol, TradingStatus)>>, MarketError> {
        let _ = symbols;
        Ok(None)
    }

    /// (V32) 그 종목의 그날(`date`, 그 시장의 날짜) 누적 거래량.
    ///
    /// - `Ok(Some(수량))`: 그날 거래량. 그날 아직 체결이 없으면(공급자의 마지막 일봉이 그 날짜보다 앞) 0이다
    ///   (거래일이 바뀌면 0부터).
    /// - `Ok(None)`: 모른다(이 공급자는 거래량을 주지 않는다 · 일봉이 하나도 없다). 기본은 요청을 보내지 않고
    ///   `Ok(None)`이다.
    async fn daily_volume(
        &self,
        symbol: &Symbol,
        date: NaiveDate,
    ) -> Result<Option<Decimal>, MarketError> {
        let _ = (symbol, date);
        Ok(None)
    }
}
