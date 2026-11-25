//! server 35차 시험: 업종표 · 금지 종목을 실제로 잇고, 시연에서 주문까지 간다(결정 19번 · P5 · P8 · P12 ·
//! 스펙 R7-3 · F5 · additions.md D14 · C33).
//!
//! - `file`: 보관 폴더의 `rules-config.json`을 켤 때 읽어 주문 전 점검에 쓴다 — 없으면 빈 표(P8로 막힘, 지금
//!   동작) · 업종이 있으면 P8을 지남 · 금지 종목은 P5로 막힘 · 깨진 파일이면 켜지지 않고 까닭을 말함.
//! - `api`: `GET` · `POST /api/rules/config` — 모양 · 그 컴퓨터에서만 고침(원격 403) · 틀린 값 400과 저장 안 됨 ·
//!   바꾼 값이 다시 켜지 않아도 다음 점검에 쓰임 · 다시 켜도 남음 · 바꾼 일의 이벤트 · 관문 401.
//! - `demo`: 시연 조립(`run_assembly --demo`)은 지켜보는 종목에 시연 업종을 조립 값으로 넣고 파일에 쓰지 않는다 ·
//!   팀이 사기로 정하면 막힘이 아니라 주문(또는 승인 기다림)까지 간다 · 가짜 서버 밖 요청 0건.
//! - `proposal`: 규칙 개선안의 승인 요청도 `office.approval`(review · waiting)을 낸다.
//!
//! 화면(금지 종목 · 업종표 묶음 · 승인 답 실패 뒤 다시 읽기)은 `tests/web/rules-config*.test.ts` ·
//! `office-approval-resync.test.ts`(vitest)가 본다. 시간은 넣어 준 시계로 정한다(실제로 오래 자지 않는다).

mod api;
mod common;
mod demo;
mod file;
mod proposal;
