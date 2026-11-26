//! 알림 전달처 설정 경로([`super::NOTIFY_SETTINGS_PATH`]): 읽기는 세션이면(원격 포함), 바꾸기는 그 컴퓨터에서만
//! (`pi_gate::local_only` — 원격 세션은 403). 모양은 모듈 문서 [`super`]에 있다.

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

use super::NOTIFY_SETTINGS_PATH;
use super::book::NotifySettings;
use crate::reach::Linked;

/// 세션 경로(`GateBuilder::protected_routes`)에 끼운다. 바꾸기는 그 안에서 `pi_gate::local_only`로 닫는다.
pub(crate) fn routes(linked: Arc<Linked>) -> Router {
    let change = Router::new()
        .route(NOTIFY_SETTINGS_PATH, post(change))
        .with_state(Arc::clone(&linked));
    Router::new()
        .route(NOTIFY_SETTINGS_PATH, get(show))
        .with_state(linked)
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
    State(linked): State<Arc<Linked>>,
    peer: Option<axum::Extension<Peer>>,
    headers: HeaderMap,
) -> Response {
    let editable = from_this_computer(peer.as_deref(), &headers);
    reply(StatusCode::OK, linked.notify.current().view(editable))
}

async fn change(State(linked): State<Arc<Linked>>, body: Bytes) -> Response {
    let value: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
    let next = match NotifySettings::parse(&value, linked.notify.current().talk()) {
        Ok(next) => next,
        Err(refusal) => return reply(StatusCode::BAD_REQUEST, refusal.body()),
    };
    match linked.change_notify(next) {
        Ok(()) => reply(StatusCode::OK, next.view(true)),
        Err(error) => {
            tracing::warn!(%error, "알림 설정을 저장하지 못했습니다");
            reply(
                StatusCode::INTERNAL_SERVER_ERROR,
                json!({"error": "internal"}),
            )
        }
    }
}
