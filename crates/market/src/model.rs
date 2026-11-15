//! 시세 · 달력 값.

use chrono::{DateTime, Datelike, Duration, NaiveDate, Utc, Weekday};
use rust_decimal::Decimal;

use crate::MarketError;

/// 시장. 나중에 코인이 더해질 수 있어 `non_exhaustive`다.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Market {
    /// 국내 주식 (토스증권 `market-calendar/KR`).
    Kr,
    /// 미국 주식 (토스증권 `market-calendar/US`).
    Us,
}

/// 종목 심볼. 예: `005930`, `AAPL`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Symbol(String);

impl Symbol {
    /// 글자 그대로 심볼을 만든다(바꾸지 않는다).
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// 글자를 확인해 심볼을 만든다(바꾸지 않는다).
    ///
    /// 받는 글자: 영문(`A-Z` · `a-z`) · 숫자(`0-9`) · `.` · `-`(공식 명세의 종목 글자).
    /// 비었거나, 다른 글자(빈칸 · `,` · `&` · 한글 · 전각 글자 등)가 있거나, 32자를 넘으면
    /// [`MarketError::InvalidSymbol`]이다. 오류에는 받은 글자를 담지 않는다.
    /// 시세 어댑터([`crate::TossAdapter`])는 시세를 부르기 전에 모든 종목을 이 규칙으로 확인한다.
    pub fn parse(value: &str) -> Result<Self, MarketError> {
        let allowed = |c: char| c.is_ascii_alphanumeric() || matches!(c, '.' | '-');
        if value.is_empty() || value.len() > MAX_SYMBOL_LEN || !value.chars().all(allowed) {
            return Err(MarketError::InvalidSymbol);
        }
        Ok(Self(value.to_string()))
    }

    /// 심볼 글자.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// 통화. 공식 명세가 "모르는 값도 받아들이라"고 해서 `Other`가 있다.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Currency {
    /// 한국 원화.
    Krw,
    /// 미국 달러.
    Usd,
    /// 명세에 없는 통화 코드(받은 글자 그대로).
    Other(String),
}

/// 시세 하나.
#[derive(Debug, Clone, PartialEq)]
pub struct Quote {
    /// 종목.
    pub symbol: Symbol,
    /// 가격. 십진수다(부동소수점을 거치지 않는다).
    pub price: Decimal,
    /// 통화.
    pub currency: Currency,
    /// 거래소가 준 시각. 체결이 없으면 없다.
    pub exchange_time: Option<DateTime<Utc>>,
    /// 우리가 받은 시각([`crate::Clock::now`]). 늦은 시세(N6) 판단의 기준이다.
    pub received_at: DateTime<Utc>,
}

/// 세션 종류.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum SessionKind {
    /// 데이마켓 (미국만).
    DayMarket,
    /// 프리마켓.
    PreMarket,
    /// 정규장.
    Regular,
    /// 애프터마켓.
    AfterMarket,
}

/// 세션 하나. `start`는 포함하고 `end`는 포함하지 않는다(`start <= t < end`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    /// 종류.
    pub kind: SessionKind,
    /// 시작 시각(포함).
    pub start: DateTime<Utc>,
    /// 종료 시각(제외).
    pub end: DateTime<Utc>,
}

/// 영업일 하나와 그날의 세션들.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TradingDay {
    /// 날짜. 국내는 KST 기준, 미국은 미국 현지 기준(공식 명세 그대로).
    pub date: NaiveDate,
    /// 그날 열리는 세션들(시작 시각 순). 휴장이면 비어 있다.
    pub sessions: Vec<Session>,
}

impl TradingDay {
    /// 세션이 하나도 없으면(휴장) 참.
    pub fn is_holiday(&self) -> bool {
        self.sessions.is_empty()
    }

    /// 그 종류의 세션. 없으면(부분 휴장 포함) `None`.
    pub fn session(&self, kind: SessionKind) -> Option<&Session> {
        self.sessions.iter().find(|session| session.kind == kind)
    }
}

/// 기준일과 앞뒤 영업일의 세션들.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarketCalendar {
    /// 어느 시장의 달력인가.
    pub market: Market,
    /// 기준일.
    pub today: TradingDay,
    /// 이전 영업일.
    pub previous_business_day: TradingDay,
    /// 다음 영업일.
    pub next_business_day: TradingDay,
}

impl MarketCalendar {
    /// 그 시각에 열려 있는 세션. 없으면 장 닫힘(`None`).
    ///
    /// 규칙:
    /// - 세 날(이전 영업일 · 기준일 · 다음 영업일)의 세션을 모두 본다. 미국 정규장처럼
    ///   날짜를 넘는 세션도 그래서 찾는다.
    /// - 시작 시각은 포함하고 종료 시각은 제외한다(`start <= at < end`). 프리마켓이
    ///   09:00에 끝나고 정규장이 09:00에 시작하면 09:00은 정규장이다.
    /// - 국내 단일가 구간(`singlePriceAuction*`)은 따로 나누지 않고 그 세션에 넣는다.
    pub fn session_at(&self, at: DateTime<Utc>) -> Option<Session> {
        [
            &self.previous_business_day,
            &self.today,
            &self.next_business_day,
        ]
        .into_iter()
        .flat_map(|day| day.sessions.iter())
        .find(|session| session.start <= at && at < session.end)
        .cloned()
    }

    /// 이 달력이 그 시각을 덮는가(N6). 덮지 않으면 "장 닫힘"인지 알 수 없다(달력이 낡음).
    ///
    /// 덮는 범위는 이전 영업일의 시작(포함)부터 다음 영업일의 끝(제외)까지다.
    /// - 날의 시작은 그 시장 날짜의 자정과 그날 첫 세션의 시작 중 이른 쪽,
    ///   날의 끝은 그 다음 날 자정과 그날 마지막 세션의 끝 중 늦은 쪽이다
    ///   (미국 데이마켓 · 애프터마켓처럼 시장 날짜 밖으로 나가는 세션도 덮는다).
    /// - 시장 날짜: 국내는 KST(UTC+9), 미국은 미 동부 시각.
    /// - 그 사이의 휴장일 · 주말은 덮는다(달력이 영업일이 아니라고 알려 준 날이다).
    pub fn covers(&self, at: DateTime<Utc>) -> bool {
        let start = day_start(self.market, &self.previous_business_day);
        let end = day_end(self.market, &self.next_business_day);
        match (start, end) {
            (Some(start), Some(end)) => start <= at && at < end,
            // 날짜를 시각으로 바꾸지 못하면(범위 밖) 덮는지 모른다 → 덮지 않는다.
            _ => false,
        }
    }
}

/// 종목 글자의 가장 긴 길이.
const MAX_SYMBOL_LEN: usize = 32;

/// 날의 시작: 그 시장 날짜의 자정과 첫 세션의 시작 중 이른 쪽.
fn day_start(market: Market, day: &TradingDay) -> Option<DateTime<Utc>> {
    let midnight = market_midnight(market, day.date)?;
    let first = day.sessions.iter().map(|session| session.start).min();
    Some(first.map_or(midnight, |first| first.min(midnight)))
}

/// 날의 끝: 그 다음 날 자정과 마지막 세션의 끝 중 늦은 쪽.
fn day_end(market: Market, day: &TradingDay) -> Option<DateTime<Utc>> {
    let midnight = market_midnight(market, day.date.succ_opt()?)?;
    let last = day.sessions.iter().map(|session| session.end).max();
    Some(last.map_or(midnight, |last| last.max(midnight)))
}

/// 그 시장 날짜의 자정(UTC).
fn market_midnight(market: Market, date: NaiveDate) -> Option<DateTime<Utc>> {
    let offset_hours = match market {
        Market::Kr => 9,
        Market::Us => {
            if us_eastern_dst_at_midnight(date) {
                -4
            } else {
                -5
            }
        }
    };
    let local = date.and_hms_opt(0, 0, 0)?;
    let utc = local.checked_sub_signed(Duration::try_hours(offset_hours)?)?;
    Some(utc.and_utc())
}

/// 미 동부의 그날 자정이 서머타임인가.
///
/// 서머타임은 3월 둘째 일요일 02:00부터 11월 첫째 일요일 02:00까지다. 바뀌는 날의 자정은
/// 아직 바뀌기 전이므로, 3월 둘째 일요일 다음 날부터 11월 첫째 일요일까지의 자정이 서머타임이다.
fn us_eastern_dst_at_midnight(date: NaiveDate) -> bool {
    let year = date.year();
    let (Some(begin), Some(end)) = (
        NaiveDate::from_weekday_of_month_opt(year, 3, Weekday::Sun, 2),
        NaiveDate::from_weekday_of_month_opt(year, 11, Weekday::Sun, 1),
    ) else {
        return false;
    };
    begin < date && date <= end
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date(text: &str) -> NaiveDate {
        text.parse().unwrap()
    }

    #[test]
    fn 미_동부_자정의_서머타임은_바뀌는_다음_날부터다() {
        // 2026년: 3월 8일 시작, 11월 1일 끝.
        assert!(!us_eastern_dst_at_midnight(date("2026-03-08")));
        assert!(us_eastern_dst_at_midnight(date("2026-03-09")));
        assert!(us_eastern_dst_at_midnight(date("2026-11-01")));
        assert!(!us_eastern_dst_at_midnight(date("2026-11-02")));
        assert!(!us_eastern_dst_at_midnight(date("2026-01-15")));
    }

    #[test]
    fn 시장_날짜의_자정은_시장_시각대로_센다() {
        let at = |text: &str| text.parse::<DateTime<Utc>>().unwrap();
        assert_eq!(
            market_midnight(Market::Kr, date("2026-03-25")),
            Some(at("2026-03-24T15:00:00Z"))
        );
        assert_eq!(
            market_midnight(Market::Us, date("2026-03-25")),
            Some(at("2026-03-25T04:00:00Z"))
        );
        assert_eq!(
            market_midnight(Market::Us, date("2026-01-15")),
            Some(at("2026-01-15T05:00:00Z"))
        );
    }
}
