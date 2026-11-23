//! W5 6차(`docs/dev/modules/server.md` "6차 작업")의 테스트.
//!
//! 잠긴 테스트 파일을 고치지 않으려고 따로 둔 최상위 테스트 바이너리다. 서버는 임시 보관 폴더 ·
//! `127.0.0.1:0`으로 띄우고, AI 키 확인 · 웹 푸시 · GitHub는 로컬 가짜 서버나 아무도 듣지 않는
//! 로컬 주소뿐이다. 실제 증권사 · AI 서버 · GitHub를 부르지 않는다.

// 11차(Vite 이전): 옛 시험이 빌드한 화면(dist)에서 같은 뜻을 찾는 도우미.
mod backup;
mod common;
mod keys;
mod restore;
#[path = "../support/refs.rs"]
mod screen_refs;
#[path = "../support/served.rs"]
mod screen_served;
