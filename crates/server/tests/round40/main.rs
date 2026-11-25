//! server 39차 시험: 메신저(텔레그램)를 잇는다 — 등록 · 페어링 · 받기 루프 · 명령 · 알림 · 마법사(결정 24 ·
//! additions.md V87~V91). 바깥(텔레그램)은 로컬 가짜 서버(wiremock)뿐이고 시간은 넣어 준 시계로 줄인다.
//!
//! - `register`: 토큰은 getMe로 확인한 뒤에만 저장 · 틀린 토큰은 저장 안 됨 · 원격 403 · 응답 · 이벤트 · 홈 폴더의
//!   비밀 아닌 파일에 토큰 글자 없음 · 지우면 장부도 없음.
//! - `status`: 상태 모양(보낸 사람 ID · 코드 · 토큰 없음).
//! - `inbound`: 처음 보는 1:1에 코드 답 한 번 · 그룹 글 무시 · 다시 켜도 자리를 이어 같은 글을 두 번 처리하지 않음 ·
//!   401이면 멈추고 unauthorized · 429면 그만큼 쉼 · 연결 실패는 물러서며 다시.
//! - `pairing`: 맞는 코드로 연결 · 틀리면 400 · 5번째에 423 · 원격 403 · 빼기 · 허용 1명.
//! - `commands`: status · approvals · reject(사무실 승인 이벤트 answered) · 없는 번호 · stop · help · 승인 글은
//!   "웹에서만"(승인 그대로) · 허용 안 된 사람의 명령은 실행 안 됨.
//! - `outbound`: 승인 요청 알림이 허용된 대화로(가린 글 + 거절 안내) · 허용된 사람이 없으면 보내지 않음 · 보내기
//!   실패해도 승인 길은 그대로 · 비밀 글자는 가려져 나감.
//! - `demo`: 시연 모드는 루프 없음 · 등록은 시연 안내 · 가짜 서버 밖 요청 0건.
//! - `wizard`: `setup messenger`가 도는 서버 · 가짜 텔레그램으로 토큰 → 코드 → 연결까지 가고 출력에 토큰이 없다.
//!
//! 화면(메신저 묶음)은 `tests/web/setup-messenger*.test.ts`(vitest)와 `tests/e2e/messenger.spec.ts`가 본다.

mod bench;
mod commands;
mod common;
mod demo;
mod inbound;
mod outbound;
mod pairing;
mod register;
mod status;
mod wizard;
