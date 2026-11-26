//! 다듬기 묶음 E 시험(docs/dev/backlog.md "다듬기 후보" — 쓰는 데 걸리는 것부터). 바깥(증권사 · AI · 자료 · 네이버 ·
//! 텔레그램)은 로컬 가짜 서버뿐이다.
//!
//! - `demo_symbols`: 1번 — 키도 종목도 없는 시연은 시연용 기본 종목을 조립 값으로 지켜본다(파일에 쓰지 않는다, E56).
//!   설정 상태의 `symbols_source`는 `"demo"`.

mod common;
mod demo_symbols;
