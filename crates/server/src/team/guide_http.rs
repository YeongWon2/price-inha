//! (기능 L · 결정 30) 팀 지침 경로.
//!
//! # 읽기 (`GET` [`GUIDELINES_PATH`], 세션 — 원격 세션도 읽는다, 200)
//!
//! ```text
//! {"active": [지침], "retired": [지침], "editable": 이 컴퓨터에서 온 요청인가}
//! 지침 = {"id", "kind": "avoid" | "enter" | "manage", "text", "when", "symbol": 종목 | null,
//!         "basis": [근거 글자], "uses", "wins", "losses", "win_rate_pct": 정수 | null,
//!         "created_at", "updated_at", "state": "active" | "retired", "retired_why": 글자 | null}
//! ```
//!
//! 활성은 판단 글에 넣는 차례(성적 순)이고, 내린 것은 늦게 바뀐 것부터다.
//!
//! # 끄기 (`POST` [`GUIDELINE_RETIRE_PATH`], 세션 · 같은 출처 · **이 컴퓨터에서만** — 원격 세션은 403)
//!
//! 몸통은 보지 않는다. 200 `{"id", "state": "retired"}`(이미 내렸으면 바뀐 것 없이 같은 답), 없는 번호는 404
//! `{"error": "not_found"}`. 내리면 [`KIND_GUIDELINE`] 이벤트(`retired`, 까닭 `person`)를 남긴다.

use std::sync::Arc;

use axum::Router;
use axum::extract::{Path, State};
use axum::http::header::CACHE_CONTROL;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use pi_gate::Peer;
use pi_journal::{Guideline, PersonRetire};
use serde_json::{Value, json};

use super::Crew;

/// 팀 지침 읽기 경로.
pub const GUIDELINES_PATH: &str = "/api/guidelines";
/// 팀 지침 하나 끄기 경로.
pub const GUIDELINE_RETIRE_PATH: &str = "/api/guidelines/{id}/retire";
/// 저장되는 이벤트 종류: 지침의 올림 · 고침 · 내림 · 합침 · 1층 길로 보냄.
/// 내용 `{"action": "promoted" | "revised" | "retired" | "merged" | "to_rules", "id": 지침 번호 | null,
/// "kind": 종류 | null, "why": 까닭, "into": 합쳐진 쪽 번호 | null}`. 비밀 값 · 지침 글은 담지 않는다.
pub const KIND_GUIDELINE: &str = "guideline";

/// 세션 경로에 끼운다(끄기는 안에서 `pi_gate::local_only`로 닫는다).
pub(crate) fn routes(crew: Arc<Crew>) -> Router {
    let retire = Router::new()
        .route(GUIDELINE_RETIRE_PATH, post(retire_one))
        .with_state(Arc::clone(&crew));
    Router::new()
        .route(GUIDELINES_PATH, get(list))
        .with_state(crew)
        .merge(pi_gate::local_only(retire))
}

/// JSON 응답. 브라우저나 중간 자리가 간직하지 않게 한다.
fn reply(status: StatusCode, body: Value) -> Response {
    (status, [(CACHE_CONTROL, "no-store")], axum::Json(body)).into_response()
}

/// 지침 하나의 읽기 모양.
fn view(guideline: &Guideline) -> Value {
    json!({
        "id": guideline.id,
        "kind": guideline.kind.word(),
        "text": guideline.text,
        "when": guideline.when,
        "symbol": guideline.symbol,
        "basis": guideline.basis,
        "uses": guideline.uses,
        "wins": guideline.wins,
        "losses": guideline.losses,
        "win_rate_pct": guideline.win_rate_pct(),
        "created_at": guideline.created_at,
        "updated_at": guideline.updated_at,
        "state": if guideline.is_active() { "active" } else { "retired" },
        "retired_why": guideline.retired_why,
    })
}

async fn list(
    State(crew): State<Arc<Crew>>,
    peer: Option<axum::Extension<Peer>>,
    headers: HeaderMap,
) -> Response {
    let editable = crate::setup_models::from_this_computer(peer.as_deref(), &headers);
    let book = crew.guide.read();
    let active: Vec<Value> = book.ranked().into_iter().map(view).collect();
    let mut gone: Vec<&Guideline> = book.guidelines.iter().filter(|g| !g.is_active()).collect();
    gone.sort_by(|a, b| b.updated_at.cmp(&a.updated_at).then(a.id.cmp(&b.id)));
    let retired: Vec<Value> = gone.into_iter().map(view).collect();
    reply(
        StatusCode::OK,
        json!({ "active": active, "retired": retired, "editable": editable }),
    )
}

async fn retire_one(State(crew): State<Arc<Crew>>, Path(id): Path<String>) -> Response {
    let now = crew.clock.now().timestamp_millis();
    let (outcome, payload) = crew.guide.update(|book| {
        let outcome = book.retire_by_person(&id, now);
        let payload = match &outcome {
            PersonRetire::Retired(change) => Some(super::guide::change_payload(book, change)),
            _ => None,
        };
        (outcome, payload)
    });
    if outcome == PersonRetire::NotFound {
        return reply(StatusCode::NOT_FOUND, json!({ "error": "not_found" }));
    }
    if let Some(payload) = payload {
        tracing::info!(id = %id, "사람이 팀 지침 하나를 껐습니다");
        crew.record_guideline(payload).await;
    }
    reply(StatusCode::OK, json!({ "id": id, "state": "retired" }))
}
