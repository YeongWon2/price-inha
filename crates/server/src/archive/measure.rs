//! (61차) 쓰임 재기: 종류별 표 · 하루 평균 · 10년 추정 · 진단 한 줄 · `GET` [`SETUP_STORAGE_PATH`].

use axum::Router;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::Response;
use axum::routing::get;
use pi_store::{ArchiveCategory, StorageUsage, Store, UsageGroup};
use serde_json::{Value, json};

use super::{GroupReport, SETUP_STORAGE_PATH, StorageReport};
use crate::setup_http::reply;

/// 하루(밀리초).
const DAY_MS: i64 = 86_400_000;
/// 10년(일).
const TEN_YEARS_DAYS: u64 = 3650;

/// 묶음 하나의 하루 평균: 처음과 마지막 시각 사이의 날 수(양끝 포함, 적어도 1)로 나눈다. 시각이 없으면 0.
fn daily_of(group: &UsageGroup) -> u64 {
    match (group.first_at, group.last_at) {
        (Some(first), Some(last)) => {
            let days = (last.saturating_sub(first) / DAY_MS)
                .saturating_add(1)
                .max(1);
            group.bytes / u64::try_from(days).unwrap_or(1)
        }
        _ => 0,
    }
}

/// 저장소 쓰임으로 보고를 만든다.
pub fn storage_report(usage: &StorageUsage) -> StorageReport {
    let groups: Vec<GroupReport> = usage
        .groups
        .iter()
        .map(|group| GroupReport {
            table: group.table.clone(),
            kind: group.kind.clone(),
            rows: group.rows,
            bytes: group.bytes,
            daily_bytes: daily_of(group),
            archivable: group
                .kind
                .as_deref()
                .is_some_and(|kind| ArchiveCategory::covers(&group.table, kind)),
        })
        .collect();
    let daily_bytes = groups.iter().map(|g| g.daily_bytes).sum::<u64>();
    StorageReport {
        file_bytes: usage.file_bytes(),
        daily_bytes,
        ten_year_bytes: daily_bytes.saturating_mul(TEN_YEARS_DAYS),
        groups,
    }
}

/// 바이트를 그 단위의 소수 한 자리 글자로(정수 셈, 반올림).
fn tenths(bytes: u64, unit: u64) -> String {
    let scaled = (bytes.saturating_mul(10) + unit / 2) / unit;
    format!("{}.{}", scaled / 10, scaled % 10)
}

/// 진단 한 줄: "기록: N MB, 하루 약 X KB, 10년 약 Y GB".
pub fn storage_line(report: &StorageReport) -> String {
    format!(
        "기록: {} MB, 하루 약 {} KB, 10년 약 {} GB",
        tenths(report.file_bytes, 1 << 20),
        tenths(report.daily_bytes, 1 << 10),
        tenths(report.ten_year_bytes, 1 << 30)
    )
}

/// 진단 글(저장소를 잴 수 없으면 그 사실).
pub(crate) fn storage_detail(store: &Store) -> String {
    match store.storage_usage() {
        Ok(usage) => storage_line(&storage_report(&usage)),
        Err(_) => "기록: 잴 수 없음".to_owned(),
    }
}

fn report_json(report: &StorageReport) -> Value {
    let groups: Vec<Value> = report
        .groups
        .iter()
        .map(|g| {
            json!({
                "table": g.table,
                "kind": g.kind,
                "rows": g.rows,
                "bytes": g.bytes,
                "daily_bytes": g.daily_bytes,
                "archivable": g.archivable,
            })
        })
        .collect();
    json!({
        "file_bytes": report.file_bytes,
        "daily_bytes": report.daily_bytes,
        "ten_year_bytes": report.ten_year_bytes,
        "line": storage_line(report),
        "groups": groups,
    })
}

async fn storage(State(store): State<Store>) -> Response {
    let measured = tokio::task::spawn_blocking(move || store.storage_usage()).await;
    match measured {
        Ok(Ok(usage)) => reply(StatusCode::OK, report_json(&storage_report(&usage))),
        _ => reply(
            StatusCode::INTERNAL_SERVER_ERROR,
            json!({"error": "storage_unavailable"}),
        ),
    }
}

/// 세션 경로: `GET` [`SETUP_STORAGE_PATH`] — 종류별 쓰임 표.
pub(crate) fn storage_routes(store: Store) -> Router {
    Router::new()
        .route(SETUP_STORAGE_PATH, get(storage))
        .with_state(store)
}
