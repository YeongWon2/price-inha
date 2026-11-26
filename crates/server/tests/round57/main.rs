//! server 57차 시험: 설정 화면에서 담당별 AI 모델 고르기(prd R18-4 · dev C32 · V78). 바깥(증권사 · AI · 자료)은
//! 로컬 가짜 서버뿐이다.
//!
//! - `bench`: 발판이 지금 구현에서 선다 — 팀을 잇고 논의를 돌리면 가짜 AI가 담당마다 기본 모델 이름을 받는다(57차
//!   "발판 먼저"). 이 파일의 시험은 구현 전에도 통과하는 것이 정상이다.
//! - `models`: `GET|POST /api/setup/models` — 읽기 모양 · 저장 · 설정 상태에 반영 · 다시 켜도 남음 · 틀린 값 400 ·
//!   원격 403 · 깨진 파일이면 기본 · 응답에 키 값 없음.
//! - `apply`: 바꾼 뒤 그 담당의 AI 요청에 새 모델 이름이 실리고 다른 담당은 그대로 · 다시 켜지 않아도 다음 턴부터 ·
//!   도는 논의는 시작할 때의 모델로 끝남.
//! - `wizard`: `price-inha setup ai` 구역 끝의 안내 한 줄.
//!
//! 화면은 `tests/web/setup-models.test.ts`(vitest)와 `tests/e2e/setup-models.spec.ts`가 본다.

mod apply;
mod bench;
mod common;
mod models;
mod wizard;
