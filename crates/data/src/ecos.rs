//! 한국은행 ECOS `StatisticSearch`. 키가 경로에 실리므로 보여 줄 주소에서는 그 조각을 가린다.
//!
//! 결과 코드: `INFO-200` 없음, `INFO-100` 키 무효, `ERROR-602` 과도한 호출,
//! `ERROR-500` · `ERROR-600` · `ERROR-601` 서버 쪽 오류(닿지 않음), 그 밖은 거절.

use std::str::FromStr;

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::Deserialize;
use serde_json::Value;

use crate::http::{
    Endpoint, MASK, Target, format_error, invalid, unreachable_error, with_segments,
};
use crate::{Clock, DataError, EcosSeries, Fact, FactKind, FactValue, Fetched, Source, SourceName};

const ORIGIN: SourceName = SourceName::Ecos;
/// 한 번에 받는 최대 행 수.
const END_ROW: &str = "10000";
/// [`END_ROW`]의 숫자.
const MAX_ROWS: u64 = 10_000;

/// 경로 조각(키 자리 포함)으로 보낼 주소와 가린 주소를 만든다. `key_at`은 키 조각의 자리.
fn path_target(endpoint: &Endpoint, segments: &[&str], key_at: usize, key: &str) -> Target {
    let mut real = segments.to_vec();
    let mut shown = segments.to_vec();
    if let (Some(real_slot), Some(shown_slot)) = (real.get_mut(key_at), shown.get_mut(key_at)) {
        *real_slot = key;
        *shown_slot = MASK;
    }
    Target {
        real: with_segments(&endpoint.base, &real),
        shown: with_segments(&endpoint.base, &shown).to_string(),
    }
}

/// 경로에 넣는 조각은 비어 있지 않은 ASCII 영숫자만 받는다(경로를 바꿀 수 없게).
fn check_segment(text: &str, what: &str) -> Result<(), DataError> {
    if !text.is_empty() && text.len() <= 32 && text.bytes().all(|b| b.is_ascii_alphanumeric()) {
        Ok(())
    } else {
        Err(invalid(&format!("ECOS {what}는 영숫자여야 합니다")))
    }
}

fn check_series(series: &EcosSeries) -> Result<(), DataError> {
    check_segment(&series.stat_code, "통계표 코드")?;
    check_segment(&series.item_code, "항목 코드")?;
    check_segment(&series.start, "시작 시점")?;
    check_segment(&series.end, "끝 시점")?;
    if series.start.len() == series.end.len() && series.start > series.end {
        return Err(invalid("ECOS 시작 시점이 끝 시점보다 늦습니다"));
    }
    Ok(())
}

/// `RESULT` 칸이 있으면 그 코드를 결과로 바꾼다. `Ok(())`면 없음(`INFO-200`).
fn check_result(result: &Value, target: &Target) -> Result<(), DataError> {
    let code = result
        .get("CODE")
        .and_then(Value::as_str)
        .ok_or_else(|| format_error(ORIGIN, "RESULT에 CODE가 없습니다"))?;
    match code {
        "INFO-200" => Ok(()),
        "INFO-100" => Err(DataError::KeyRejected {
            origin: ORIGIN,
            code: code.to_owned(),
        }),
        "ERROR-602" => Err(DataError::RateLimited { origin: ORIGIN }),
        "ERROR-500" | "ERROR-600" | "ERROR-601" => {
            Err(unreachable_error(ORIGIN, target, "ECOS 서버 오류"))
        }
        other => Err(DataError::Rejected {
            origin: ORIGIN,
            code: known_code(other),
        }),
    }
}

/// `INFO-000` · `ERROR-100` 같은 모양만 그대로 싣는다.
fn known_code(code: &str) -> String {
    let ok = code.len() <= 16
        && code.split_once('-').is_some_and(|(head, tail)| {
            matches!(head, "INFO" | "ERROR")
                && !tail.is_empty()
                && tail.bytes().all(|b| b.is_ascii_digit())
        });
    if ok {
        code.to_owned()
    } else {
        "알 수 없음".to_owned()
    }
}

#[derive(Deserialize)]
struct Row {
    #[serde(rename = "ITEM_NAME1", default)]
    item_name: Option<String>,
    #[serde(rename = "UNIT_NAME", default)]
    unit: Option<String>,
    #[serde(rename = "TIME")]
    time: String,
    #[serde(rename = "DATA_VALUE")]
    value: String,
}

/// 통계 하나(금리 · 환율).
pub(crate) async fn series(
    endpoint: &Endpoint,
    key: &str,
    clock: &dyn Clock,
    kind: FactKind,
    series: &EcosSeries,
) -> Result<Fetched, DataError> {
    check_series(series)?;
    let segments = [
        "api",
        "StatisticSearch",
        "",
        "json",
        "kr",
        "1",
        END_ROW,
        series.stat_code.as_str(),
        series.cycle.code(),
        series.start.as_str(),
        series.end.as_str(),
        series.item_code.as_str(),
    ];
    let target = path_target(endpoint, &segments, 2, key);
    let body: Value = serde_json::from_str(&endpoint.get(&target).await?)
        .map_err(|_| format_error(ORIGIN, "JSON이 아닙니다"))?;
    let known_at = clock.now();
    if let Some(result) = body.get("RESULT") {
        check_result(result, &target)?;
        return Ok(Fetched::Missing(kind));
    }
    let total = body
        .get("StatisticSearch")
        .and_then(|table| table.get("list_total_count"))
        .and_then(Value::as_u64);
    if total.is_some_and(|total| total > MAX_ROWS) {
        return Err(DataError::Truncated { origin: ORIGIN });
    }
    let rows = body
        .get("StatisticSearch")
        .and_then(|table| table.get("row"))
        .ok_or_else(|| format_error(ORIGIN, "StatisticSearch.row가 없습니다"))?;
    let rows: Vec<Row> = serde_json::from_value(rows.clone())
        .map_err(|_| format_error(ORIGIN, "행 모양이 다릅니다"))?;
    // 전체 행 수를 모르면(칸이 없거나 수가 아님) 받은 행이 상한과 같을 때 더 있을 수 있다고 본다.
    if total.is_none() && u64::try_from(rows.len()).is_ok_and(|received| received >= MAX_ROWS) {
        return Err(DataError::Truncated { origin: ORIGIN });
    }
    let subject = format!("{}/{}", series.stat_code, series.item_code);
    let facts = rows
        .into_iter()
        .map(|row| row_fact(row, kind, &subject, &target, known_at))
        .collect::<Result<Vec<_>, _>>()?;
    if facts.is_empty() {
        return Ok(Fetched::Missing(kind));
    }
    Ok(Fetched::Facts(facts))
}

fn row_fact(
    row: Row,
    kind: FactKind,
    subject: &str,
    target: &Target,
    known_at: DateTime<Utc>,
) -> Result<Fact, DataError> {
    let value = Decimal::from_str(row.value.trim())
        .map_err(|_| format_error(ORIGIN, "값이 수가 아닙니다"))?;
    Ok(Fact {
        kind,
        subject: subject.to_owned(),
        label: row.item_name.unwrap_or_else(|| subject.to_owned()),
        period: Some(row.time),
        value: FactValue::Number(value),
        unit: row.unit.filter(|text| !text.is_empty()),
        reference: None,
        source: Source {
            name: ORIGIN,
            url: target.shown.clone(),
        },
        published_on: None,
        known_at,
    })
}

/// 키 확인: 통계표 목록 한 행. `RESULT`가 키 무효면 키 거절, 그 밖의 정상 JSON이면 받아들인 것이다.
pub(crate) async fn probe(endpoint: &Endpoint, key: &str) -> Result<(), DataError> {
    let segments = ["api", "StatisticTableList", "", "json", "kr", "1", "1"];
    let target = path_target(endpoint, &segments, 2, key);
    let body: Value = serde_json::from_str(&endpoint.get(&target).await?)
        .map_err(|_| format_error(ORIGIN, "JSON이 아닙니다"))?;
    match body.get("RESULT") {
        Some(result) => check_result(result, &target),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 경로_조각은_영숫자만_받는다() {
        assert!(check_segment("722Y001", "통계표 코드").is_ok());
        for bad in ["", "../x", "a/b", "가나", "a b"] {
            assert!(check_segment(bad, "통계표 코드").is_err(), "{bad}");
        }
    }

    #[test]
    fn 결과_코드는_아는_모양만_싣는다() {
        assert_eq!(known_code("ERROR-100"), "ERROR-100");
        assert_eq!(known_code("INFO-000"), "INFO-000");
        assert_eq!(known_code("WHAT-1"), "알 수 없음");
        assert_eq!(known_code("ERROR-"), "알 수 없음");
    }
}
