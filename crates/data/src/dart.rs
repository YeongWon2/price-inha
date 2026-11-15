//! DART 오픈API: 공시검색(`/api/list.json`)과 단일회사 주요계정(`/api/fnlttSinglAcnt.json`).
//!
//! HTTP는 늘 200이고 본문의 `status`로 결과를 가른다:
//! `000` 정상, `013` 없음, `010` · `011` · `012` · `901` 키 문제, `020` 한도, `800` 점검, 그 밖은 거절.

use std::str::FromStr;

use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::Deserialize;
use serde_json::Value;

use crate::http::{Endpoint, Target, format_error, invalid, query_target, unreachable_error};
use crate::{
    Clock, CorpCode, DataError, Fact, FactKind, FactValue, Fetched, FsDiv, ReportCode, Source,
    SourceName,
};

const ORIGIN: SourceName = SourceName::Dart;
const KEY_PARAM: &str = "crtfc_key";
/// 공시검색 한 쪽의 건수(명세의 최대 100).
const PAGE_COUNT: &str = "100";
/// 공시검색에서 따라가는 최대 쪽 수.
const MAX_PAGES: u32 = 20;

/// 본문의 `status` 글을 결과로 바꾼다. `Ok(true)` 정상, `Ok(false)` 없음(`013`).
fn check_status(body: &Value, target: &Target) -> Result<bool, DataError> {
    let status = body
        .get("status")
        .and_then(Value::as_str)
        .ok_or_else(|| format_error(ORIGIN, "status 칸이 없습니다"))?;
    match status {
        "000" => Ok(true),
        "013" => Ok(false),
        "010" | "011" | "012" | "901" => Err(DataError::KeyRejected {
            origin: ORIGIN,
            code: status.to_owned(),
        }),
        "020" => Err(DataError::RateLimited { origin: ORIGIN }),
        "800" => Err(unreachable_error(ORIGIN, target, "시스템 점검(800)")),
        other => Err(DataError::Rejected {
            origin: ORIGIN,
            code: short_code(other),
        }),
    }
}

/// 출처 코드는 짧은 영숫자만 싣는다(모르는 글을 그대로 옮기지 않는다).
fn short_code(code: &str) -> String {
    if !code.is_empty() && code.len() <= 16 && code.bytes().all(|b| b.is_ascii_alphanumeric()) {
        code.to_owned()
    } else {
        "알 수 없음".to_owned()
    }
}

fn parse_json(text: &str) -> Result<Value, DataError> {
    serde_json::from_str(text).map_err(|_| format_error(ORIGIN, "JSON이 아닙니다"))
}

/// 보내고 status를 본다. 없음(`013`)이면 `None`.
async fn get_checked(endpoint: &Endpoint, target: &Target) -> Result<Option<Value>, DataError> {
    let body = parse_json(&endpoint.get(target).await?)?;
    Ok(check_status(&body, target)?.then_some(body))
}

#[derive(Deserialize)]
struct ListPage {
    #[serde(default)]
    list: Vec<ListItem>,
    #[serde(default)]
    total_page: Option<u32>,
}

#[derive(Deserialize)]
struct ListItem {
    corp_name: String,
    report_nm: String,
    rcept_no: String,
    rcept_dt: String,
}

/// 공시 목록(접수일 `from..=to`).
pub(crate) async fn disclosures(
    endpoint: &Endpoint,
    key: &str,
    clock: &dyn Clock,
    corp: &CorpCode,
    from: NaiveDate,
    to: NaiveDate,
) -> Result<Fetched, DataError> {
    if from > to {
        return Err(invalid("시작 접수일이 끝 접수일보다 늦습니다"));
    }
    let (bgn, end) = (yyyymmdd(from), yyyymmdd(to));
    let mut items = Vec::new();
    let mut first_target = None;
    let mut page = 1;
    loop {
        let page_no = page.to_string();
        let params = [
            ("corp_code", corp.as_str()),
            ("bgn_de", bgn.as_str()),
            ("end_de", end.as_str()),
            ("page_no", page_no.as_str()),
            ("page_count", PAGE_COUNT),
        ];
        let target = query_target(
            &endpoint.base,
            &["api", "list.json"],
            KEY_PARAM,
            key,
            &params,
        );
        let Some(body) = get_checked(endpoint, &target).await? else {
            break;
        };
        let parsed: ListPage = serde_json::from_value(body)
            .map_err(|_| format_error(ORIGIN, "공시 목록 모양이 다릅니다"))?;
        items.extend(parsed.list);
        first_target.get_or_insert(target);
        let total_page = parsed.total_page.unwrap_or(1);
        if page >= total_page.min(MAX_PAGES) {
            if total_page > MAX_PAGES {
                return Err(DataError::Truncated { origin: ORIGIN });
            }
            break;
        }
        page += 1;
    }
    let known_at = clock.now();
    let (Some(target), false) = (first_target, items.is_empty()) else {
        return Ok(Fetched::Missing(FactKind::Disclosure));
    };
    let facts = items
        .into_iter()
        .map(|item| disclosure_fact(item, corp, &target, known_at))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Fetched::Facts(facts))
}

fn disclosure_fact(
    item: ListItem,
    corp: &CorpCode,
    target: &Target,
    known_at: DateTime<Utc>,
) -> Result<Fact, DataError> {
    let received = NaiveDate::parse_from_str(&item.rcept_dt, "%Y%m%d")
        .map_err(|_| format_error(ORIGIN, "접수일자가 날짜가 아닙니다"))?;
    Ok(Fact {
        kind: FactKind::Disclosure,
        subject: corp.as_str().to_owned(),
        label: item.corp_name,
        period: None,
        value: FactValue::Text(item.report_nm),
        unit: None,
        reference: Some(item.rcept_no),
        source: source(target),
        published_on: Some(received),
        known_at,
    })
}

#[derive(Deserialize)]
struct FinBody {
    #[serde(default)]
    list: Vec<FinLine>,
}

#[derive(Deserialize)]
struct FinLine {
    rcept_no: String,
    fs_div: String,
    account_nm: String,
    #[serde(default)]
    thstrm_dt: Option<String>,
    #[serde(default)]
    thstrm_amount: Option<String>,
    #[serde(default)]
    currency: Option<String>,
}

/// 단일회사 주요계정. 고른 재무제표 구분(`fs_div`)의 계정만 돌려준다.
pub(crate) async fn financials(
    endpoint: &Endpoint,
    key: &str,
    clock: &dyn Clock,
    corp: &CorpCode,
    year: u16,
    report: ReportCode,
    fs: FsDiv,
) -> Result<Fetched, DataError> {
    if !(1000..=9999).contains(&year) {
        return Err(invalid("사업연도는 네 자리여야 합니다"));
    }
    let year = year.to_string();
    let params = [
        ("corp_code", corp.as_str()),
        ("bsns_year", year.as_str()),
        ("reprt_code", report.code()),
    ];
    let segments = ["api", "fnlttSinglAcnt.json"];
    let target = query_target(&endpoint.base, &segments, KEY_PARAM, key, &params);
    let Some(body) = get_checked(endpoint, &target).await? else {
        return Ok(Fetched::Missing(FactKind::Financials));
    };
    let known_at = clock.now();
    let parsed: FinBody = serde_json::from_value(body)
        .map_err(|_| format_error(ORIGIN, "주요계정 모양이 다릅니다"))?;
    let mut facts = Vec::new();
    for line in parsed
        .list
        .into_iter()
        .filter(|line| line.fs_div == fs.code())
    {
        if let Some(fact) = financial_fact(line, corp, &target, known_at)? {
            facts.push(fact);
        }
    }
    if facts.is_empty() {
        return Ok(Fetched::Missing(FactKind::Financials));
    }
    Ok(Fetched::Facts(facts))
}

/// 계정 한 줄. 금액 칸이 비었거나 `-`(값 없음)이면 건너뛴다.
fn financial_fact(
    line: FinLine,
    corp: &CorpCode,
    target: &Target,
    known_at: DateTime<Utc>,
) -> Result<Option<Fact>, DataError> {
    let Some(amount) = line.thstrm_amount.as_deref().map(str::trim) else {
        return Ok(None);
    };
    if amount.is_empty() || amount == "-" {
        return Ok(None);
    }
    let value = Decimal::from_str(&amount.replace(',', ""))
        .map_err(|_| format_error(ORIGIN, "금액이 수가 아닙니다"))?;
    Ok(Some(Fact {
        kind: FactKind::Financials,
        subject: corp.as_str().to_owned(),
        label: line.account_nm,
        period: line.thstrm_dt.filter(|text| !text.is_empty()),
        value: FactValue::Number(value),
        unit: line.currency.filter(|text| !text.is_empty()),
        published_on: receipt_date(&line.rcept_no),
        reference: Some(line.rcept_no),
        source: source(target),
        known_at,
    }))
}

/// 접수번호(14자리) 앞 8자리는 접수일자다. 모양이 다르면 없음.
fn receipt_date(rcept_no: &str) -> Option<NaiveDate> {
    let head = rcept_no.get(..8)?;
    NaiveDate::parse_from_str(head, "%Y%m%d").ok()
}

fn yyyymmdd(day: NaiveDate) -> String {
    day.format("%Y%m%d").to_string()
}

fn source(target: &Target) -> Source {
    Source {
        name: ORIGIN,
        url: target.shown.clone(),
    }
}

/// 키 확인: 회사를 정하지 않은 공시검색 한 쪽(1건). `000` · `013`이면 키를 받아들인 것이다.
pub(crate) async fn probe(endpoint: &Endpoint, key: &str) -> Result<(), DataError> {
    let params = [("page_count", "1")];
    let target = query_target(
        &endpoint.base,
        &["api", "list.json"],
        KEY_PARAM,
        key,
        &params,
    );
    get_checked(endpoint, &target).await.map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 접수번호_앞_여덟_자리가_접수일자다() {
        assert_eq!(
            receipt_date("20260310000789"),
            NaiveDate::from_ymd_opt(2026, 3, 10)
        );
        assert_eq!(receipt_date("2026"), None);
        assert_eq!(receipt_date("가나다라마바사아자차"), None);
    }

    #[test]
    fn 모르는_출처_코드는_그대로_옮기지_않는다() {
        assert_eq!(short_code("900"), "900");
        assert_eq!(short_code("<b>긴 글</b>"), "알 수 없음");
        assert_eq!(short_code(""), "알 수 없음");
    }
}
