//! 메신저 발판: 로컬 가짜 텔레그램 · **시연 아님** 조립(시연이면 메신저 루프가 돌지 않는다 — 39차 10번) · 토큰 등록 ·
//! 페어링. round42의 가짜 텔레그램을 본떴다(그 폴더는 고치지 않는다). AI 자리는 503으로 답하는 로컬 가짜 서버라
//! 담당의 답 턴은 실패로 끝난다(이 발판은 답이 아니라 모이기를 본다). 바깥으로 나가지 않는다.

use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use pi_channel::{CODE_ALPHABET, CODE_LEN};
use pi_store::Store;
use price_inha::{
    App, MESSENGER_PAIRING_APPROVE_PATH, MESSENGER_PAIRING_REPLY, MESSENGER_PATH,
    MESSENGER_TELEGRAM_PATH, MessengerWiring,
};
use serde_json::{Value, json};
use wiremock::matchers::any;
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

use crate::common::{Launched, Reply, STEP, get, post, session};

/// 시험용 봇 토큰(진짜 토큰처럼 보이지 않는다).
const TOKEN: &str = "SIHEOM-TG-ROUND66-0001";

/// 가짜 텔레그램이 쥔 것: 내줄 갱신들 · 다음 번호.
struct TgState {
    updates: Mutex<Vec<Value>>,
    next_update: AtomicI64,
    next_message: AtomicI64,
}

/// 요청의 인자 하나(JSON 몸통 → 쿼리 차례로 찾는다).
fn param(req: &Request, name: &str) -> Option<String> {
    if let Ok(body) = serde_json::from_slice::<Value>(&req.body)
        && let Some(value) = body.get(name)
    {
        return Some(match value {
            Value::String(s) => s.clone(),
            other => other.to_string(),
        });
    }
    req.url
        .query_pairs()
        .find(|(k, _)| k == name)
        .map(|(_, v)| v.into_owned())
}

fn tg_ok(result: Value) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(json!({ "ok": true, "result": result }))
}

struct TgResponder {
    state: Arc<TgState>,
}

impl Respond for TgResponder {
    fn respond(&self, req: &Request) -> ResponseTemplate {
        let prefix = format!("/bot{TOKEN}/");
        let Some(method) = req.url.path().strip_prefix(&prefix) else {
            return ResponseTemplate::new(401).set_body_json(
                json!({"ok": false, "error_code": 401, "description": "Unauthorized"}),
            );
        };
        match method {
            "getMe" => tg_ok(json!({
                "id": 4_000_066, "is_bot": true, "first_name": "시험 봇", "username": "siheom_round66_bot"
            })),
            "getUpdates" => {
                let offset = param(req, "offset").and_then(|o| o.parse::<i64>().ok());
                let list: Vec<Value> = self
                    .state
                    .updates
                    .lock()
                    .unwrap()
                    .iter()
                    .filter(|u| offset.is_none_or(|o| u["update_id"].as_i64().unwrap() >= o))
                    .cloned()
                    .collect();
                if list.is_empty() {
                    tg_ok(json!([])).set_delay(Duration::from_millis(150))
                } else {
                    tg_ok(json!(list))
                }
            }
            "sendMessage" => {
                let id = self.state.next_message.fetch_add(1, Ordering::SeqCst);
                let chat = param(req, "chat_id").unwrap_or_default();
                tg_ok(json!({
                    "message_id": id,
                    "chat": { "id": chat.parse::<i64>().unwrap_or(0), "type": "private" },
                    "date": 1_790_000_000,
                    "text": param(req, "text").unwrap_or_default()
                }))
            }
            _ => ResponseTemplate::new(404)
                .set_body_json(json!({"ok": false, "error_code": 404, "description": "Not Found"})),
        }
    }
}

/// 로컬 가짜 텔레그램 서버.
pub struct FakeTelegram {
    server: MockServer,
    state: Arc<TgState>,
}

impl FakeTelegram {
    pub async fn start() -> FakeTelegram {
        let server = MockServer::start().await;
        let state = Arc::new(TgState {
            updates: Mutex::new(Vec::new()),
            next_update: AtomicI64::new(5000),
            next_message: AtomicI64::new(100),
        });
        Mock::given(any())
            .respond_with(TgResponder {
                state: Arc::clone(&state),
            })
            .mount(&server)
            .await;
        FakeTelegram { server, state }
    }

    /// 1:1 대화(대화 번호 = 보낸 사람 번호)에 글 하나를 받게 둔다.
    pub fn push_dm(&self, sender: i64, text: &str) {
        let id = self.state.next_update.fetch_add(1, Ordering::SeqCst);
        let update = json!({
            "update_id": id,
            "message": {
                "message_id": id + 100,
                "from": { "id": sender, "is_bot": false, "first_name": "시험" },
                "chat": { "id": sender, "type": "private" },
                "date": 1_790_000_000 + id,
                "text": text
            }
        });
        self.state.updates.lock().unwrap().push(update);
    }

    /// 그 대화로 보낸 글들.
    async fn sent_to(&self, chat: i64) -> Vec<String> {
        let chat = chat.to_string();
        self.server
            .received_requests()
            .await
            .unwrap_or_default()
            .iter()
            .filter(|r| r.url.path().ends_with("/sendMessage"))
            .filter(|r| param(r, "chat_id").is_some_and(|c| c == chat))
            .filter_map(|r| param(r, "text"))
            .collect()
    }

    /// 그 대화로 보낸 글 가운데 조건에 맞는 것이 올 때까지 기다려 돌려준다.
    async fn wait_sent(&self, chat: i64, what: &str, check: impl Fn(&str) -> bool) -> String {
        let deadline = Instant::now() + STEP;
        loop {
            if let Some(text) = self.sent_to(chat).await.into_iter().find(|t| check(t)) {
                return text;
            }
            assert!(Instant::now() < deadline, "시간 안에 오지 않았다: {what}");
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }
}

/// 페어링 답 글에서 코드를 꺼낸다.
fn code_in(text: &str) -> Option<String> {
    let rest = text.split(MESSENGER_PAIRING_REPLY).nth(1)?;
    let code = rest.split_whitespace().next()?;
    let ok = code.chars().count() == CODE_LEN && code.chars().all(|c| CODE_ALPHABET.contains(c));
    ok.then(|| code.to_string())
}

/// 메신저를 그 가짜 텔레그램으로 이은 서버(시연 아님).
pub struct MessengerBench {
    launched: Launched,
}

impl MessengerBench {
    /// 띄우고 토큰을 등록해 받는 중이 될 때까지 기다린 뒤, 그 사람을 페어링한다.
    pub async fn linked(tg: &FakeTelegram, owner: i64) -> MessengerBench {
        let wiring = MessengerWiring {
            telegram_api_base: tg.server.uri(),
            poll_timeout_secs: 1,
            request_timeout: Duration::from_secs(3),
            backoff_initial: Duration::from_secs(3),
            backoff_max: Duration::from_secs(7),
            send_attempts: 3,
            talk_batch: Duration::from_secs(2),
        };
        let bench = MessengerBench {
            launched: Launched::start(false, Some(wiring)).await,
        };
        let reply = bench
            .post(MESSENGER_TELEGRAM_PATH, &json!({ "token": TOKEN }))
            .await;
        assert_eq!(reply.status, 200, "토큰 등록: {}", reply.text);
        bench.wait_polling().await;
        tg.push_dm(owner, "안녕하세요");
        let answer = tg
            .wait_sent(owner, "페어링 코드 답", |t| code_in(t).is_some())
            .await;
        let code = code_in(&answer).unwrap();
        let approved = bench
            .post(MESSENGER_PAIRING_APPROVE_PATH, &json!({ "code": code }))
            .await;
        assert_eq!(approved.status, 200, "맞는 코드로 승인: {}", approved.text);
        bench
    }

    fn app(&self) -> &App {
        &self.launched.app
    }

    pub fn store(&self) -> Store {
        self.app().store()
    }

    async fn post(&self, path: &str, body: &Value) -> Reply {
        let addr = self.app().local_addr();
        let cookie = session(addr).await;
        post(addr, path, &cookie, body).await
    }

    /// 텔레그램이 받는 중이 될 때까지 기다린다.
    async fn wait_polling(&self) {
        let deadline = Instant::now() + STEP;
        loop {
            let addr = self.app().local_addr();
            let cookie = session(addr).await;
            let status = get(addr, MESSENGER_PATH, Some(&cookie)).await.body;
            if status["telegram"]["state"] == json!("polling") {
                return;
            }
            assert!(Instant::now() < deadline, "받는 중이 되지 않았다: {status}");
            tokio::time::sleep(Duration::from_millis(30)).await;
        }
    }

    pub async fn close(self) {
        tokio::time::timeout(STEP, self.launched.app.shutdown())
            .await
            .expect("시간 안에 꺼져야 한다")
            .expect("끄기가 성공해야 한다");
    }
}
