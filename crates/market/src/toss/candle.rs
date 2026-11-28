//! (V32) 일봉 응답에서 그날 누적 거래량을 읽는다.
//!
//! 공식 명세(REST OpenAPI v1.2.31): `GET /api/v1/candles?symbol=&interval=1d&count=1`의
//! `result.candles`는 최신순이고, 봉마다 `timestamp`(일봉은 그 거래일의 현지 자정) · `volume`(십진수 글자)이 있다.

use chrono::{DateTime, NaiveDate};
use rust_decimal::Decimal;
use serde::Deserialize;

use crate::MarketError;

use super::http::unexpected;

#[derive(Deserialize)]
struct CandlesResponse {
    result: CandlePage,
}

#[derive(Deserialize)]
struct CandlePage {
    candles: Vec<Candle>,
}

#[derive(Deserialize)]
struct Candle {
    timestamp: String,
    /// 문자열 십진수만 받는다(숫자 타입이면 모양 오류).
    volume: String,
}

/// 맨 앞(가장 최근) 일봉으로 `date`의 누적 거래량을 정한다.
///
/// - 봉의 날짜(그 봉의 시차로 본 날짜)가 `date`면 그 `volume`.
/// - 봉의 날짜가 `date`보다 앞이면 그날 아직 체결이 없다: 0.
/// - 봉이 없거나 봉의 날짜가 `date`보다 뒤면 모른다(`None`).
/// - `volume`이 십진수가 아니거나 0보다 작으면 [`MarketError::UnexpectedResponse`](받은 값은 담지 않는다).
pub(super) fn parse_daily_volume(
    body: &str,
    date: NaiveDate,
) -> Result<Option<Decimal>, MarketError> {
    let page: CandlesResponse =
        serde_json::from_str(body).map_err(|_| unexpected("일봉 응답의 모양이 다르다"))?;
    let Some(latest) = page.result.candles.into_iter().next() else {
        return Ok(None);
    };
    let day = DateTime::parse_from_rfc3339(&latest.timestamp)
        .map_err(|_| unexpected("일봉 시각을 읽을 수 없다"))?
        .date_naive();
    let volume: Decimal = latest
        .volume
        .parse()
        .map_err(|_| unexpected("일봉 거래량을 읽을 수 없다"))?;
    if volume.is_sign_negative() && !volume.is_zero() {
        return Err(unexpected("일봉 거래량이 0보다 작다"));
    }
    Ok(match day.cmp(&date) {
        std::cmp::Ordering::Equal => Some(volume),
        std::cmp::Ordering::Less => Some(Decimal::ZERO),
        std::cmp::Ordering::Greater => None,
    })
}
