//! 다듬기 묶음 E 시험(docs/dev/backlog.md "다듬기 후보" — 쓰는 데 걸리는 것부터). 바깥(증권사 · AI · 자료 · 네이버 ·
//! 텔레그램)은 로컬 가짜 서버뿐이다.
//!
//! - `demo_symbols`: 1번 — 키도 종목도 없는 시연은 시연용 기본 종목을 조립 값으로 지켜본다(파일에 쓰지 않는다, E56).
//!   설정 상태의 `symbols_source`는 `"demo"`.
//! - `data_key`: 2번 — 옛 자료 키 등록은 `retired_item`으로 거절(확인기를 부르지 않는다), 진단 · doctor 키 줄에서 뺀다.
//! - `demo_chat`: 4번 — 시연용 가짜 AI의 채팅 답은 표시 이름 · 물음 되받기 · "(시연)".
//! - `diagnosis`: 5번 — 시연 진단의 "AI 연결" 줄은 시연용 가짜 AI(고른 방식은 뒤에 그대로). 표의 줄바꿈은 vitest
//!   `setup-diag-wrap.test.ts`가 본다.

mod common;
mod data_key;
mod demo_chat;
mod demo_symbols;
mod diagnosis;
mod doctor_cli;
