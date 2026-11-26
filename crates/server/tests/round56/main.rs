//! 기능 H(backlog "하네스 36차" · "기능 E · 대조 필요")의 Rust 쪽 시험: 키 화면(`/keys`)을 그 컴퓨터에서 세션 없이 연다.
//!
//! - `bench`: 발판이 지금 구현에서 서는가(서버가 뜨고 · 그 컴퓨터 세션으로 키 화면이 열리고 · 다른 기기로 보이게
//!   바꿀 수 있다).
//! - `keys`: 그 컴퓨터(루프백 · 전달 머리글 없음)는 세션 없이 키 화면이 열리고 화면이 세션을 얻는다. 다른 기기 ·
//!   전달 머리글이 있는 요청은 지금처럼 세션(원격 세션)이 있어야 한다. 키 등록 API(`/api/setup/keys` ·
//!   `/api/remote/keys`)의 관문은 그대로다.
//!
//! 서버는 임시 보관 폴더 · `127.0.0.1:0` · 시연 모드로 띄운다. AI 키 확인은 로컬 가짜 서버, 나머지 바깥 주소는
//! 아무도 듣지 않는 로컬 주소다. 화면 쪽(세션을 언제 얻나)은 vitest(`tests/web/keys-session.test.ts`)에 있다.

mod bench;
mod common;
mod keys;
#[path = "../support/refs.rs"]
mod screen_refs;
#[path = "../support/served.rs"]
mod screen_served;
