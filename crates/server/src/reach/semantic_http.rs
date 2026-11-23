//! (10차 10c 13번) 뜻 검색 설정 화면([`SETUP_SEMANTIC_PATH`]).

use std::sync::Arc;

use axum::Router;
use axum::body::Bytes;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::Response;
use axum::routing::{get, post};
use serde_json::{Value, json};

use super::office_http::reply;
use super::semantic::{ChangeError, Semantic, SemanticSettings};
use super::{SEMANTIC_LOOPBACK_ONLY, SETUP_SEMANTIC_PATH};

async fn show(State(semantic): State<Arc<Semantic>>) -> Response {
    reply(StatusCode::OK, semantic.settings().to_json())
}

async fn change(State(semantic): State<Arc<Semantic>>, body: Bytes) -> Response {
    let body: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
    let Some(settings) = SemanticSettings::from_json(&body) else {
        return reply(StatusCode::BAD_REQUEST, json!({"error": "invalid_input"}));
    };
    match semantic.change(settings) {
        Ok(()) => reply(StatusCode::OK, semantic.settings().to_json()),
        Err(ChangeError::NotLoopback) => reply(
            StatusCode::BAD_REQUEST,
            json!({"error": SEMANTIC_LOOPBACK_ONLY}),
        ),
        Err(ChangeError::Save(error)) => {
            tracing::warn!(%error, "뜻 검색 설정을 남기지 못했습니다");
            reply(
                StatusCode::INTERNAL_SERVER_ERROR,
                json!({"error": "internal"}),
            )
        }
    }
}

/// `GateBuilder::protected_routes`에 끼운다. 읽기는 세션이면 되고, 바꾸기는 그 컴퓨터에서만 받는다
/// (`pi_gate::local_only`).
pub(crate) fn routes(semantic: Arc<Semantic>) -> Router {
    let change = Router::new()
        .route(SETUP_SEMANTIC_PATH, post(change))
        .with_state(Arc::clone(&semantic));
    Router::new()
        .route(SETUP_SEMANTIC_PATH, get(show))
        .with_state(semantic)
        .merge(pi_gate::local_only(change))
}
