//! (35차 2번) 금지 종목 · 업종표 경로([`RULES_CONFIG_PATH`]): 읽기는 세션이면(원격 포함), 고치기는 그 컴퓨터에서만
//! (`pi_gate::local_only` — 원격 세션은 403, 하루 AI 상한과 같은 관문). 모양은 경로 상수 문서에 있다.

use std::sync::Arc;

use axum::Router;
use axum::body::Bytes;
use axum::extract::State;
use axum::http::header::CACHE_CONTROL;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use pi_gate::{FORWARDING_HEADERS, Peer};
use serde_json::{Value, json};

use super::rules_config::parse_request;
use super::{RULES_CONFIG_PATH, Trading};

/// `GateBuilder::protected_routes`에 끼운다(주문 길 경로와 함께).
pub(super) fn routes(trading: Arc<Trading>) -> Router {
    let change = Router::new()
        .route(RULES_CONFIG_PATH, post(change))
        .with_state(Arc::clone(&trading));
    Router::new()
        .route(RULES_CONFIG_PATH, get(show))
        .with_state(trading)
        .merge(pi_gate::local_only(change))
}

fn reply(status: StatusCode, body: Value) -> Response {
    (status, [(CACHE_CONTROL, "no-store")], axum::Json(body)).into_response()
}

/// 이 요청이 그 컴퓨터에서 왔는가: `pi_gate::local_only`와 같은 판단(관문이 넣은 상대 주소가 루프백이고
/// 전달 헤더가 하나도 없다). 상대 주소를 모르면 아니다.
fn from_this_computer(peer: Option<&Peer>, headers: &HeaderMap) -> bool {
    let loopback = peer.is_some_and(|Peer(ip)| ip.to_canonical().is_loopback());
    loopback
        && FORWARDING_HEADERS
            .iter()
            .all(|name| !headers.contains_key(*name))
}

async fn show(
    State(trading): State<Arc<Trading>>,
    peer: Option<axum::Extension<Peer>>,
    headers: HeaderMap,
) -> Response {
    let editable = from_this_computer(peer.as_deref(), &headers);
    reply(StatusCode::OK, trading.rules_view(editable))
}

async fn change(State(trading): State<Arc<Trading>>, body: Bytes) -> Response {
    let value: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
    let Some(rules) = parse_request(&value) else {
        return reply(StatusCode::BAD_REQUEST, json!({"error": "invalid_input"}));
    };
    match trading.change_rules(rules).await {
        Ok(view) => reply(StatusCode::OK, view),
        Err(error) => {
            tracing::warn!(%error, "금지 종목 · 업종표를 저장하지 못했습니다");
            reply(
                StatusCode::INTERNAL_SERVER_ERROR,
                json!({"error": "internal"}),
            )
        }
    }
}
