//! 히스토리 화면의 읽는 경로(29차 1번 · C30 · V83 · V84 · F7-7): 매매 목록 · 매매 하나 · 승인 요청 기록.
//!
//! 폰 경로와 같은 상태([`Phone`])로 세션 관문 안에 끼운다. 조회는 저장소의 히스토리 조회
//! (`trade_page` · `trade_flow` · `approval_page`)만 쓴다. 쪽의 `next`는 한 줄 더 읽어 더 있는지 본다.

use std::sync::Arc;

use axum::Router;
use axum::extract::{Path, RawQuery, State};
use axum::http::StatusCode;
use axum::response::Response;
use axum::routing::get;
use pi_market::Market;
use pi_store::{
    AccountKind, Amount, ApprovalId, ApprovalLine, ApprovalStatus, HISTORY_PAGE_MAX, StoreError,
    Trade, TradeFlow, TradeId, TradeListItem, TradeSide, TradeState,
};
use rust_decimal::Decimal;
use serde_json::{Value, json};

use super::office_http::reply;
use super::phone_http::{Phone, internal_error, refuse};
use super::{APPROVAL_LOG_PATH, HISTORY_DEFAULT_MAX, TRADE_PATH, TRADES_PATH};
use crate::team::discussion_end::{EndFilter, end_fields, ended_page, with_end};
use crate::trading::approval_status_word;

/// 쪽 읽기 값: `before` · `max` · 거르기 글자.
struct Paging {
    before: Option<i64>,
    max: usize,
    filter: Option<String>,
}

/// `?before=&max=&<filter>=`를 읽는다. 틀린 값이면 `None`(400). 같은 이름이 여럿이면 처음 것.
fn paging(query: Option<&str>, filter: &str) -> Option<Paging> {
    let mut before = None;
    let mut max = None;
    let mut chosen = None;
    for (key, value) in url::form_urlencoded::parse(query.unwrap_or_default().as_bytes()) {
        if key == "before" && before.is_none() {
            before = Some(value.parse::<i64>().ok().filter(|n| *n >= 1)?);
        } else if key == "max" && max.is_none() {
            max = Some(value.parse::<u64>().ok().filter(|n| *n >= 1)?);
        } else if key == filter && chosen.is_none() {
            chosen = Some(value.into_owned());
        }
    }
    let max = max.map_or(HISTORY_DEFAULT_MAX, |n| {
        usize::try_from(n).map_or(HISTORY_PAGE_MAX, |n| n.min(HISTORY_PAGE_MAX))
    });
    Some(Paging {
        before,
        max,
        filter: chosen,
    })
}

/// 쪽이 꽉 찼으면 마지막 줄 뒤로 한 줄 더 읽어 다음 쪽이 있는지 본다. 있으면 마지막 줄의 번호.
fn next_cursor<E>(
    shown: usize,
    max: usize,
    last: Option<i64>,
    one_more: impl FnOnce(i64) -> Result<bool, E>,
) -> Result<Option<i64>, E> {
    match last {
        Some(last) if shown >= max => Ok(one_more(last)?.then_some(last)),
        _ => Ok(None),
    }
}

/// 매매 상태 글자(스펙의 매매 상태표).
fn trade_state_word(state: TradeState) -> &'static str {
    match state {
        TradeState::Discussing => "discussing",
        TradeState::Judged => "judged",
        TradeState::AwaitingApproval => "awaiting_approval",
        TradeState::Blocked => "blocked",
        TradeState::Ordered => "ordered",
        TradeState::Holding => "holding",
        TradeState::Sold => "sold",
        TradeState::Reviewed => "reviewed",
    }
}

fn account_word(account: AccountKind) -> &'static str {
    match account {
        AccountKind::Paper => "paper",
        AccountKind::Live => "live",
    }
}

/// 매매 목록의 줄 하나(매매 하나의 앞 칸도 같다).
fn trade_item(item: &TradeListItem) -> Value {
    json!({
        "id": item.id.0,
        "symbol": item.symbol,
        "account": account_word(item.account),
        "state": trade_state_word(item.state),
        "created_at": item.created_at.0,
        "updated_at": item.updated_at.0,
        "decided_at": item.decided_at.map(|at| at.0),
    })
}

async fn trades(State(phone): State<Arc<Phone>>, RawQuery(query): RawQuery) -> Response {
    let Some(paging) = paging(query.as_deref(), "state") else {
        return refuse(StatusCode::BAD_REQUEST, "invalid_input");
    };
    // (69차) 논의중은 끝난 논의(지켜봄 · 끝나지 못함)와 나눠 거른다.
    if let Some(filter) = paging.filter.as_deref().and_then(EndFilter::parse) {
        return ended_trades(&phone, &paging, filter);
    }
    let state = match paging.filter.as_deref() {
        None => None,
        Some(word) => {
            match TradeState::ALL
                .into_iter()
                .find(|state| trade_state_word(*state) == word)
            {
                Some(state) => Some(state),
                None => return refuse(StatusCode::BAD_REQUEST, "invalid_input"),
            }
        }
    };
    let store = &phone.store;
    let read = store
        .trade_page(paging.before.map(TradeId), paging.max, state)
        .and_then(|rows| {
            let last = rows.last().map(|row| row.id.0);
            let next = next_cursor(rows.len(), paging.max, last, |last| {
                Ok::<_, StoreError>(!store.trade_page(Some(TradeId(last)), 1, state)?.is_empty())
            })?;
            Ok((rows, next))
        })
        .and_then(|(rows, next)| {
            let items = rows
                .iter()
                .map(|row| with_end(store, row.id, row.state, trade_item(row)))
                .collect::<Result<Vec<_>, _>>()?;
            Ok((items, next))
        });
    match read {
        Ok((items, next)) => reply(StatusCode::OK, json!({ "trades": items, "next": next })),
        Err(error) => internal_error(error),
    }
}

/// (69차) 논의중을 나눈 거르기(`discussing` · `watched` · `failed`)의 쪽.
fn ended_trades(phone: &Phone, paging: &Paging, filter: EndFilter) -> Response {
    let before = paging.before.map(TradeId);
    match ended_page(&phone.store, before, paging.max, filter) {
        Ok((rows, next)) => {
            let items: Vec<Value> = rows
                .iter()
                .map(|(row, end)| {
                    let mut item = trade_item(row);
                    if let Value::Object(map) = &mut item {
                        map.extend(end_fields(end.as_ref()));
                    }
                    item
                })
                .collect();
            reply(StatusCode::OK, json!({ "trades": items, "next": next }))
        }
        Err(error) => internal_error(error),
    }
}

/// 그 시장 통화의 글자와, 저장소 금액(통화 최소 단위) → 십진수 글자.
fn currency_of(market: Market) -> &'static str {
    match market {
        Market::Us => "USD",
        _ => "KRW",
    }
}

fn money(market: Market, amount: Option<Amount>) -> Option<String> {
    amount.map(|amount| match market {
        Market::Us => Decimal::new(amount.0, 2).to_string(),
        _ => Decimal::from(amount.0).to_string(),
    })
}

/// 매매의 근거 칸(글은 가린 것).
fn basis_of(phone: &Phone, trade: &Trade, market: Market) -> Value {
    let hide = |text: &str| phone.shown_reason(text);
    let r = &trade.rationale;
    json!({
        "hypothesis": r.hypothesis.as_deref().map(hide),
        "trigger": r.trigger.as_deref().map(hide),
        "entry": money(market, r.entry),
        "stop": money(market, r.stop),
        "target": money(market, r.target),
        "expected_value": money(market, r.expected_value),
        "scores": trade.scores.iter().map(|s| json!({
            "role": s.role, "score": s.score, "reason": hide(&s.reason),
        })).collect::<Vec<_>>(),
        "consensus_score": trade.consensus_score,
        "sources": trade.sources.iter().map(|s| json!({
            "name": hide(&s.name), "known_at": s.known_at.0,
        })).collect::<Vec<_>>(),
    })
}

/// 매매 하나의 몸통: 목록의 줄 칸 + 상태 흐름 · 방향 · 수량 · 자료 이름 · 근거 · 쉬운 말 요약.
fn trade_body(phone: &Phone, trade: &Trade, flow: &TradeFlow) -> Value {
    let market = phone.trading.market();
    let mut body = trade_item(&TradeListItem {
        id: trade.id,
        symbol: trade.symbol.clone(),
        account: trade.account,
        state: trade.state,
        created_at: trade.created_at,
        updated_at: trade.updated_at,
        decided_at: trade.decided_at,
    });
    let states: Vec<Value> = flow
        .states
        .iter()
        .map(|change| {
            json!({
                "state": trade_state_word(change.to),
                "from": change.from.map(trade_state_word),
                "at": change.at.0,
            })
        })
        .collect();
    let side = flow.side.map(|side| match side {
        TradeSide::Buy => "buy",
        TradeSide::Sell => "sell",
    });
    let facts: Vec<String> = flow
        .fact_labels
        .iter()
        .map(|label| phone.shown_reason(label))
        .collect();
    // 근거 경로(`/api/trades/{id}/basis`)와 같은 쉬운 말 요약(`pi_journal::plain_summary`).
    let summary = phone.shown_reason(&pi_journal::plain_summary(trade));
    body["states"] = json!(states);
    body["side"] = json!(side);
    body["quantity"] = json!(flow.quantity);
    body["facts"] = json!(facts);
    body["currency"] = json!(currency_of(market));
    body["basis"] = basis_of(phone, trade, market);
    body["summary"] = json!(summary);
    body
}

/// (50차 · C23) 매매 하나의 근거에 근거 카드 칸(담당별 의견 · 팀장 요약 · 찬반 수)을 더한다.
fn with_card(
    phone: &Phone,
    mut body: Value,
    (trade, flow, record): &(Trade, TradeFlow, pi_store::TradeOpinions),
) -> Value {
    let hide = |text: &str| phone.shown_reason(text);
    let fields = crate::team::card::card_fields(trade, flow.side, record, &hide);
    if let Value::Object(basis) = &mut body["basis"] {
        basis.extend(fields);
    }
    body
}

async fn trade(State(phone): State<Arc<Phone>>, Path(id): Path<String>) -> Response {
    let Ok(id) = id.parse::<i64>() else {
        return refuse(StatusCode::BAD_REQUEST, "invalid_input");
    };
    let id = TradeId(id);
    let read = phone
        .store
        .get_trade(id)
        .and_then(|trade| Ok((trade, phone.store.trade_flow(id)?)))
        .and_then(|(trade, flow)| Ok((trade, flow, phone.store.trade_opinions(id)?)));
    match read {
        Ok(read) => {
            let body = with_card(&phone, trade_body(&phone, &read.0, &read.1), &read);
            // (69차) 논의중이면 끝난 논의 칸.
            match with_end(&phone.store, id, read.0.state, body) {
                Ok(body) => reply(StatusCode::OK, body),
                Err(error) => internal_error(error),
            }
        }
        Err(StoreError::TradeNotFound(_)) => refuse(StatusCode::NOT_FOUND, "not_found"),
        Err(error) => internal_error(error),
    }
}

/// 승인 요청 기록의 줄 하나. 요약은 승인 카드와 같은 가린 사유 글이다.
fn approval_item(phone: &Phone, line: &ApprovalLine) -> Value {
    let text = |name: &str| line.summary[name].as_str().unwrap_or_default();
    json!({
        "id": line.id.0,
        "number": text("number"),
        "summary": phone.shown_reason(text("reason")),
        "status": approval_status_word(line.status),
        "created_at": line.created_at.0,
        "resolved_at": line.resolved_at.map(|at| at.0),
        "consumed_at": line.consumed_at.map(|at| at.0),
    })
}

async fn approvals(State(phone): State<Arc<Phone>>, RawQuery(query): RawQuery) -> Response {
    let Some(paging) = paging(query.as_deref(), "status") else {
        return refuse(StatusCode::BAD_REQUEST, "invalid_input");
    };
    let status = match paging.filter.as_deref() {
        None => None,
        Some(word) => {
            match ApprovalStatus::ALL
                .into_iter()
                .find(|status| approval_status_word(*status) == word)
            {
                Some(status) => Some(status),
                None => return refuse(StatusCode::BAD_REQUEST, "invalid_input"),
            }
        }
    };
    let store = &phone.store;
    let read = store
        .approval_page(paging.before.map(ApprovalId), paging.max, status)
        .and_then(|rows| {
            let last = rows.last().map(|row| row.id.0);
            let next = next_cursor(rows.len(), paging.max, last, |last| {
                Ok::<_, StoreError>(
                    !store
                        .approval_page(Some(ApprovalId(last)), 1, status)?
                        .is_empty(),
                )
            })?;
            Ok((rows, next))
        });
    match read {
        Ok((rows, next)) => {
            let items: Vec<Value> = rows
                .iter()
                .map(|line| approval_item(&phone, line))
                .collect();
            reply(StatusCode::OK, json!({ "approvals": items, "next": next }))
        }
        Err(error) => internal_error(error),
    }
}

/// 폰 경로에 합친다(같은 상태, 세션 관문 안).
pub(super) fn routes() -> Router<Arc<Phone>> {
    Router::new()
        .route(TRADES_PATH, get(trades))
        .route(TRADE_PATH, get(trade))
        .route(APPROVAL_LOG_PATH, get(approvals))
}

#[cfg(test)]
mod tests {
    use super::{next_cursor, paging};
    use crate::HISTORY_DEFAULT_MAX;
    use pi_store::HISTORY_PAGE_MAX;

    #[test]
    fn 쪽_값은_1_이상의_정수만_받고_max는_100으로_줄인다() {
        let plain = paging(None, "state").expect("빈 값");
        assert_eq!(
            (plain.before, plain.max, plain.filter),
            (None, HISTORY_DEFAULT_MAX, None)
        );
        let big = paging(Some("max=500&before=7&state=sold"), "state").expect("큰 max");
        assert_eq!(big.max, HISTORY_PAGE_MAX);
        assert_eq!(big.before, Some(7));
        assert_eq!(big.filter.as_deref(), Some("sold"));
        for bad in [
            "max=0",
            "max=-1",
            "max=abc",
            "before=abc",
            "before=-3",
            "before=0",
        ] {
            assert!(paging(Some(bad), "state").is_none(), "{bad}");
        }
    }

    #[test]
    fn 다음_쪽은_쪽이_꽉_차고_한_줄_더_있을_때만이다() {
        let more = |_| Ok::<_, ()>(true);
        let none = |_| Ok::<_, ()>(false);
        assert_eq!(next_cursor(2, 2, Some(5), more), Ok(Some(5)));
        assert_eq!(next_cursor(2, 2, Some(5), none), Ok(None));
        assert_eq!(next_cursor(1, 2, Some(5), more), Ok(None));
        assert_eq!(next_cursor(0, 2, None, more), Ok(None));
    }
}
