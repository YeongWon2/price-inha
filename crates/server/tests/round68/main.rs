//! 백로그 U(지난 묶음이 "못 함"으로 남긴 것)의 server 시험.
//!
//! - `bench`: 발판이 지금 구현에서 서는가(시연이 아닌 모드 · 뉴스 구독).
//! - `doctor_read_only`: `doctor`가 저장소 · 백업 상태를 읽기 전용 손잡이로 본다 — 판 11 파일이 바뀌지 않고, 쓰는
//!   손잡이(도는 서버)와 잠금 다툼이 없다.
//! - `seen_return`: 깨우기 엔진이 상한으로 버린 뉴스는 "본 것"에서 빠져 다음 가져오기에서 다시 넘어간다.
//! - `live_restart`: 실전 템플릿 멈춤 → 다시 시작(새 템플릿 고르기)에서 구독이 멈췄다 다시 붙는다.
//! - `service_log`: macOS 서비스 정의는 표준 출력을 버리고 서버가 돌려 쓰는 기록 파일로 쓰게 하며, 켜지기 전 오류
//!   파일은 작은 상한으로 줄인다.
//!
//! 공용 발판은 round64(시연이 아닌 모드 · 로컬 가짜 시세 · AI · 자료 · 뉴스 서버)와 round65(서비스 재료 · 가짜
//! 바깥 명령 실행기)의 것을 그대로 쓴다(그 폴더들은 고치지 않는다). 실제 증권사 · AI · 자료 서버를 부르지 않고,
//! 계좌는 모의계좌뿐이다(실전중 상태는 저장소 상태로만 만든다).

#[path = "../round64/live.rs"]
pub mod live;

#[path = "../round65/common.rs"]
pub mod common;

mod bench;
mod doctor_read_only;
mod live_restart;
mod seen_return;
mod service_log;
