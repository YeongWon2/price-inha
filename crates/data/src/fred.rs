//! FRED `fred/series/observations`. 키는 쿼리 `api_key`에 실린다.
//!
//! 값이 `.`인 관측은 FRED가 값이 없음을 나타내는 표시라 건너뛴다.

use std::str::FromStr;

use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::Deserialize;

use crate::http::{Endpoint, Target, format_error, invalid, query_target};
use crate::{Clock, DataError, Fact, FactKind, FactValue, Fetched, Source, SourceName};

const ORIGIN: SourceName = SourceName::Fred;
const KEY_PARAM: &str = "api_key";
/// FRED가 값이 없는 관측에 쓰는 글자.
const MISSING_VALUE: &str = ".";

#[derive(Deserialize)]
struct Body {
    observations: Vec<Observation>,
}

#[derive(Deserialize)]
struct Observation {
    #[serde(default)]
    realtime_start: Option<String>,
    date: String,
    value: String,
}

fn check_series_id(series_id: &str) -> Result<(), DataError> {
    let ok = !series_id.is_empty()
        && series_id.len() <= 64
        && series_id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'.');
    if ok {
        Ok(())
    } else {
        Err(invalid("FRED 계열 id는 영숫자여야 합니다"))
    }
}

fn day(date: NaiveDate) -> String {
    date.format("%Y-%m-%d").to_string()
}

/// 관측값(관측일 `from..=to`).
pub(crate) async fn observations(
    endpoint: &Endpoint,
    key: &str,
    clock: &dyn Clock,
    series_id: &str,
    from: NaiveDate,
    to: NaiveDate,
) -> Result<Fetched, DataError> {
    check_series_id(series_id)?;
    if from > to {
        return Err(invalid("시작 관측일이 끝 관측일보다 늦습니다"));
    }
    let (start, end) = (day(from), day(to));
    let params = [
        ("series_id", series_id),
        ("file_type", "json"),
        ("observation_start", start.as_str()),
        ("observation_end", end.as_str()),
    ];
    let segments = ["fred", "series", "observations"];
    let target = query_target(&endpoint.base, &segments, KEY_PARAM, key, &params);
    let text = endpoint.get(&target).await?;
    let known_at = clock.now();
    let body: Body = serde_json::from_str(&text)
        .map_err(|_| format_error(ORIGIN, "observations 모양이 다릅니다"))?;
    let mut facts = Vec::new();
    for observation in body.observations {
        if let Some(fact) = observation_fact(observation, series_id, &target, known_at)? {
            facts.push(fact);
        }
    }
    if facts.is_empty() {
        return Ok(Fetched::Missing(FactKind::UsRate));
    }
    Ok(Fetched::Facts(facts))
}

fn observation_fact(
    observation: Observation,
    series_id: &str,
    target: &Target,
    known_at: DateTime<Utc>,
) -> Result<Option<Fact>, DataError> {
    let text = observation.value.trim();
    if text == MISSING_VALUE {
        return Ok(None);
    }
    let value =
        Decimal::from_str(text).map_err(|_| format_error(ORIGIN, "관측값이 수가 아닙니다"))?;
    let published_on = match observation.realtime_start.as_deref() {
        Some(start) => Some(
            start
                .parse::<NaiveDate>()
                .map_err(|_| format_error(ORIGIN, "realtime_start가 날짜가 아닙니다"))?,
        ),
        None => None,
    };
    Ok(Some(Fact {
        kind: FactKind::UsRate,
        subject: series_id.to_owned(),
        label: series_id.to_owned(),
        period: Some(observation.date),
        value: FactValue::Number(value),
        unit: None,
        reference: None,
        source: Source {
            name: ORIGIN,
            url: target.shown.clone(),
        },
        published_on,
        known_at,
    }))
}

/// 키 확인: 잘 알려진 계열(`DGS10`)의 정보 한 번. 잘못된 키는 FRED가 HTTP 400으로 답한다.
pub(crate) async fn probe(endpoint: &Endpoint, key: &str) -> Result<(), DataError> {
    let params = [("series_id", "DGS10"), ("file_type", "json")];
    let target = query_target(&endpoint.base, &["fred", "series"], KEY_PARAM, key, &params);
    endpoint.get(&target).await.map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 계열_id는_비어_있지_않은_영숫자다() {
        assert!(check_series_id("DGS10").is_ok());
        assert!(check_series_id("BAMLH0A0HYM2").is_ok());
        assert!(check_series_id("").is_err());
        assert!(check_series_id("a&api_key=x").is_err());
    }
}
