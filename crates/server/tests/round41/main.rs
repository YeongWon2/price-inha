//! server 40차 시험: 사람이 내리는 전체 정지(경로 · 메신저 `/stop`)와 알림 전달처 설정 · 조용한 시간(결정 25 · 24 ·
//! 스펙 F4 · R14-1 · additions.md V87~V91). 바깥(증권사 · 텔레그램 · 푸시 서비스 · GitHub)은 로컬 가짜 서버뿐이다.
//!
//! - `bench`: 발판이 지금 구현에서 선다 — 운용 중이 되고 그대로 남음 · 승인 요청을 만들 수 있음 · 지금 알림이 메신저와
//!   폰 푸시로 감(40차 "발판 먼저").
//! - `stop`: `POST /api/team/stop` — 운용 중 · 쉬는 중에서 전체 정지(기록 사유 `OwnerStop`, 내 승인 표시) · 이미 정지면
//!   409 · 세션 없으면 401 · 원격 세션으로도 됨 · 정지 뒤 새 사는 주문 막힘 · 다시 시작은 지금 규칙대로 · 담당 도구에 없음.
//! - `messenger_stop`: 허용된 사람의 `/stop`은 정지하고 답 · 이미 정지면 그 답 · 허용 안 된 사람은 그대로 · 도움말에 `/stop`.
//! - `settings`: `GET` · `POST /api/notify/settings` — 기본 모양 · 잠긴 종류 · 둘 다 끄면 400 · 원격 403 · 다시 켜도 남음.
//! - `routing`: 바꾼 설정대로 그 종류가 그 전달처로 가지 않음(가짜 텔레그램 · 가짜 푸시) · 설정 파일이 깨지면 기본(모두).
//! - `quiet`: 조용한 시간대에는 급하지 않은 알림이 메신저 · 푸시로 가지 않고 급한 알림(승인 요청)은 감 · 자정을 넘는 구간 ·
//!   틀린 시각 글자 400.
//!
//! 화면은 `tests/web/account-stop.test.ts` · `setup-notify*.test.ts`(vitest)와 `tests/e2e/stop.spec.ts` ·
//! `notify.spec.ts`가 본다.

mod bench;
mod common;
mod messenger_stop;
mod quiet;
mod routing;
mod settings;
mod stop;
