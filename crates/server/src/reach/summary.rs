//! 하루 정리(V12 · V50): 한 일 · 손익 · 복기 · 백업 시각을 한 묶음으로.

use std::sync::Arc;

use rust_decimal::Decimal;
use serde_json::{Value, json};

use super::linked::{Linked, clock_text};
use crate::{AppError, Crew, Trading};

/// 오늘(시장 달력의 날짜)의 하루 정리([`super::DAILY_SUMMARY_PATH`]의 모양).
pub(crate) async fn daily_summary(
    linked: &Linked,
    trading: &Arc<Trading>,
    crew: Option<&Arc<Crew>>,
) -> Result<Value, AppError> {
    let date = trading.date_of(linked.clock.now());
    let orders = trading.filled_on(date).await?;
    let pnl = trading.day_pnl().await?.normalize();
    let reviews = match crew {
        Some(crew) => crew.reviews_on(date)?,
        None => Vec::new(),
    };
    let backup_at = linked
        .backup
        .as_ref()
        .and_then(|_| linked.backup_status().ok())
        .and_then(|status| status.last_success);
    let backup = backup_at
        .map(|at| format!(" · 백업 완료 {}", clock_text(at)))
        .unwrap_or_default();
    let done = format!("체결 {orders}건 · 복기 {}건{backup}", reviews.len());
    let sign = if pnl > Decimal::ZERO { "+" } else { "" };
    let pnl_text = format!("오늘 손익 {sign}{pnl}원");
    let review = if reviews.is_empty() {
        "오늘 남긴 복기가 없습니다".to_string()
    } else {
        reviews
            .iter()
            .map(|r| format!("매매 {}: {}", r.trade.0, r.note))
            .collect::<Vec<_>>()
            .join("\n")
    };
    Ok(json!({
        "date": date.format("%Y-%m-%d").to_string(),
        "done": done,
        "pnl": pnl_text,
        "review": linked.secrets.redact(&review),
        "orders": orders,
        "pnl_krw": pnl.to_string(),
        "reviews": reviews.len(),
        "backup_at": backup_at.map(|at| at.0),
    }))
}
