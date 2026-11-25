//! 텔레그램 어댑터(V87). `getMe` · `getUpdates`(long polling) · `sendMessage`.
//!
//! 바깥 주소는 [`TelegramConfig::api_base`]로 받는다(시험은 로컬 가짜 서버). 요청 주소는
//! `{api_base}/bot{토큰}/{메서드}`다. 토큰이 주소 경로에 들므로 오류 · 기록 어디에도 주소를 통째로
//! 찍지 않는다(토큰 자리를 가린 글만).

use std::fmt;
use std::time::Duration;

use async_trait::async_trait;
use chrono::DateTime;
use serde::Deserialize;
use serde_json::{Value, json};
use url::Url;
use zeroize::Zeroizing;

use crate::{Channel, ChatId, Cursor, Error, Inbound, Limits, MsgRef, OutText, Polled, SenderId};

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
    client: reqwest::Client,
}

impl fmt::Debug for Telegram {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Telegram")
            .field("token", &self.config.token)
            .finish_non_exhaustive()
    }
}

/// 429 응답에 `parameters.retry_after`가 없을 때 기다릴 초.
const DEFAULT_RETRY_AFTER_SECS: u64 = 5;

/// Bot API 응답 봉투(`ok` · `result` · `description` · `error_code` · `parameters`).
#[derive(Deserialize)]
struct Envelope {
    ok: bool,
    #[serde(default)]
    result: Option<Value>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    error_code: Option<i64>,
    #[serde(default)]
    parameters: Option<ResponseParameters>,
}

#[derive(Deserialize)]
struct ResponseParameters {
    #[serde(default)]
    retry_after: Option<u64>,
}

impl Telegram {
    /// 만든다. HTTP 클라이언트를 만들 수 없으면 [`Error::Config`].
    pub fn new(config: TelegramConfig) -> Result<Telegram, Error> {
        let client = reqwest::Client::builder()
            .timeout(config.request_timeout)
            .redirect(reqwest::redirect::Policy::none())
            .no_proxy()
            .build()
            .map_err(|e| Error::Config(e.without_url().to_string()))?;
        let telegram = Telegram { config, client };
        // 만들 때 한 번 주소를 조립해 본다(주소가 틀렸으면 여기서 알린다).
        telegram.method_url("getMe")?;
        Ok(telegram)
    }

    /// `{api_base}/bot{토큰}/{메서드}`. 오류 글에 주소를 넣지 않는다.
    fn method_url(&self, method: &str) -> Result<Url, Error> {
        let base = self.config.api_base.as_str().trim_end_matches('/');
        let raw = Zeroizing::new(format!(
            "{base}/bot{}/{method}",
            self.config.token.0.as_str()
        ));
        Url::parse(&raw).map_err(|_| Error::Config("텔레그램 주소를 만들 수 없다".to_owned()))
    }

    /// 글에서 토큰을 가린다(바깥에서 온 글이 주소를 되돌려 줄 때를 막는다).
    fn scrub(&self, text: &str) -> String {
        let token = self.config.token.0.as_str();
        if token.is_empty() {
            text.to_owned()
        } else {
            text.replace(token, "***")
        }
    }

    /// 연결 · 읽기 오류. reqwest 오류의 주소(토큰이 든)를 떼어 낸다.
    fn transport(&self, e: reqwest::Error) -> Error {
        Error::Transport(self.scrub(&e.without_url().to_string()))
    }

    /// 메서드 하나를 부르고 `result`를 돌려준다. 다시 하지 않는다.
    async fn call(&self, method: &str, body: Value, extra_wait: Duration) -> Result<Value, Error> {
        let url = self.method_url(method)?;
        let response = self
            .client
            .post(url)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(body.to_string())
            .timeout(self.config.request_timeout.saturating_add(extra_wait))
            .send()
            .await
            .map_err(|e| self.transport(e))?;
        let status = response.status().as_u16();
        let bytes = response.bytes().await.map_err(|e| self.transport(e))?;
        let outcome = self.decode(method, status, &bytes);
        if let Err(err) = &outcome {
            tracing::debug!(method, status, error = %err, "텔레그램 요청이 실패했다");
        }
        outcome
    }

    /// 응답 상태와 본문을 `result` 또는 종류 있는 오류로 바꾼다.
    fn decode(&self, method: &str, status: u16, bytes: &[u8]) -> Result<Value, Error> {
        let envelope = serde_json::from_slice::<Envelope>(bytes).ok();
        let code = envelope.as_ref().and_then(|e| e.error_code);
        if status == 401 || code == Some(401) {
            return Err(Error::Unauthorized);
        }
        if status == 429 || code == Some(429) {
            let retry_after_secs = envelope
                .as_ref()
                .and_then(|e| e.parameters.as_ref())
                .and_then(|p| p.retry_after)
                .unwrap_or(DEFAULT_RETRY_AFTER_SECS);
            return Err(Error::RateLimited { retry_after_secs });
        }
        let success = (200..300).contains(&status);
        let Some(envelope) = envelope else {
            return Err(if success {
                Error::Malformed(format!("{method}: 응답이 Bot API JSON이 아니다"))
            } else {
                Error::Api {
                    status,
                    description: String::new(),
                }
            });
        };
        if !success || !envelope.ok {
            let status = if success {
                code.and_then(|c| u16::try_from(c).ok()).unwrap_or(status)
            } else {
                status
            };
            let description = self.scrub(envelope.description.as_deref().unwrap_or_default());
            return Err(Error::Api {
                status,
                description,
            });
        }
        envelope
            .result
            .ok_or_else(|| Error::Malformed(format!("{method}: 응답에 result가 없다")))
    }
}

/// 갱신 하나에서 받은 글을 읽는다. `message`에 `text`가 없거나 칸이 모자라면 `None`.
fn read_update(update: &Value) -> Option<Inbound> {
    let message = update.get("message")?;
    let text = message.get("text")?.as_str()?;
    let sender = message.get("from")?.get("id")?.as_i64()?;
    let chat = message.get("chat")?;
    let chat_id = chat.get("id")?.as_i64()?;
    let is_direct = chat.get("type")?.as_str()? == "private";
    let at = DateTime::from_timestamp(message.get("date")?.as_i64()?, 0)?;
    let message_id = message.get("message_id")?.as_i64()?;
    Some(Inbound {
        channel: TELEGRAM_CHANNEL.to_owned(),
        sender: SenderId(sender.to_string()),
        chat: ChatId(chat_id.to_string()),
        is_direct,
        text: text.to_owned(),
        at,
        message_id,
    })
}

#[async_trait]
impl Channel for Telegram {
    fn name(&self) -> &'static str {
        TELEGRAM_CHANNEL
    }

    /// `getMe`. `result.username`을 돌려준다. 401은 [`Error::Unauthorized`].
    async fn check(&self) -> Result<String, Error> {
        let result = self.call("getMe", json!({}), Duration::ZERO).await?;
        result
            .get("username")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| Error::Malformed("getMe: username이 없다".to_owned()))
    }

    /// `getUpdates`(`offset` = `cursor`, `timeout` = [`TelegramConfig::poll_timeout_secs`]).
    /// `message`에 `text`가 있는 갱신만 [`crate::Inbound`]로 돌려주고, `next`는 받은 갱신 가운데 가장 큰
    /// `update_id` + 1이다. 429는 [`Error::RateLimited`](`parameters.retry_after`), 401은
    /// [`Error::Unauthorized`], 그 밖의 `ok: false`는 [`Error::Api`], 읽을 수 없는 응답은
    /// [`Error::Malformed`]. 다시 하지 않는다.
    async fn poll(&self, cursor: Option<Cursor>) -> Result<Polled, Error> {
        let mut body = json!({ "timeout": self.config.poll_timeout_secs });
        if let Some(Cursor(offset)) = cursor {
            body["offset"] = json!(offset);
        }
        let wait = Duration::from_secs(u64::from(self.config.poll_timeout_secs));
        let result = self.call("getUpdates", body, wait).await?;
        let updates = result
            .as_array()
            .ok_or_else(|| Error::Malformed("getUpdates: result가 목록이 아니다".to_owned()))?;
        let mut items = Vec::new();
        let mut next = cursor;
        for update in updates {
            let id = update
                .get("update_id")
                .and_then(Value::as_i64)
                .ok_or_else(|| Error::Malformed("getUpdates: update_id가 없다".to_owned()))?;
            let after = Cursor(id.saturating_add(1));
            next = Some(next.map_or(after, |c| Cursor(c.0.max(after.0))));
            if let Some(inbound) = read_update(update) {
                items.push(inbound);
            }
        }
        Ok(Polled { items, next })
    }

    /// `sendMessage`(`chat_id`, `text`). 서식(parse_mode)을 쓰지 않는다. [`TELEGRAM_TEXT_MAX`]를 넘으면
    /// [`crate::split_text`]로 나눠 차례대로 보낸다.
    async fn send(&self, to: &ChatId, text: &OutText) -> Result<Vec<MsgRef>, Error> {
        let mut refs = Vec::new();
        for piece in crate::split_text(text.as_str(), TELEGRAM_TEXT_MAX) {
            let body = json!({ "chat_id": to.0, "text": piece });
            let result = self.call("sendMessage", body, Duration::ZERO).await?;
            let message_id = result
                .get("message_id")
                .and_then(Value::as_i64)
                .ok_or_else(|| Error::Malformed("sendMessage: message_id가 없다".to_owned()))?;
            refs.push(MsgRef {
                chat: to.clone(),
                message_id,
            });
        }
        Ok(refs)
    }

    fn limits(&self) -> Limits {
        Limits {
            text_max: TELEGRAM_TEXT_MAX,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{BotToken, Telegram, TelegramConfig};
    use crate::Error;
    use std::time::Duration;
    use url::Url;

    const TOKEN: &str = "SIHEOM-UNIT-TOKEN";

    fn telegram(base: &str) -> Telegram {
        Telegram::new(TelegramConfig {
            api_base: Url::parse(base).unwrap(),
            token: BotToken::new(TOKEN.to_owned()),
            poll_timeout_secs: 1,
            request_timeout: Duration::from_secs(1),
        })
        .unwrap()
    }

    #[test]
    fn 주소는_기준_주소의_경로_뒤에_붙는다() {
        let tg = telegram("http://127.0.0.1:9/proxy/");
        let url = tg.method_url("getMe").unwrap();
        assert_eq!(url.path(), format!("/proxy/bot{TOKEN}/getMe"));
    }

    #[test]
    fn 응답_설명에_든_토큰은_가린다() {
        let tg = telegram("http://127.0.0.1:9");
        let body = format!(r#"{{"ok":false,"error_code":400,"description":"bad /bot{TOKEN}/x"}}"#);
        let err = tg.decode("sendMessage", 400, body.as_bytes()).unwrap_err();
        assert!(matches!(err, Error::Api { status: 400, .. }));
        assert!(!err.to_string().contains(TOKEN), "{err}");
    }

    #[test]
    fn retry_after_가_없는_429는_기본_간격이다() {
        let tg = telegram("http://127.0.0.1:9");
        let err = tg.decode("getUpdates", 429, b"not json").unwrap_err();
        assert_eq!(
            err,
            Error::RateLimited {
                retry_after_secs: super::DEFAULT_RETRY_AFTER_SECS
            }
        );
    }
}
