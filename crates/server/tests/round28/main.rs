//! 26차(`docs/dev/modules/server.md` "26차 공통 메뉴 · 디자인 토큰 · 관리 화면 틀")의 Rust 쪽 시험.
//!
//! - `gate`: 4번(관리 화면 넷 `/account/` · `/templates/` · `/rules/` · `/history/`이 설정 · 사무실 화면과
//!   같은 관문을 받는다 — 그 컴퓨터는 세션 없이 열리고, 다른 기기는 원격 세션이 없으면 401 · 있으면 200 HTML,
//!   화면이 부르는 묶음도 열린다).
//! - `bundle`: 6번(N21 · V77 — 관리 화면 묶음에 three가 실리지 않는다, 첫 화면 JS + CSS가 60KB 안).
//!
//! 서버는 임시 보관 폴더 · `127.0.0.1:0` · 시연 모드로 띄우고, 바깥(증권사 · AI)은 아무도 듣지 않는 로컬 주소다.
//! 화면 동작(메뉴 · 토큰 · 화면 틀)의 시험은 vitest(`tests/web/*.test.ts`)에 있다.

mod bundle;
mod common;
mod gate;
#[path = "../support/built.rs"]
mod screen_built;
#[path = "../support/dist.rs"]
mod screen_dist;
#[path = "../support/refs.rs"]
mod screen_refs;
