//! (7차) 접속 키 조립: 발급 · 잠금 풀기 · 접속 기록 보기. gate의 [`pi_gate::AccessKeys`]를 잇는다.
//!
//! | 경로 | 누가 | 무엇 |
//! |---|---|---|
//! | `POST` [`ACCESS_KEY_PATH`] | 세션 + 같은 출처 + 그 컴퓨터 | 새 키를 만들어 답에 **한 번만** 싣는다(`{"access_key", "generation"}`). 앞 세대 원격 세션은 끊긴다(V41) |
//! | `POST` [`ACCESS_UNLOCK_PATH`] | 세션 + 같은 출처 + 그 컴퓨터 | `{"device": "<IP>"}`의 잠금을 푼다(N3). 답은 `{"unlocked": bool}` |
//! | `GET` [`ACCESS_LOG_PATH`] | 세션 | 접속 기록(새것부터 [`ACCESS_LOG_LIMIT`]줄)과 지금 잠긴 기기(V40) |
//!
//! 키 글자는 발급 답 말고는 어디에도(기록 · 로그 · 다른 응답) 싣지 않는다. 저장소에는 gate가
//! 소금 친 해시만 둔다. 잠금 알림(E2)은 `reach`의 `LockNotices`가 notify로 잇는다.
//!
//! 관문을 연 뒤에야 [`pi_gate::AccessKeys`]를 얻으므로, 경로는 빈 자리([`AccessSlot`])를 쥐고
//! 열고 나서 채운다. 채우기 전의 요청은 503이다.

use std::net::IpAddr;
use std::sync::{Arc, OnceLock};

use axum::Router;
use axum::body::Bytes;
use axum::extract::State;
use axum::http::StatusCode;
use axum::http::header::CACHE_CONTROL;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use pi_gate::AccessKeys;
use pi_store::Store;
use serde_json::{Value, json};

/// `POST`: 새 접속 키를 만든다(그 컴퓨터에서만).
pub const ACCESS_KEY_PATH: &str = "/api/access/key";
/// `POST`: 막힌 기기를 푼다(그 컴퓨터에서만).
pub const ACCESS_UNLOCK_PATH: &str = "/api/access/unlock";
/// `GET`: 접속 기록과 지금 잠긴 기기(세션).
pub const ACCESS_LOG_PATH: &str = "/api/access/log";

/// 접속 기록 보기에 싣는 줄 수(새것부터).
const ACCESS_LOG_LIMIT: usize = 200;

/// 관문을 연 뒤 채우는 [`AccessKeys`] 자리.
pub(crate) type AccessSlot = Arc<OnceLock<AccessKeys>>;

#[derive(Clone)]
struct LogState {
    slot: AccessSlot,
    store: Store,
}

/// 그 컴퓨터에서만 열리는 경로: 발급 · 잠금 풀기. `GateBuilder::local_routes`에 끼운다.
pub(crate) fn local_routes(slot: AccessSlot) -> Router {
    Router::new()
        .route(ACCESS_KEY_PATH, post(issue))
        .route(ACCESS_UNLOCK_PATH, post(unlock))
        .with_state(slot)
}

/// 세션이 있어야 열리는 경로: 접속 기록 보기. `GateBuilder::protected_routes`에 끼운다.
pub(crate) fn session_routes(slot: AccessSlot, store: Store) -> Router {
    Router::new()
        .route(ACCESS_LOG_PATH, get(access_log))
        .with_state(LogState { slot, store })
}

/// JSON 응답. 브라우저나 중간 자리가 간직하지 않게 한다.
fn reply(status: StatusCode, body: Value) -> Response {
    (status, [(CACHE_CONTROL, "no-store")], axum::Json(body)).into_response()
}

fn refuse(status: StatusCode, reason: &str) -> Response {
    reply(status, json!({ "error": reason }))
}

fn not_ready() -> Response {
    refuse(StatusCode::SERVICE_UNAVAILABLE, "not_ready")
}

async fn issue(State(slot): State<AccessSlot>) -> Response {
    let Some(keys) = slot.get() else {
        return not_ready();
    };
    match keys.issue().await {
        Ok(issued) => reply(
            StatusCode::OK,
            json!({
                pi_gate::ACCESS_KEY_FIELD: issued.key,
                "generation": issued.generation,
            }),
        ),
        Err(e) => {
            tracing::error!(error = %e, "접속 키를 만들지 못했습니다");
            refuse(StatusCode::INTERNAL_SERVER_ERROR, "issue_failed")
        }
    }
}

/// 본문 `{"device": "<IP>"}`에서 기기를 읽는다.
fn device_of(body: &[u8]) -> Option<IpAddr> {
    let value: Value = serde_json::from_slice(body).ok()?;
    value.get("device")?.as_str()?.trim().parse().ok()
}

async fn unlock(State(slot): State<AccessSlot>, body: Bytes) -> Response {
    let Some(keys) = slot.get() else {
        return not_ready();
    };
    let Some(device) = device_of(&body) else {
        return refuse(StatusCode::BAD_REQUEST, "invalid_input");
    };
    let unlocked = keys.unlock(device);
    if unlocked {
        tracing::info!(%device, "잠긴 기기를 풀었습니다");
    }
    reply(StatusCode::OK, json!({ "unlocked": unlocked }))
}

async fn access_log(State(state): State<LogState>) -> Response {
    let Some(keys) = state.slot.get() else {
        return not_ready();
    };
    let locked: Vec<Value> = keys
        .locked()
        .into_iter()
        .map(|d| json!({ "device": d.device.to_string(), "until": d.until.0 }))
        .collect();
    let store = state.store.clone();
    let read = tokio::task::spawn_blocking(move || store.access_log()).await;
    let Ok(Ok(records)) = read else {
        return refuse(StatusCode::INTERNAL_SERVER_ERROR, "read_failed");
    };
    let entries: Vec<Value> = records
        .iter()
        .rev()
        .take(ACCESS_LOG_LIMIT)
        .map(|r| json!({ "seq": r.seq, "at": r.at.0, "origin": r.origin, "action": r.action }))
        .collect();
    reply(
        StatusCode::OK,
        json!({ "entries": entries, "locked": locked }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty() -> AccessSlot {
        Arc::default()
    }

    /// (8차) 관문을 열기 전(자리를 채우기 전)에는 세 경로 모두 503 `not_ready`이고 아무것도 하지 않는다.
    #[tokio::test]
    async fn 채우기_전에는_발급_풀기_기록_모두_503이다() {
        let issued = issue(State(empty())).await;
        assert_eq!(issued.status(), StatusCode::SERVICE_UNAVAILABLE);
        let body = Bytes::from_static(br#"{"device": "100.64.0.7"}"#);
        let unlocked = unlock(State(empty()), body).await;
        assert_eq!(unlocked.status(), StatusCode::SERVICE_UNAVAILABLE);
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().join("test.db")).unwrap();
        let state = LogState {
            slot: empty(),
            store,
        };
        let log = access_log(State(state)).await;
        assert_eq!(log.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(
            log.headers().get(CACHE_CONTROL).map(|v| v.as_bytes()),
            Some(&b"no-store"[..])
        );
    }
}
