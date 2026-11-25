//! (23번, C25 · R7-4 · M11 · E54 · V70) 결정 메모(어제 메모)를 읽는 경로.
//!
//! 폰 경로와 같은 상태([`Phone`])로 세션 관문 안에 끼운다. 두 모양이 있다:
//!
//! - `?days=N`(없으면 [`MEMOS_DEFAULT_DAYS`]): 어제 메모 카드가 읽는 배열(C25). 결정 날이나 정산된 날(시장 날짜)이
//!   오늘에서 N일 전 이후인 메모, 결정 날이 늦은 것(같으면 번호가 큰 것)부터.
//! - `?before=&max=`: 히스토리 탭이 읽는 쪽 `{"memos": [...], "next"}`. 매매 목록([`super::TRADES_PATH`])과 같은
//!   쪽 넘기기이고 커서는 메모 번호 `id`다(번호가 큰 것부터).
//!
//! 줄 하나: `{"id", "trade", "date", "symbol", "decision": "buy"|"sell", "state": "pending"|"settled"|"unsettled",
//! "reason"(그때 가설, 가린 것)?, "return_pct"?, "excess_pct"?, "hold_days"?, "lesson"?, "verdict"?,
//! "unsettled_reason"?, "settled_at"?}`. 결과 판정(`verdict`)은 여기서 한다: 정산됨이면 초과수익(없으면 수익률)이
//! 0보다 크면 `right`, 작으면 `wrong`, 0이면 `unknown`, 정산 못 함은 `unknown`(까닭 `no_price`, E54). 정산 대기는 없다.

use std::sync::Arc;

use axum::Router;
use axum::extract::{RawQuery, State};
use axum::http::StatusCode;
use axum::response::Response;
use axum::routing::get;
use chrono::{Days, NaiveDate, TimeZone, Utc};
use pi_store::{
    DecisionMemo, DecisionMemoId, DecisionMemoState, HISTORY_PAGE_MAX, StoreError, TradeSide,
};
use serde_json::{Map, Value, json};

use super::HISTORY_DEFAULT_MAX;
use super::office_http::reply;
use super::phone_http::{Phone, internal_error, refuse};

/// 결정 메모 경로(C25).
pub const MEMOS_PATH: &str = "/api/memos";
/// `?days=`가 없을 때의 날수(C25 `?days=7`).
pub const MEMOS_DEFAULT_DAYS: u32 = 7;
/// `?days=`의 가장 큰 값.
pub const MEMOS_MAX_DAYS: u32 = 365;
/// 정산이 끝났을 때의 사무실 이벤트(C25): `{"trade", "state": "settled"|"unsettled"}`.
pub const KIND_OFFICE_MEMO: &str = "office.memo";

/// 읽을 모양.
#[derive(Debug, PartialEq, Eq)]
enum Ask {
    /// 최근 N일(배열).
    Days(u32),
    /// 쪽(히스토리).
    Page { before: Option<i64>, max: usize },
}

/// `?days=` 또는 `?before=&max=`를 읽는다. 틀린 값이거나 둘을 섞으면 `None`(400). 같은 이름이 여럿이면 처음 것.
fn ask_of(query: Option<&str>) -> Option<Ask> {
    let (mut days, mut before, mut max) = (None, None, None);
    for (key, value) in url::form_urlencoded::parse(query.unwrap_or_default().as_bytes()) {
        let number = || value.parse::<i64>().ok().filter(|n| *n >= 1);
        match key.as_ref() {
            "days" if days.is_none() => days = Some(number()?),
            "before" if before.is_none() => before = Some(number()?),
            "max" if max.is_none() => max = Some(value.parse::<u64>().ok().filter(|n| *n >= 1)?),
            _ => {}
        }
    }
    if before.is_none() && max.is_none() {
        let days = u32::try_from(days.unwrap_or(i64::from(MEMOS_DEFAULT_DAYS))).ok()?;
        return (days <= MEMOS_MAX_DAYS).then_some(Ask::Days(days));
    }
    if days.is_some() {
        return None;
    }
    let max = max.map_or(HISTORY_DEFAULT_MAX, |n| {
        usize::try_from(n).map_or(HISTORY_PAGE_MAX, |n| n.min(HISTORY_PAGE_MAX))
    });
    Some(Ask::Page { before, max })
}

/// 결과 판정 글자. 정산 대기는 없음.
fn verdict(memo: &DecisionMemo) -> Option<&'static str> {
    match memo.state {
        DecisionMemoState::Pending => None,
        DecisionMemoState::Unsettleable => Some("unknown"),
        DecisionMemoState::Settled => {
            let basis = memo.excess_bp.or(memo.return_bp).unwrap_or_default();
            Some(match basis.signum() {
                1 => "right",
                -1 => "wrong",
                _ => "unknown",
            })
        }
    }
}

fn state_word(state: DecisionMemoState) -> &'static str {
    match state {
        DecisionMemoState::Pending => "pending",
        DecisionMemoState::Settled => "settled",
        DecisionMemoState::Unsettleable => "unsettled",
    }
}

/// 만분율 → 백분율 수(만분율은 f64가 정확히 담는 범위다).
fn percent(bp: i64) -> Value {
    json!(bp as f64 / 100.0)
}

/// 줄 하나(없는 칸은 뺀다).
fn memo_item(phone: &Phone, memo: &DecisionMemo) -> Value {
    let mut item = Map::new();
    item.insert("id".into(), json!(memo.id.0));
    item.insert("trade".into(), json!(memo.trade.0));
    item.insert("date".into(), json!(memo.date));
    item.insert("symbol".into(), json!(memo.symbol));
    let decision = match memo.side {
        TradeSide::Buy => "buy",
        TradeSide::Sell => "sell",
    };
    item.insert("decision".into(), json!(decision));
    item.insert("state".into(), json!(state_word(memo.state)));
    let reason = phone
        .store
        .get_trade(memo.trade)
        .ok()
        .and_then(|trade| trade.rationale.hypothesis);
    let optional = [
        (
            "reason",
            reason.map(|text| json!(phone.shown_reason(&text))),
        ),
        ("return_pct", memo.return_bp.map(percent)),
        ("excess_pct", memo.excess_bp.map(percent)),
        ("hold_days", memo.holding_days.map(|days| json!(days))),
        ("lesson", memo.lesson.as_ref().map(|text| json!(text))),
        ("verdict", verdict(memo).map(|word| json!(word))),
        ("settled_at", memo.settled_at.map(|at| json!(at.0))),
    ];
    for (name, value) in optional {
        if let Some(value) = value {
            item.insert(name.into(), value);
        }
    }
    if memo.state == DecisionMemoState::Unsettleable {
        item.insert("unsettled_reason".into(), json!("no_price"));
    }
    Value::Object(item)
}

/// 결정 날이나 정산된 날이 `cutoff` 이후(같은 날 포함)인가.
fn recent(memo: &DecisionMemo, cutoff: NaiveDate, settled_day: Option<NaiveDate>) -> bool {
    let decided = NaiveDate::parse_from_str(&memo.date, "%Y-%m-%d").is_ok_and(|day| day >= cutoff);
    decided || settled_day.is_some_and(|day| day >= cutoff)
}

/// 모든 메모(번호가 큰 것부터).
fn all_memos(phone: &Phone) -> Result<Vec<DecisionMemo>, StoreError> {
    let mut out: Vec<DecisionMemo> = Vec::new();
    loop {
        let before = out.last().map(|memo| memo.id);
        let page = phone.store.decision_memos_page(before, HISTORY_PAGE_MAX)?;
        if page.is_empty() {
            return Ok(out);
        }
        out.extend(page);
    }
}

fn recent_list(phone: &Phone, days: u32) -> Result<Value, StoreError> {
    let trading = &phone.trading;
    let today = trading.date_of(trading.now());
    let cutoff = today
        .checked_sub_days(Days::new(u64::from(days)))
        .unwrap_or(NaiveDate::MIN);
    let settled_day = |memo: &DecisionMemo| {
        let at = Utc.timestamp_millis_opt(memo.settled_at?.0).single()?;
        Some(trading.date_of(at))
    };
    let mut list: Vec<DecisionMemo> = all_memos(phone)?
        .into_iter()
        .filter(|memo| recent(memo, cutoff, settled_day(memo)))
        .collect();
    list.sort_by(|a, b| b.date.cmp(&a.date).then(b.id.0.cmp(&a.id.0)));
    Ok(Value::Array(
        list.iter().map(|memo| memo_item(phone, memo)).collect(),
    ))
}

fn page(phone: &Phone, before: Option<i64>, max: usize) -> Result<Value, StoreError> {
    let store = &phone.store;
    let rows = store.decision_memos_page(before.map(DecisionMemoId), max)?;
    let next = match rows.last() {
        Some(last) if rows.len() >= max => {
            let more = !store.decision_memos_page(Some(last.id), 1)?.is_empty();
            more.then_some(last.id.0)
        }
        _ => None,
    };
    let memos: Vec<Value> = rows.iter().map(|memo| memo_item(phone, memo)).collect();
    Ok(json!({ "memos": memos, "next": next }))
}

async fn memos(State(phone): State<Arc<Phone>>, RawQuery(query): RawQuery) -> Response {
    let read = match ask_of(query.as_deref()) {
        None => return refuse(StatusCode::BAD_REQUEST, "invalid_input"),
        Some(Ask::Days(days)) => recent_list(&phone, days),
        Some(Ask::Page { before, max }) => page(&phone, before, max),
    };
    match read {
        Ok(body) => reply(StatusCode::OK, body),
        Err(error) => internal_error(error),
    }
}

/// 폰 경로에 합친다(같은 상태, 세션 관문 안).
pub(super) fn routes() -> Router<Arc<Phone>> {
    Router::new().route(MEMOS_PATH, get(memos))
}

#[cfg(test)]
mod tests {
    use super::{Ask, MEMOS_DEFAULT_DAYS, ask_of, recent};
    use crate::HISTORY_DEFAULT_MAX;
    use chrono::NaiveDate;
    use pi_store::{
        DecisionMemo, DecisionMemoId, DecisionMemoState, HISTORY_PAGE_MAX, TradeId, TradeSide,
        UnixMillis,
    };

    fn day(text: &str) -> NaiveDate {
        NaiveDate::parse_from_str(text, "%Y-%m-%d").unwrap_or(NaiveDate::MIN)
    }

    #[test]
    fn 날수와_쪽_값을_읽고_틀리거나_섞으면_거부한다() {
        assert_eq!(ask_of(None), Some(Ask::Days(MEMOS_DEFAULT_DAYS)));
        assert_eq!(ask_of(Some("days=30")), Some(Ask::Days(30)));
        assert_eq!(
            ask_of(Some("max=500&before=7")),
            Some(Ask::Page {
                before: Some(7),
                max: HISTORY_PAGE_MAX
            })
        );
        assert_eq!(
            ask_of(Some("before=3")),
            Some(Ask::Page {
                before: Some(3),
                max: HISTORY_DEFAULT_MAX
            })
        );
        for bad in [
            "days=0",
            "days=366",
            "days=x",
            "max=0",
            "before=-1",
            "days=3&max=2",
        ] {
            assert_eq!(ask_of(Some(bad)), None, "{bad}");
        }
    }

    #[test]
    fn 최근은_결정_날이나_정산된_날로_본다() {
        let memo = DecisionMemo {
            id: DecisionMemoId(1),
            trade: TradeId(1),
            date: "2026-11-10".into(),
            symbol: "000660".into(),
            side: TradeSide::Buy,
            state: DecisionMemoState::Pending,
            return_bp: None,
            excess_bp: None,
            holding_days: None,
            lesson: None,
            created_at: UnixMillis(0),
            settled_at: None,
        };
        let cutoff = day("2026-11-14");
        assert!(!recent(&memo, cutoff, None));
        assert!(recent(&memo, day("2026-11-10"), None));
        assert!(recent(&memo, cutoff, Some(day("2026-11-17"))));
        assert!(!recent(&memo, cutoff, Some(day("2026-11-13"))));
    }
}
