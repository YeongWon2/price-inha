//! 메신저 채널(텔레그램 · 슬랙)로 글을 주고받는 한 겹. 결정 24.
//!
//! 지시서: `docs/dev/modules/channel.md`. 원본 동작 근거: `docs/research/onboarding-messenger.md`
//! 2.4 · 2.5 · 5장 (다).
//!
//! - [`Channel`]: 채널 한 겹(확인 · 받기 · 보내기 · 한도). 1차 구현은 [`Telegram`](long polling).
//! - [`Gatekeeper`]: 보낸 사람 판정(허용 목록 · 페어링 코드). 상태는 [`PairingBook`] 포트로 받는다.
//! - [`OutText`]: 가린 글. [`Redactor`]를 거쳐야만 만들 수 있고, [`Channel::send`]는 이것만 받는다.
//! - [`parse_command`]: 허용된 사람의 글을 [`Command`]로 읽는다(실행하지 않는다).
//!
//! 이 모듈은 환경 변수를 읽지 않는다. 토큰 · 바깥 주소는 생성자 인자로 받는다.

mod channel;
mod clock;
mod command;
mod error;
mod model;
mod pairing;
mod redact;
mod split;
mod telegram;

pub use channel::Channel;
pub use clock::Clock;
pub use command::{Command, parse_command};
pub use error::{ApproveError, BookError, Error, RedactError};
pub use model::{ChatId, Cursor, Inbound, Limits, MsgRef, Polled, SenderId};
pub use pairing::{
    CODE_ALPHABET, CODE_LEN, Gatekeeper, IgnoreReason, IssueRecord, Judgement, MAX_ALLOWED,
    MAX_FAILED_APPROVALS, MAX_PENDING, PAIRING_LOCKOUT_SECS, PAIRING_RATE_LIMIT_SECS,
    PAIRING_TTL_SECS, PairingBook, PairingCode, PairingState, PendingRequest,
};
pub use redact::{OutText, Redactor};
pub use split::split_text;
pub use telegram::{
    BotToken, TELEGRAM_API_BASE, TELEGRAM_CHANNEL, TELEGRAM_TEXT_MAX, Telegram, TelegramConfig,
};
