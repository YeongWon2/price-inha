//! 하루 정리(V12 · V50 · 29차 V84): 한 일 · 손익 · 복기 · 백업 시각을 한 묶음으로.

use std::sync::Arc;

use chrono::NaiveDate;
use rust_decimal::Decimal;
use serde_json::{Value, json};

use super::linked::{Linked, clock_text};
use crate::{AppError, Crew, Trading};

/// 그 시장 날짜의 하루 정리([`super::DAILY_SUMMARY_PATH`]의 모양).
///
/// `date`가 없거나 시장 달력의 오늘이면 오늘의 정리다(날짜를 받기 전과 같은 글).
/// 다른 날짜면 그날의 체결 · 복기만 싣고, 그날 시작 평가액은 남아 있지 않으므로 `pnl_krw`는 `null`이다.
pub(crate) async fn daily_summary(
    linked: &Linked,
    trading: &Arc<Trading>,
    crew: Option<&Arc<Crew>>,
    date: Option<NaiveDate>,
) -> Result<Value, AppError> {
    let today = trading.date_of(linked.clock.now());
    let date = date.unwrap_or(today);
    let orders = trading.filled_on(date).await?;
    let reviews = match crew {
        Some(crew) => crew.reviews_on(date)?,
        None => Vec::new(),
    };
    let backup_at = linked
        .backup()
        .and_then(|_| linked.backup_status().ok())
        .and_then(|status| status.last_success);
    let notes = reviews
        .iter()
        .map(|r| format!("매매 {}: {}", r.trade.0, r.note))
        .collect::<Vec<_>>()
        .join("\n");
    let (done, pnl_text, review, pnl) = if date == today {
        let pnl = trading.day_pnl().await?.normalize();
        let backup = backup_at
            .map(|at| format!(" · 백업 완료 {}", clock_text(at)))
            .unwrap_or_default();
        let sign = if pnl > Decimal::ZERO { "+" } else { "" };
        (
            format!("체결 {orders}건 · 복기 {}건{backup}", reviews.len()),
            format!("오늘 손익 {sign}{pnl}원"),
            or_none(notes, "오늘 남긴 복기가 없습니다"),
            Value::String(pnl.to_string()),
        )
    } else {
        (
            format!("체결 {orders}건 · 복기 {}건", reviews.len()),
            "그날 손익 기록이 없습니다".to_string(),
            or_none(notes, "그날 남긴 복기가 없습니다"),
            Value::Null,
        )
    };
    Ok(json!({
        "date": date.format("%Y-%m-%d").to_string(),
        "done": done,
        "pnl": pnl_text,
        "review": linked.secrets.redact(&review),
        "orders": orders,
        "pnl_krw": pnl,
        "reviews": reviews.len(),
        "backup_at": backup_at.map(|at| at.0),
    }))
}

/// 글이 비었으면 대신 보일 글.
fn or_none(text: String, none: &str) -> String {
    if text.is_empty() {
        none.to_string()
    } else {
        text
    }
}

/// `YYYY-MM-DD` 모양의 있는 날짜만 읽는다(자리 수까지 맞아야 한다).
pub(crate) fn parse_date(text: &str) -> Option<NaiveDate> {
    let bytes = text.as_bytes();
    let shaped = bytes.len() == 10
        && bytes.iter().enumerate().all(|(index, byte)| match index {
            4 | 7 => *byte == b'-',
            _ => byte.is_ascii_digit(),
        });
    if !shaped {
        return None;
    }
    NaiveDate::parse_from_str(text, "%Y-%m-%d").ok()
}

#[cfg(test)]
mod tests {
    use super::parse_date;

    #[test]
    fn 날짜는_자리_수까지_맞는_있는_날짜만_읽는다() {
        assert!(parse_date("2026-11-16").is_some());
        for bad in [
            "2026-02-30",
            "2026/11/16",
            "20261116",
            "2026-1-16",
            "yesterday",
            "2026-13-01",
            "",
        ] {
            assert!(parse_date(bad).is_none(), "{bad}");
        }
    }
}
