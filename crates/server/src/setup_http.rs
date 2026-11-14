//! 시작 전 필수 설정의 경로: 설정 상태 읽기 · 키 등록 · 종목 등록. 모양은 [`crate::setup`]의 문서에 있다.
//!
//! 요청 본문을 기록하지 않는다. 응답은 "등록됨"이나 까닭뿐이다.

use std::sync::Arc;

use axum::Router;
use axum::extract::{Request, State};
use axum::http::StatusCode;
use axum::http::header::CACHE_CONTROL;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use serde_json::{Value, json};

use crate::desk::Desk;
use crate::{SETUP_KEYS_PATH, SETUP_PATH, SETUP_SYMBOLS_PATH, SetupRefusal};

/// 등록 요청 본문의 크기 상한(바이트). 키 몇 개나 종목 200개에는 넉넉하다. 넘으면 읽지 않고 거부한다.
const MAX_BODY_BYTES: usize = 16 * 1024;

/// 세션이 있어야 열리는 경로: 설정 상태. `GateBuilder::protected_routes`에 끼운다.
pub(crate) fn status_routes(desk: Arc<Desk>) -> Router {
    Router::new()
        .route(SETUP_PATH, get(status))
        .with_state(desk)
}

/// 세션 · 같은 출처에 더해 그 컴퓨터에서만 열리는 경로: 키 등록 · 종목 등록.
/// `GateBuilder::local_routes`에 끼운다(그 경로 자체가 `pi_gate::local_only`를 거친다).
pub(crate) fn register_routes(desk: Arc<Desk>) -> Router {
    Router::new()
        .route(SETUP_KEYS_PATH, post(register_key))
        .route(SETUP_SYMBOLS_PATH, post(register_symbols))
        .with_state(desk)
}

/// JSON 응답. 브라우저나 중간 자리가 간직하지 않게 한다.
fn reply(status: StatusCode, body: Value) -> Response {
    (status, [(CACHE_CONTROL, "no-store")], axum::Json(body)).into_response()
}

/// 등록 결과를 응답으로 옮긴다: "등록됨" 또는 까닭. 그 밖의 칸은 없다.
fn registered(outcome: Result<(), SetupRefusal>) -> Response {
    match outcome {
        Ok(()) => reply(StatusCode::OK, json!({ "registered": true })),
        Err(why) => {
            let status =
                StatusCode::from_u16(why.status()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
            reply(status, json!({ "error": why.reason() }))
        }
    }
}

/// 본문을 상한까지만 읽는다. 넘거나 읽지 못하면 `None`(잘못된 입력으로 거부한다).
async fn body_of(request: Request) -> Option<axum::body::Bytes> {
    axum::body::to_bytes(request.into_body(), MAX_BODY_BYTES)
        .await
        .ok()
}

async fn status(State(desk): State<Arc<Desk>>) -> Response {
    reply(StatusCode::OK, desk.status().to_json())
}

async fn register_key(State(desk): State<Arc<Desk>>, request: Request) -> Response {
    let outcome = match body_of(request).await {
        Some(body) => desk.register_key(&body).await,
        None => Err(SetupRefusal::InvalidInput),
    };
    registered(outcome)
}

async fn register_symbols(State(desk): State<Arc<Desk>>, request: Request) -> Response {
    let outcome = match body_of(request).await {
        Some(body) => {
            // 파일을 쓰는 동안 다른 요청을 막지 않게 따로 돌린다.
            tokio::task::spawn_blocking(move || desk.register_symbols(&body))
                .await
                .unwrap_or(Err(SetupRefusal::SaveFailed))
        }
        None => Err(SetupRefusal::InvalidInput),
    };
    registered(outcome)
}
