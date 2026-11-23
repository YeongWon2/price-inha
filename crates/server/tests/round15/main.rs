//! 8차(`docs/dev/modules/server.md` "8차 작업": 바깥 키 칸 등록 · 접속 키 다듬기)의 테스트.
//!
//! 잠긴 테스트 파일을 고치지 않으려고 따로 둔 최상위 테스트 바이너리다. 서버는 임시 보관 폴더 ·
//! `127.0.0.1:0`으로 띄우고, 자료 출처(DART · ECOS · FRED)와 GitHub은 로컬 가짜 서버로 흉내 낸다.
//! 시험의 키 값은 모두 가짜 글자다.

// 11차(Vite 이전): 옛 시험이 빌드한 화면(dist)에서 같은 뜻을 찾는 도우미.
mod common;
mod page;
#[path = "../support/refs.rs"]
mod screen_refs;
#[path = "../support/served.rs"]
mod screen_served;
mod slots;
