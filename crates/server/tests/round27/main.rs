//! 25차 사무실 다듬기(`docs/dev/modules/server.md` "25차 사무실 다듬기 — 순한 카메라 · 읽히는 판단 글 · 시연의
//! 속도", 결정 18)의 Rust 테스트. 1번(발언자 카메라)은 화면 시험(`tests/web/office-speaker-camera.test.ts`)이다.
//!
//! 잠긴 테스트 파일을 고치지 않으려고 따로 둔 최상위 테스트 바이너리다. 서버는 실행 파일과 같은 조립
//! ([`price_inha::run_assembly`] → [`price_inha::App::launch_run`])을 쓰되 시계 · 바깥 주소 · 가짜 AI 뜸만
//! 시험이 정한다(round26의 도우미를 본떴다).
//!
//! - `sentence`: 2번 팀장 판단 글을 사람이 읽는 문장으로([`price_inha::leader_sentence`]의 모양).
//! - `talk`: 2번 `office.talk`의 `text`가 문장으로 나오고 방 로그 원문은 그대로다.
//! - `pace`: 3번 시연 조립의 가짜 AI 뜸(기본 2.5초 · 줄이면 논의 글이 차례로 간격을 두고 나온다) · 기본 가짜 AI는 뜸 없음.
//! - `receipt`: 4번 시연 조립으로 띄운 뒤 `team-open` 영수증에 중단이 없다.

mod common;
mod pace;
mod receipt;
mod sentence;
mod talk;
