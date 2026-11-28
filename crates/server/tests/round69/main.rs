//! 69차(체험판에서 찾음 — 지켜보기로 끝난 매매가 "논의중"으로 보이는 문제)의 Rust 쪽 시험.
//!
//! - `scaffold`: 발판이 지금 구현에서 서는지 — 서버가 뜨고, 논의중 · 판단된 매매와 매매 방의 방 로그 줄을 만들고,
//!   매매 목록 · 매매 하나 · 근거 경로를 읽는다.
//! - `ended`: 매매 목록(`GET /api/trades`) · 매매 하나(`GET /api/trades/{id}`) · 근거(`GET /api/trades/{id}/basis`)에
//!   "논의중인데 그 매매 방의 마지막 논의 결과가 지켜보기"면 `watched: {"reason_code", "at"}`, 논의가 오류로
//!   끝났으면 `failed: {"reason_code", "at"}`가 붙는다. 그 뒤 새 논의의 줄(담당의 글)이 있으면 칸이 없다. 매매
//!   상태는 그대로 `discussing`이다. 상태 거르기는 `discussing` · `watched` · `failed`로 나뉜다.
//!
//! 화면 쪽(히스토리 · 사무실 근거 패널의 "지켜봄 — <까닭 글>")은 vitest(`tests/web/discussion-end.test.ts`)에 있다.
//! (46차) 2026-11-28 체험판 · 백로그 U · V32와 함께 잠금.

mod common;
mod ended;
mod scaffold;
