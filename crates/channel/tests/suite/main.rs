//! pi-channel 공개 API 시험. 지시서 `docs/dev/modules/channel.md` 1차 "시험으로 확인할 것".
//!
//! 로컬 가짜 텔레그램 서버(wiremock)만 쓴다. 실제 텔레그램 · 슬랙을 부르지 않는다.
//! 시각은 시험이 정한다. 오래 자는 시험이 없다.
//! 요청 · 응답 모양은 Telegram Bot API 공식 문서(Making requests · getMe · getUpdates ·
//! sendMessage · ResponseParameters)를 따른다.

mod command;
mod common;
mod logs;
mod pairing;
mod redact;
mod split;
mod telegram;
