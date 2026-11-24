//! 주문 길의 경로(세션이 있어야 열린다): 팀 상태 · 다시 시작 · 승인 답하기 · 템플릿 고르기 · 값 맞추기.
//! 모양은 [`super`]의 경로 상수 문서에 있다.

use std::sync::Arc;

use axum::Router;
use axum::body::Bytes;
use axum::extract::State;
use axum::http::StatusCode;
use axum::http::header::CACHE_CONTROL;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use serde_json::{Value, json};

use super::day::{Restart, team_word};
use super::templates::Refusal;
use super::{
    APPROVAL_ANSWER_PATH, RESTART_KEYS_REQUIRED, RESTART_NOT_STOPPED, TEAM_PATH, TEAM_RESTART_PATH,
    TEMPLATE_ADJUST_PATH, TEMPLATE_OVER_CAP, TEMPLATE_PATH, Trading,
};
use crate::TradingError;

/// `GateBuilder::protected_routes`에 끼운다.
pub(crate) fn routes(trading: Arc<Trading>) -> Router {
    Router::new()
        .route(TEAM_PATH, get(team))
        .route(TEAM_RESTART_PATH, post(restart))
        .route(APPROVAL_ANSWER_PATH, post(answer))
        .route(TEMPLATE_PATH, post(choose))
        .route(TEMPLATE_ADJUST_PATH, post(adjust))
        .with_state(trading)
}

/// JSON 응답. 브라우저나 중간 자리가 간직하지 않게 한다.
fn reply(status: StatusCode, body: Value) -> Response {
    (status, [(CACHE_CONTROL, "no-store")], axum::Json(body)).into_response()
}

fn failed(error: &TradingError) -> Response {
    tracing::warn!(%error, "주문 길 요청을 처리하지 못했습니다");
    reply(
        StatusCode::INTERNAL_SERVER_ERROR,
        json!({ "error": "internal" }),
    )
}

/// 몸통을 JSON으로 읽는다. 읽을 수 없으면 `Null`.
fn json_of(body: &Bytes) -> Value {
    serde_json::from_slice(body).unwrap_or(Value::Null)
}

async fn team(State(trading): State<Arc<Trading>>) -> Response {
    match trading.view() {
        Ok(view) => reply(StatusCode::OK, view),
        Err(error) => failed(&error),
    }
}

async fn restart(State(trading): State<Arc<Trading>>) -> Response {
    match trading.restart() {
        Ok(Restart::Done(state)) => {
            // 다시 시작해도 쓰는 템플릿이 멈춤이거나 없으면 사는 주문은 나가지 않는다: 까닭을 함께 알린다.
            let mut body = json!({ "state": team_word(state) });
            if let Some(notice) = trading.template_notice() {
                body["notice"] = json!(notice);
            }
            reply(StatusCode::OK, body)
        }
        Ok(Restart::KeysRequired) => reply(
            StatusCode::CONFLICT,
            json!({
                "error": RESTART_KEYS_REQUIRED,
                "message": "증권사 키와 AI 키를 먼저 다시 등록해야 다시 시작할 수 있습니다",
            }),
        ),
        Ok(Restart::NotStopped) => reply(
            StatusCode::CONFLICT,
            json!({ "error": RESTART_NOT_STOPPED }),
        ),
        Err(error) => failed(&error),
    }
}

async fn answer(State(trading): State<Arc<Trading>>, body: Bytes) -> Response {
    let body = json_of(&body);
    let text = |name: &str| body[name].as_str().unwrap_or_default().to_string();
    let answered = trading
        .answer(&text("number"), &text("fingerprint"), &text("answer"))
        .await;
    match answered {
        Ok(answered) => {
            let result = if answered.approved {
                "approved"
            } else {
                "rejected"
            };
            reply(
                StatusCode::OK,
                json!({ "result": result, "reason": answered.reason }),
            )
        }
        Err(error) => failed(&error),
    }
}

fn template_reply(outcome: Result<Value, Refusal>) -> Response {
    match outcome {
        Ok(body) => reply(StatusCode::OK, body),
        Err(Refusal::Invalid) => {
            reply(StatusCode::BAD_REQUEST, json!({ "error": "invalid_input" }))
        }
        Err(Refusal::NotFound) => reply(StatusCode::NOT_FOUND, json!({ "error": "not_found" })),
        Err(Refusal::Locked) => reply(StatusCode::CONFLICT, json!({ "error": "locked" })),
        Err(Refusal::OverCap(fields)) => reply(
            StatusCode::BAD_REQUEST,
            json!({ "error": TEMPLATE_OVER_CAP, "fields": fields }),
        ),
        Err(Refusal::Store(error)) => failed(&TradingError::Store(error)),
        Err(Refusal::Failed(error)) => failed(&error),
    }
}

async fn choose(State(trading): State<Arc<Trading>>, body: Bytes) -> Response {
    template_reply(trading.choose_template(&json_of(&body)))
}

async fn adjust(State(trading): State<Arc<Trading>>, body: Bytes) -> Response {
    template_reply(trading.adjust_template(&json_of(&body)).await)
}
