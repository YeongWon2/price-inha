//! W5 3차(`docs/dev/modules/server.md` "3차"): 검색 · 백업 · 사무실 · 폰 잇기(T16 · T19 · T20 · T21 · T22)의 테스트.
//!
//! 잠긴 테스트 파일을 고치지 않으려고 따로 둔 최상위 테스트 바이너리다. 서버는 임시 보관 폴더 ·
//! `127.0.0.1:0`으로 띄우고, AI는 시연용 가짜 공급자, 웹 푸시 · GitHub · 자료 · 토스증권은 로컬 가짜 서버뿐이다.
//! 실제 증권사 · AI 서버 · GitHub를 부르지 않는다.

// 11차(Vite 이전): 옛 시험이 빌드한 화면(dist)에서 같은 뜻을 찾는 도우미.
mod anomaly;
mod assets;
mod backup;
mod common;
mod knowledge;
mod lock;
mod office;
mod phone;
#[path = "../support/screen.rs"]
mod screen;
mod secrets;
