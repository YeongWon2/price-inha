//! 시험 도우미: 가짜 텔레그램 서버 · 시험용 장부 · 가리개 · 시각.

use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use chrono::{DateTime, TimeDelta, Utc};
use pi_channel::{
    BookError, BotToken, ChatId, Inbound, OutText, PairingBook, PairingState, RedactError,
    Redactor, SenderId, Telegram, TelegramConfig,
};
use serde_json::{Value, json};
use url::Url;
use wiremock::{Request, Respond, ResponseTemplate};

/// 진짜 토큰처럼 보이지 않는 시험용 글자.
pub const TOKEN: &str = "SIHEOM-TG-TOKEN-0001";
/// 시험용 비밀 값(가리개가 가린다).
pub const SECRET: &str = "SIHEOM-SECRET-0001";
/// 가짜 봇 이름.
pub const BOT: &str = "siheom_bot";

/// 가짜 서버의 메서드 경로: `/bot{토큰}/{메서드}`.
pub fn bot_path(method: &str) -> String {
    format!("/bot{TOKEN}/{method}")
}

/// `base`를 보는 어댑터. long polling 기다림은 1초(가짜 서버는 바로 답한다).
pub fn telegram_at(base: &str) -> Telegram {
    let config = TelegramConfig {
        api_base: Url::parse(base).unwrap(),
        token: BotToken::new(TOKEN.to_string()),
        poll_timeout_secs: 1,
        request_timeout: Duration::from_secs(5),
    };
    Telegram::new(config).unwrap()
}

/// 요청의 인자 하나. 쿼리 → JSON 본문 → 폼 본문 차례로 찾는다(텔레그램은 셋 다 받는다).
pub fn param(req: &Request, name: &str) -> Option<String> {
    if let Some((_, v)) = req.url.query_pairs().find(|(k, _)| k == name) {
        return Some(v.into_owned());
    }
    if let Ok(body) = serde_json::from_slice::<Value>(&req.body) {
        return match body.get(name)? {
            Value::String(s) => Some(s.clone()),
            other => Some(other.to_string()),
        };
    }
    url::form_urlencoded::parse(&req.body)
        .find(|(k, _)| k == name)
        .map(|(_, v)| v.into_owned())
}

/// 텔레그램 성공 응답.
pub fn ok(result: Value) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(json!({ "ok": true, "result": result }))
}

/// 텔레그램 실패 응답.
pub fn fail(status: u16, description: &str) -> ResponseTemplate {
    ResponseTemplate::new(status).set_body_json(json!({
        "ok": false, "error_code": status, "description": description
    }))
}

/// 글 갱신 하나(`message`에 `text`).
pub fn text_update(update_id: i64, sender: i64, chat: i64, chat_type: &str, text: &str) -> Value {
    json!({
        "update_id": update_id,
        "message": {
            "message_id": update_id + 100,
            "from": { "id": sender, "is_bot": false, "first_name": "시험" },
            "chat": { "id": chat, "type": chat_type },
            "date": 1_790_000_000 + update_id,
            "text": text
        }
    })
}

/// `sendMessage`에 차례로 100, 101, … 번 글 번호를 주는 가짜 응답.
pub struct Numbered(pub AtomicI64);

impl Numbered {
    pub fn from(first: i64) -> Numbered {
        Numbered(AtomicI64::new(first))
    }
}

impl Respond for Numbered {
    fn respond(&self, req: &Request) -> ResponseTemplate {
        let id = self.0.fetch_add(1, Ordering::SeqCst);
        let chat = param(req, "chat_id").unwrap_or_default();
        ok(json!({
            "message_id": id,
            "chat": { "id": chat.parse::<i64>().unwrap_or(0), "type": "private" },
            "date": 1_790_000_000,
            "text": param(req, "text").unwrap_or_default()
        }))
    }
}

/// 받은 요청들의 `text` 인자(차례대로).
pub fn sent_texts(reqs: &[Request]) -> Vec<String> {
    reqs.iter()
        .map(|r| param(r, "text").unwrap_or_default())
        .collect()
}

/// 시험용 페어링 장부(메모리).
#[derive(Default)]
pub struct TestBook(Mutex<PairingState>);

impl TestBook {
    pub fn new() -> Arc<TestBook> {
        Arc::new(TestBook::default())
    }

    pub fn state(&self) -> PairingState {
        self.0.lock().unwrap().clone()
    }

    pub fn set(&self, state: PairingState) {
        *self.0.lock().unwrap() = state;
    }
}

impl PairingBook for TestBook {
    fn load(&self) -> Result<PairingState, BookError> {
        Ok(self.state())
    }

    fn save(&self, state: &PairingState) -> Result<(), BookError> {
        self.set(state.clone());
        Ok(())
    }
}

/// 기준 시각.
pub fn t0() -> DateTime<Utc> {
    DateTime::from_timestamp(1_790_000_000, 0).unwrap()
}

/// 기준 시각 + 분.
pub fn at_min(minutes: i64) -> DateTime<Utc> {
    t0() + TimeDelta::minutes(minutes)
}

pub fn sender(id: &str) -> SenderId {
    SenderId(id.to_string())
}

/// `id`가 보낸 1:1 글.
pub fn dm(id: &str) -> Inbound {
    Inbound {
        channel: "telegram".to_string(),
        sender: sender(id),
        chat: ChatId(id.to_string()),
        is_direct: true,
        text: "안녕".to_string(),
        at: t0(),
        message_id: 1,
    }
}

/// `id`가 그룹에 쓴 글.
pub fn group(id: &str) -> Inbound {
    Inbound {
        chat: ChatId("-100200".to_string()),
        is_direct: false,
        ..dm(id)
    }
}

/// 아무것도 바꾸지 않는 가리개.
pub struct PassRedactor;

impl Redactor for PassRedactor {
    fn redact(&self, text: &str) -> Result<String, RedactError> {
        Ok(text.to_string())
    }
}

/// [`SECRET`]을 `[가림]`으로 바꾸는 가리개.
pub struct MaskRedactor;

impl Redactor for MaskRedactor {
    fn redact(&self, text: &str) -> Result<String, RedactError> {
        Ok(text.replace(SECRET, "[가림]"))
    }
}

/// 늘 실패하는 가리개.
pub struct FailRedactor;

impl Redactor for FailRedactor {
    fn redact(&self, _text: &str) -> Result<String, RedactError> {
        Err(RedactError)
    }
}

/// 가리개를 거친(아무것도 바꾸지 않은) 보낼 글.
pub fn out(text: &str) -> OutText {
    OutText::redacted(text, &PassRedactor).unwrap()
}
