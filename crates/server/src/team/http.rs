//! 투자 팀의 경로(세션이 있어야 열린다): 담당 명단 · 매매 근거 · 개선안 목록 · 개선안 답하기 · 규칙 읽기.
//! 모양은 [`super`]의 경로 상수 문서에 있다.

use std::sync::Arc;

use axum::Router;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::http::header::CACHE_CONTROL;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use pi_agent::Role;
use pi_store::{StoreError, TradeId};
use serde_json::{Value, json};

use super::build::role_name;
use super::{
    Crew, PROPOSAL_ANSWER_PATH, PROPOSALS_PATH, RULES_VIEW_PATH, TEAM_MEMBERS_PATH,
    TRADE_BASIS_PATH,
};
use crate::TeamError;

/// `GateBuilder::protected_routes`에 끼운다.
pub(crate) fn routes(crew: Arc<Crew>) -> Router {
    Router::new()
        .route(TEAM_MEMBERS_PATH, get(members))
        .route(TRADE_BASIS_PATH, get(basis))
        .route(PROPOSALS_PATH, get(proposals))
        .route(PROPOSAL_ANSWER_PATH, post(answer))
        .route(RULES_VIEW_PATH, get(rules))
        .with_state(crew)
}

/// JSON 응답. 브라우저나 중간 자리가 간직하지 않게 한다.
fn reply(status: StatusCode, body: Value) -> Response {
    (status, [(CACHE_CONTROL, "no-store")], axum::Json(body)).into_response()
}

fn failed(error: &TeamError) -> Response {
    tracing::warn!(%error, "투자 팀 요청을 처리하지 못했습니다");
    reply(
        StatusCode::INTERNAL_SERVER_ERROR,
        json!({ "error": "internal" }),
    )
}

async fn members() -> Response {
    let list: Vec<Value> = Role::ALL
        .iter()
        .map(|role| json!({ "id": role.id(), "name": role_name(*role) }))
        .collect();
    reply(StatusCode::OK, json!({ "members": list }))
}

async fn basis(State(crew): State<Arc<Crew>>, Path(id): Path<String>) -> Response {
    let not_found = || reply(StatusCode::NOT_FOUND, json!({ "error": "not_found" }));
    let Ok(id) = id.parse::<i64>() else {
        return not_found();
    };
    match crew.journal.read_basis(TradeId(id)) {
        Ok(summary) => reply(StatusCode::OK, json!({ "id": id, "summary": summary })),
        Err(pi_journal::Error::Store(StoreError::TradeNotFound(_))) => not_found(),
        Err(error) => failed(&TeamError::Journal(error.to_string())),
    }
}

async fn proposals(State(crew): State<Arc<Crew>>) -> Response {
    match crew.proposal_list() {
        Ok(list) => reply(StatusCode::OK, json!({ "proposals": list })),
        Err(error) => failed(&error),
    }
}

async fn rules(State(crew): State<Arc<Crew>>) -> Response {
    match crew.rules_view() {
        Ok(view) => reply(StatusCode::OK, view),
        Err(error) => failed(&error),
    }
}

async fn answer(State(crew): State<Arc<Crew>>, body: Bytes) -> Response {
    let body: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
    match crew.answer_proposal(&body) {
        Ok(done) => reply(
            StatusCode::OK,
            json!({ "result": done.result, "reason": done.reason }),
        ),
        Err(TeamError::Unavailable) => reply(
            StatusCode::SERVICE_UNAVAILABLE,
            json!({ "error": "unavailable" }),
        ),
        Err(error) => failed(&error),
    }
}
