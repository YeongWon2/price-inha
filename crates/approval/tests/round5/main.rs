//! approval 5차 시험(백로그 Q · server.md 검토 참고): `unhandled_due`가 닫힌 요청을 매번 처음부터 다 읽지 않게
//! 바꿔도 돌려주는 것은 그대로이고(늦게 닫힌 옛 요청 · 다른 관리자가 남긴 처리 기록 · 다시 켜기),
//! 막힌 까닭 글(`DueHandling::Blocked`)은 상한까지만 남는다.
//! 공용 도우미는 `tests/round4/common.rs`를 그대로 쓴다(그 폴더는 고치지 않는다).
//! (6차) 2026-11-27 백로그 작업을 합쳐 잠금.

#[path = "../round4/common.rs"]
pub mod common;

mod blocked_cap;
mod due_floor;
