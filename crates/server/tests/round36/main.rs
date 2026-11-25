//! server 34차 시험: 승인 ✋ 말풍선의 서버 쪽(C27 · E49 · V62 · R5-10).
//!
//! - `snapshot`: `GET /api/office`의 `pending_approvals` — 기다리는 승인만(담당 · 번호 · 시각 · 가린 요약 · 지문 앞
//!   6자), 오래된 것부터 · 승인 · 거절 · 만료 뒤에는 빠짐 · 비밀 값과 전체 지문 없음 · 옛 칸은 그대로.
//! - `events`: `office.approval` 이벤트에 `agent` · `state`(waiting · answered · expired)가 더해진다(옛 칸은 그대로).
//!
//! 화면(✋ 말풍선 · 알림음 · 끝남 표시 · 범례)은 `tests/web/office-*.test.ts`(vitest)가 본다.
//! 시간(신호 유효 시간 · 기한)은 넣어 준 시계를 옮겨 잰다(실제로 오래 자지 않는다).

mod common;
mod events;
mod snapshot;
