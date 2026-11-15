//! 응답 본문을 값으로 읽는다. 모양은 공식 명세(OpenAPI v1.2.28)를 따른다.

use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::Deserialize;

use crate::{
    Currency, Market, MarketCalendar, MarketError, Quote, Session, SessionKind, Symbol, TradingDay,
};

use super::http::unexpected;

#[derive(Deserialize)]
struct PricesResponse {
    result: Vec<PriceItem>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PriceItem {
    symbol: String,
    #[serde(default)]
    timestamp: Option<String>,
    last_price: String,
    currency: String,
}

/// 현재가 응답을 시세들로 읽는다. 받은 시각은 넘겨받은 `received_at`이다.
/// `wanted`가 거르는 종목(요청하지 않은 종목)은 버린다. 가격이 십진수로 읽히지 않으면
/// 어느 종목이든 응답 모양 오류지만, 0 이하 검사는 버리기 뒤에 해서 요청한 종목에만 한다.
pub(super) fn parse_quotes(
    body: &str,
    received_at: DateTime<Utc>,
    wanted: impl Fn(&str) -> bool,
) -> Result<Vec<Quote>, MarketError> {
    let parsed: PricesResponse = serde_json::from_str(body).map_err(|_| unexpected("현재가"))?;
    let mut quotes = Vec::with_capacity(parsed.result.len());
    for item in parsed.result {
        // 문자열에서 바로 십진수로 읽는다. 자리가 넘쳐 값이 바뀔 것 같으면 받지 않는다.
        let price = Decimal::from_str_exact(&item.last_price).map_err(|_| unexpected("현재가"))?;
        if !wanted(&item.symbol) {
            continue;
        }
        // 0 이하의 가격은 있을 수 없는 값이다(-0도 0이다).
        if price <= Decimal::ZERO {
            return Err(unexpected("현재가"));
        }
        let exchange_time = item
            .timestamp
            .as_deref()
            .map(|text| parse_time(text, "현재가"))
            .transpose()?;
        quotes.push(Quote {
            symbol: Symbol::new(item.symbol),
            price,
            currency: parse_currency(item.currency),
            exchange_time,
            received_at,
        });
    }
    Ok(quotes)
}

fn parse_currency(code: String) -> Currency {
    match code.as_str() {
        "KRW" => Currency::Krw,
        "USD" => Currency::Usd,
        _ => Currency::Other(code),
    }
}

fn parse_time(text: &str, what: &str) -> Result<DateTime<Utc>, MarketError> {
    DateTime::parse_from_rfc3339(text)
        .map(|time| time.with_timezone(&Utc))
        .map_err(|_| unexpected(what))
}

#[derive(Deserialize)]
struct CalendarResponse {
    result: CalendarResult,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CalendarResult {
    today: Day,
    previous_business_day: Day,
    next_business_day: Day,
}

/// 하루. 국내는 `integrated` 안에, 미국은 바로 아래에 세션이 있다.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Day {
    date: NaiveDate,
    #[serde(default)]
    integrated: Option<Windows>,
    #[serde(flatten)]
    direct: Windows,
}

/// 세션 칸들. 휴장이면 null이다.
#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct Windows {
    #[serde(default)]
    day_market: Option<Window>,
    #[serde(default)]
    pre_market: Option<Window>,
    #[serde(default)]
    regular_market: Option<Window>,
    #[serde(default)]
    after_market: Option<Window>,
}

/// 세션 하나의 시작과 끝. 국내의 단일가 구간 필드는 읽지 않는다(그 세션에 들어 있다).
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Window {
    start_time: String,
    end_time: String,
}

/// 장 시간 응답을 달력으로 읽는다.
pub(super) fn parse_calendar(body: &str, market: Market) -> Result<MarketCalendar, MarketError> {
    let parsed: CalendarResponse = serde_json::from_str(body).map_err(|_| unexpected("장 시간"))?;
    Ok(MarketCalendar {
        market,
        today: trading_day(parsed.result.today, market)?,
        previous_business_day: trading_day(parsed.result.previous_business_day, market)?,
        next_business_day: trading_day(parsed.result.next_business_day, market)?,
    })
}

fn trading_day(day: Day, market: Market) -> Result<TradingDay, MarketError> {
    let windows = match market {
        Market::Kr => day.integrated.unwrap_or_default(),
        Market::Us => day.direct,
    };
    let mut sessions = Vec::new();
    for (kind, window) in [
        (SessionKind::DayMarket, windows.day_market),
        (SessionKind::PreMarket, windows.pre_market),
        (SessionKind::Regular, windows.regular_market),
        (SessionKind::AfterMarket, windows.after_market),
    ] {
        let Some(window) = window else { continue };
        let start = parse_time(&window.start_time, "장 시간")?;
        let end = parse_time(&window.end_time, "장 시간")?;
        if end <= start {
            return Err(unexpected("장 시간"));
        }
        sessions.push(Session { kind, start, end });
    }
    sessions.sort_by_key(|session| session.start);
    Ok(TradingDay {
        date: day.date,
        sessions,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(text: &str) -> DateTime<Utc> {
        text.parse().unwrap()
    }

    #[test]
    fn 자리가_넘치는_가격은_반올림하지_않고_거절한다() {
        let body = r#"{"result":[{"symbol":"X","lastPrice":"0.12345678901234567890123456789012","currency":"USD"}]}"#;
        let err = parse_quotes(body, at("2026-03-25T01:00:00Z"), |_| true).unwrap_err();
        assert!(matches!(err, MarketError::UnexpectedResponse { .. }));
    }

    #[test]
    fn 가격이_숫자_타입이면_거절한다() {
        // 명세는 문자열이다. JSON 숫자는 부동소수점을 거칠 수 있어 받지 않는다.
        let body = r#"{"result":[{"symbol":"X","lastPrice":185.7,"currency":"USD"}]}"#;
        let err = parse_quotes(body, at("2026-03-25T01:00:00Z"), |_| true).unwrap_err();
        assert!(matches!(err, MarketError::UnexpectedResponse { .. }));
    }

    #[test]
    fn 응답_모양_오류에_받은_값이_담기지_않는다() {
        let body = r#"{"result":[{"symbol":"X","lastPrice":"비밀-글자","currency":"USD"}]}"#;
        let err = parse_quotes(body, at("2026-03-25T01:00:00Z"), |_| true).unwrap_err();
        assert!(!format!("{err} {err:?}").contains("비밀-글자"));
    }

    #[test]
    fn 국내_달력은_미국식_칸을_읽지_않는다() {
        // integrated가 null(휴장)인데 바로 아래에 세션 칸이 있어도 국내에서는 쓰지 않는다.
        let window =
            r#"{"startTime":"2026-03-25T09:00:00+09:00","endTime":"2026-03-25T15:30:00+09:00"}"#;
        let day = |date: &str| {
            format!(r#"{{"date":"{date}","integrated":null,"regularMarket":{window}}}"#)
        };
        let body = format!(
            r#"{{"result":{{"today":{},"previousBusinessDay":{},"nextBusinessDay":{}}}}}"#,
            day("2026-03-25"),
            day("2026-03-24"),
            day("2026-03-26")
        );
        let kr = parse_calendar(&body, Market::Kr).unwrap();
        assert!(kr.today.is_holiday());
        let us = parse_calendar(&body, Market::Us).unwrap();
        assert_eq!(us.today.sessions.len(), 1);
    }

    #[test]
    fn 끝이_시작보다_앞선_세션은_거절한다() {
        let window =
            r#"{"startTime":"2026-03-25T15:30:00+09:00","endTime":"2026-03-25T09:00:00+09:00"}"#;
        let day = format!(r#"{{"date":"2026-03-25","regularMarket":{window}}}"#);
        let body = format!(
            r#"{{"result":{{"today":{day},"previousBusinessDay":{day},"nextBusinessDay":{day}}}}}"#
        );
        let err = parse_calendar(&body, Market::Us).unwrap_err();
        assert!(matches!(err, MarketError::UnexpectedResponse { .. }));
    }
}
