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
    let _ = crew;
    Router::new()
}
