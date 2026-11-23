//! 11차 Vite 이전(`docs/dev/modules/server.md` "11차 Vite 이전")의 Rust 쪽 시험.
//!
//! 잠긴 테스트 파일을 고치지 않으려고 따로 둔 최상위 테스트 바이너리다. 서버는 임시 보관 폴더 ·
//! `127.0.0.1:0` · 시연 모드로 띄우고, 바깥(증권사 · AI)은 아무도 듣지 않는 로컬 주소로 둔다.
//! 실제 증권사 · AI 서버를 부르지 않는다. 화면 동작의 시험(vitest)은 `tests/web/*.test.ts`에 있다.
//!
//! - `pages`: 3번 · 5번(각 페이지 경로가 200 · 맞는 내용 형식 · CSP · 캐시, 내주는 HTML이 빌드 결과 그대로).
//! - `dist`: 1번 · 3번 · 5번(dist의 모든 파일이 경로로 닿는다, 해시 붙은 묶음이 Vite 빌드에서 온다,
//!   서비스 워커 · manifest는 해시 없는 고정 이름, three 라이선스 글이 dist에 있다).
//! - `source`: 3번(server가 더는 `pi_office::web_assets()` · `pi_notify::phone_asset(s)` ·
//!   `web/*.html` 직접 include를 쓰지 않는다) — server 소스 글을 읽어 본다.

mod common;
mod dist;
mod pages;
mod source;
// 빌드 결과(dist)를 읽는 도우미(옛 시험과 함께 쓴다).
#[path = "../support/built.rs"]
mod screen_built;
#[path = "../support/dist.rs"]
mod screen_dist;
#[path = "../support/files.rs"]
mod screen_files;
#[path = "../support/refs.rs"]
mod screen_refs;
