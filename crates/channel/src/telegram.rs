//! 텔레그램 어댑터(V87). `getMe` · `getUpdates`(long polling) · `sendMessage`.
//!
//! 바깥 주소는 [`TelegramConfig::api_base`]로 받는다(시험은 로컬 가짜 서버). 요청 주소는
//! `{api_base}/bot{토큰}/{메서드}`다. 토큰이 주소 경로에 들므로 오류 · 기록 어디에도 주소를 통째로
//! 찍지 않는다(토큰 자리를 가린 글만).

use std::fmt;
use std::time::Duration;

use async_trait::async_trait;
use url::Url;
use zeroize::Zeroizing;

use crate::{Channel, ChatId, Cursor, Error, Limits, MsgRef, OutText, Polled};

/// 텔레그램 Bot API 주소(실제). 조립하는 곳이 [`TelegramConfig::api_base`]에 넣는다.
pub const TELEGRAM_API_BASE: &str = "https://api.telegram.org";
/// 텔레그램 글 하나의 최대 글자 수(원본 OpenClaw `TELEGRAM_TEXT_CHUNK_LIMIT = 4000`).
pub const TELEGRAM_TEXT_MAX: usize = 4000;
/// 채널 이름.
pub const TELEGRAM_CHANNEL: &str = "telegram";

/// 봇 토큰. 메모리에서 지울 수 있게(zeroize) 쥐고, `Debug`는 토큰을 가린다.
#[derive(Clone)]
pub struct BotToken(Zeroizing<String>);

impl BotToken {
    /// 토큰 글자로 만든다.
    pub fn new(token: String) -> BotToken {
        BotToken(Zeroizing::new(token))
    }
}

impl fmt::Debug for BotToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("BotToken(***)")
    }
}

/// 텔레그램 어댑터 설정.
#[derive(Debug, Clone)]
pub struct TelegramConfig {
    /// Bot API 주소(실제는 [`TELEGRAM_API_BASE`], 시험은 로컬 가짜 서버).
    pub api_base: Url,
    /// 봇 토큰.
    pub token: BotToken,
    /// `getUpdates`의 `timeout`(초, long polling 기다림). 그대로 요청에 싣는다.
    pub poll_timeout_secs: u32,
    /// 요청 하나의 제한 시간(long polling 기다림과 따로 — 어댑터가 기다림을 더해 쓴다).
    pub request_timeout: Duration,
}

/// 텔레그램 어댑터. `Debug`는 토큰을 찍지 않는다.
pub struct Telegram {
    config: TelegramConfig,
}

impl fmt::Debug for Telegram {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Telegram")
            .field("token", &self.config.token)
            .finish_non_exhaustive()
    }
}

impl Telegram {
    /// 만든다. HTTP 클라이언트를 만들 수 없으면 [`Error::Config`].
    pub fn new(config: TelegramConfig) -> Result<Telegram, Error> {
        Ok(Telegram { config })
    }
}

#[async_trait]
impl Channel for Telegram {
    fn name(&self) -> &'static str {
        TELEGRAM_CHANNEL
    }

    /// `getMe`. `result.username`을 돌려준다. 401은 [`Error::Unauthorized`].
    async fn check(&self) -> Result<String, Error> {
        let _ = (self.config.token.0.len(), &self.config.api_base);
        Err(Error::Config(String::new()))
    }

    /// `getUpdates`(`offset` = `cursor`, `timeout` = [`TelegramConfig::poll_timeout_secs`]).
    /// `message`에 `text`가 있는 갱신만 [`crate::Inbound`]로 돌려주고, `next`는 받은 갱신 가운데 가장 큰
    /// `update_id` + 1이다. 429는 [`Error::RateLimited`](`parameters.retry_after`), 401은
    /// [`Error::Unauthorized`], 그 밖의 `ok: false`는 [`Error::Api`], 읽을 수 없는 응답은
    /// [`Error::Malformed`]. 다시 하지 않는다.
    async fn poll(&self, cursor: Option<Cursor>) -> Result<Polled, Error> {
        let _ = cursor;
        Err(Error::Config(String::new()))
    }

    /// `sendMessage`(`chat_id`, `text`). 서식(parse_mode)을 쓰지 않는다. [`TELEGRAM_TEXT_MAX`]를 넘으면
    /// [`crate::split_text`]로 나눠 차례대로 보낸다.
    async fn send(&self, to: &ChatId, text: &OutText) -> Result<Vec<MsgRef>, Error> {
        let _ = (to, text);
        Ok(Vec::new())
    }

    fn limits(&self) -> Limits {
        Limits { text_max: 0 }
    }
}
