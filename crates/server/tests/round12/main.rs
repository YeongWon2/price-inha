//! W5 마지막 차수(`docs/dev/modules/server.md` "4차 후보" · "후보 일괄 처리 때 다른 모듈에서 넘어온 것")의 테스트.
//!
//! 잠긴 테스트 파일을 고치지 않으려고 따로 둔 최상위 테스트 바이너리다. 서버는 임시 보관 폴더 ·
//! `127.0.0.1:0`으로 띄우고, AI는 시연용 가짜 공급자, 웹 푸시 · GitHub · 자료는 로컬 가짜 서버뿐이다.
//! 실제 증권사 · AI 서버 · GitHub를 부르지 않는다.

// 11차(Vite 이전): 옛 시험이 빌드한 화면(dist)에서 같은 뜻을 찾는 도우미.
mod backup;
mod common;
mod day;
mod order;
#[path = "../support/screen.rs"]
mod screen;
mod stop;
mod tools;
