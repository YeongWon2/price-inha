//! round42 시험(server 41차)이 함께 쓰는 도우미. round41(40차)의 발판(가짜 증권사 · 가짜 텔레그램 · 가짜 푸시 ·
//! 닿지 않는 백업 자리 · 시험 시계)을 본떠 두고(그 폴더는 고치지 않는다) **팀을 이어 논의 · 담당 답이 실제로 도는
//! 조립**을 더했다(round24의 시연이 아닌 조립 — 로컬 가짜 AI 서버 · 로컬 가짜 자료 서버).
//!
//! - [`Bench`]: 임시 보관 폴더 · `127.0.0.1:0` · **시연 아님**(시연이면 메신저 루프가 돌지 않는다 — 39차 10번). 필수 키는
//!   시험용 값으로 미리 등록한다. 토스 자리는 [`HYNIX`] 시세를 시계의 지금 시각으로 주는 로컬 가짜 증권사, Anthropic
//!   자리는 [`FakeAi`](대화 요청에 역할 · 턴별 정한 글로 답하고 받은 요청을 남긴다), 자료(DART · ECOS · FRED · 네이버)
//!   자리는 "자료 없음"으로 답하는 로컬 가짜 서버다. 팀(`TeamWiring`)을 이어 [`price_inha::App::launch_with_messenger`]로
//!   띄운다. 시계는 월요일 10시(한국 시각)에 서 있고 [`Bench::pass`]로만 옮긴다.
//! - 켜면 깨우기 엔진의 첫 tick이 뒤에서 하루 판단 AI를 부른다(하루 판단 글도 `office.talk`로 나온다). 그래서 "메신저로
//!   간 글"을 세는 시험은 글 내용으로 가려 센다.
//! - [`FakeAi`]의 답(round24 · 시연용 가짜 AI와 같은 규칙): 하루 판단 `{"plan": "trade"}` · 점수 70 · 팀장 판단은
//!   지켜보기(주문을 내지 않는다) · 논의 글은 scout가 `@strategy @risk`를 부르고 strategy · risk가 정한 글로 답한다 ·
//!   사무실 채팅(`[사무실 채팅]` — 멘션 답 · 공지 확인 답)은 [`FakeAi::script_chat`]으로 정한 글(없으면 기본 글).
//! - 폰 푸시: [`PushServer`]가 `/push/…`로 오는 POST에 201을 주고 받은 요청을 남긴다. 알림 종류는 푸시 머리글
//!   `Urgency`로 가린다(승인 요청 · 멈춤 `high` · 백업 실패 `normal`).
//! - [`FakeTelegram`]: 받을 갱신을 차례로 내주고(`offset` 이상만), 보낸 글(`sendMessage`)은 받은 요청으로 남는다.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use chrono::{DateTime, Datelike, NaiveDate, TimeZone, Utc, Weekday};
use pi_ai::AnthropicConfig;
use pi_approval::{ApprovalNumber, Fingerprint};
use pi_channel::{CODE_ALPHABET, CODE_LEN};
use pi_data::{CorpCode, DataConfig, NaverNewsConfig};
use pi_market::{
    Clock, Currency, Market, MarketCalendar, Quote, Session, SessionKind, StalePolicy, Symbol,
    TossConfig, TradingDay,
};
use pi_paper::{
    InitialCash, LimitFillPrice, MarketRules, MaxFill, MoneyRounding, OrderType, PaperConfig,
    ProceedsUsable, RoundingMode, SessionRules, Settlement, TickBand,
};
use pi_rules::{
    Funding, OrderDraft, ProductKind, Purpose, Rationale, RulesConfig, SourceStamp, SymbolStatus,
    TradeStage,
};
use pi_secrets::{
    Credentials, KeyKind, KeySlot, ProviderKeys, SecretStore, Verifier, VerifyFailure,
};
use pi_store::{
    AccountKind, Amount, Judgment, NewTrade, RoleScore, SourceRef, Store, TeamState, TradeId,
    TradeState, UnixMillis,
};
use price_inha::{
    AI_DAILY_LIMIT_KEY, App, BackupWiring, CalendarFn, Crew, DayPlan, Launch,
    MESSENGER_PAIRING_APPROVE_PATH, MESSENGER_PAIRING_REPLY, MESSENGER_PATH,
    MESSENGER_TELEGRAM_PATH, MessengerWiring, OrderRequest, PUSH_SUBSCRIBE_PATH, Placed, Reach,
    SETUP_AI_PATH, Settings, TEMPLATE_ADJUST_PATH, TEMPLATE_PATH, TeamWiring, TradingWiring,
    Upstreams, Wiring,
};
use rust_decimal::Decimal;
use serde_json::{Value, json};
use tempfile::TempDir;
use wiremock::matchers::{any, method, path, path_regex};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

/// 한 걸음을 기다리는 가장 긴 시간.
pub const STEP: Duration = Duration::from_secs(15);
/// "일어나지 않는다"를 지켜보는 짧은 시간.
pub const QUIET: Duration = Duration::from_millis(700);
/// 매매하는 종목(국내). 업종표에 "반도체"로 들어 있다.
pub const HYNIX: &str = "000660";
/// 시험용 봇 토큰(진짜 토큰처럼 보이지 않는다 — 텔레그램 토큰은 `숫자:글자` 모양이다).
pub const TOKEN: &str = "SIHEOM-TG-ROUND42-0001";
/// 가짜 봇 이름(getMe의 `username`).
pub const BOT: &str = "siheom_round42_bot";
/// 시험용 증권사 · AI 키(진짜 키처럼 보이지 않는다).
const CLIENT_ID: &str = "zz-round42-client-id-0001";
const CLIENT_SECRET: &str = "zz-round42-client-secret-0002";
pub const AI_KEY: &str = "zz-round42-ai-key-qwmv-0003";
/// 매매 근거의 진입가 · 손절값 · 목표값 · 주당 기대값(원).
const ENTRY: i64 = 100_000;
const STOP: i64 = 95_000;
const TARGET: i64 = 110_000;
const EDGE: i64 = 5_000;
/// 시험 조립의 물러서기: 3초부터 두 배씩, 7초에서 멈춘다(시험 시계는 실제로 자지 않는다).
pub const BACKOFF_INITIAL: Duration = Duration::from_secs(3);
pub const BACKOFF_MAX: Duration = Duration::from_secs(7);

fn d(text: &str) -> Decimal {
    text.parse().unwrap()
}

// ───────────── 시계 · 달력 ─────────────

/// 시험이 정하는 시계. 자는 것은 짧게만 자고, 1초 이상 자려 한 시간은 적어 둔다(같은 값은 한 번만).
pub struct TestClock {
    at: AtomicI64,
    slept: Mutex<Vec<Duration>>,
}

impl TestClock {
    fn at(time: DateTime<Utc>) -> Arc<Self> {
        Arc::new(Self {
            at: AtomicI64::new(time.timestamp_millis()),
            slept: Mutex::new(Vec::new()),
        })
    }

    fn pass(&self, seconds: i64) {
        self.at.fetch_add(seconds * 1000, Ordering::SeqCst);
    }
}

#[async_trait]
impl Clock for TestClock {
    fn now(&self) -> DateTime<Utc> {
        Utc.timestamp_millis_opt(self.at.load(Ordering::SeqCst))
            .unwrap()
    }

    async fn sleep(&self, duration: Duration) {
        if duration >= Duration::from_secs(1) {
            let mut slept = self.slept.lock().unwrap();
            if !slept.contains(&duration) {
                slept.push(duration);
            }
        }
        tokio::time::sleep(duration.min(Duration::from_millis(10))).await;
    }
}

/// 2026-11-16 월요일.
pub fn monday() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 11, 16).unwrap()
}

/// 그 날의 한국 시각.
pub fn kst(date: NaiveDate, hour: u32, minute: u32) -> DateTime<Utc> {
    let offset = chrono::FixedOffset::east_opt(9 * 3600).unwrap();
    offset
        .from_local_datetime(&date.and_hms_opt(hour, minute, 0).unwrap())
        .unwrap()
        .with_timezone(&Utc)
}

fn ms(at: DateTime<Utc>) -> UnixMillis {
    UnixMillis(at.timestamp_millis())
}

fn business_day(date: NaiveDate, forward: bool) -> NaiveDate {
    let mut next = date;
    loop {
        next = if forward {
            next.succ_opt().unwrap()
        } else {
            next.pred_opt().unwrap()
        };
        if !matches!(next.weekday(), Weekday::Sat | Weekday::Sun) {
            return next;
        }
    }
}

fn kr_day(date: NaiveDate) -> TradingDay {
    TradingDay {
        date,
        sessions: vec![Session {
            kind: SessionKind::Regular,
            start: kst(date, 9, 0),
            end: kst(date, 15, 30),
        }],
    }
}

fn kr_calendar(date: NaiveDate) -> MarketCalendar {
    MarketCalendar {
        market: Market::Kr,
        today: kr_day(date),
        previous_business_day: kr_day(business_day(date, false)),
        next_business_day: kr_day(business_day(date, true)),
    }
}

// ───────────── 가짜 텔레그램 ─────────────

/// 가짜 텔레그램이 쥔 것.
struct TgState {
    updates: Mutex<Vec<Value>>,
    next_update: AtomicI64,
    next_message: AtomicI64,
    revoked: AtomicBool,
    rate_limit_next: Mutex<Option<u64>>,
    fail_updates: AtomicU32,
    fail_send: AtomicBool,
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

fn tg_fail(status: u16, description: &str) -> ResponseTemplate {
    ResponseTemplate::new(status).set_body_json(json!({
        "ok": false, "error_code": status, "description": description
    }))
}

struct TgResponder {
    state: Arc<TgState>,
}

impl Respond for TgResponder {
    fn respond(&self, req: &Request) -> ResponseTemplate {
        let prefix = format!("/bot{TOKEN}/");
        let Some(method) = req.url.path().strip_prefix(&prefix) else {
            return tg_fail(401, "Unauthorized");
        };
        let state = &self.state;
        if state.revoked.load(Ordering::SeqCst) {
            return tg_fail(401, "Unauthorized");
        }
        match method {
            "getMe" => tg_ok(json!({
                "id": 4_000_041, "is_bot": true, "first_name": "시험 봇", "username": BOT
            })),
            "getUpdates" => {
                if let Some(secs) = state.rate_limit_next.lock().unwrap().take() {
                    return ResponseTemplate::new(429).set_body_json(json!({
                        "ok": false,
                        "error_code": 429,
                        "description": format!("Too Many Requests: retry after {secs}"),
                        "parameters": { "retry_after": secs }
                    }));
                }
                if state.fail_updates.load(Ordering::SeqCst) > 0 {
                    state.fail_updates.fetch_sub(1, Ordering::SeqCst);
                    return tg_fail(502, "Bad Gateway");
                }
                let offset = param(req, "offset").and_then(|o| o.parse::<i64>().ok());
                let list: Vec<Value> = state
                    .updates
                    .lock()
                    .unwrap()
                    .iter()
                    .filter(|u| offset.is_none_or(|o| u["update_id"].as_i64().unwrap() >= o))
                    .cloned()
                    .collect();
                if list.is_empty() {
                    // long polling 흉내: 받을 것이 없으면 잠깐 쥐었다가 빈 목록.
                    tg_ok(json!([])).set_delay(Duration::from_millis(150))
                } else {
                    tg_ok(json!(list))
                }
            }
            "sendMessage" => {
                if state.fail_send.load(Ordering::SeqCst) {
                    return tg_fail(500, "Internal Server Error");
                }
                let id = state.next_message.fetch_add(1, Ordering::SeqCst);
                let chat = param(req, "chat_id").unwrap_or_default();
                tg_ok(json!({
                    "message_id": id,
                    "chat": { "id": chat.parse::<i64>().unwrap_or(0), "type": "private" },
                    "date": 1_790_000_000,
                    "text": param(req, "text").unwrap_or_default()
                }))
            }
            _ => tg_fail(404, "Not Found"),
        }
    }
}

/// 보낸 글 하나.
#[derive(Debug, Clone)]
pub struct Sent {
    pub chat: String,
    pub text: String,
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
            revoked: AtomicBool::new(false),
            rate_limit_next: Mutex::new(None),
            fail_updates: AtomicU32::new(0),
            fail_send: AtomicBool::new(false),
        });
        Mock::given(any())
            .respond_with(TgResponder {
                state: Arc::clone(&state),
            })
            .mount(&server)
            .await;
        FakeTelegram { server, state }
    }

    /// 기준 주소(`http://127.0.0.1:<포트>`).
    pub fn uri(&self) -> String {
        self.server.uri()
    }

    fn push(&self, sender: i64, chat: i64, chat_type: &str, text: &str) -> i64 {
        let id = self.state.next_update.fetch_add(1, Ordering::SeqCst);
        let update = json!({
            "update_id": id,
            "message": {
                "message_id": id + 100,
                "from": { "id": sender, "is_bot": false, "first_name": "시험" },
                "chat": { "id": chat, "type": chat_type },
                "date": 1_790_000_000 + id,
                "text": text
            }
        });
        self.state.updates.lock().unwrap().push(update);
        id
    }

    /// 1:1 대화(대화 번호 = 보낸 사람 번호)에 글 하나를 받게 둔다. 갱신 번호를 돌려준다.
    pub fn push_dm(&self, sender: i64, text: &str) -> i64 {
        self.push(sender, sender, "private", text)
    }

    /// 그룹 대화에 글 하나를 받게 둔다.
    pub fn push_group(&self, sender: i64, chat: i64, text: &str) -> i64 {
        self.push(sender, chat, "group", text)
    }

    /// 받은 요청 모두.
    pub async fn requests(&self) -> Vec<Request> {
        self.server.received_requests().await.unwrap_or_default()
    }

    /// 보낸 글들(성공 여부와 상관없이 보내려 한 차례대로).
    pub async fn sent(&self) -> Vec<Sent> {
        self.requests()
            .await
            .iter()
            .filter(|r| r.url.path().ends_with("/sendMessage"))
            .map(|r| Sent {
                chat: param(r, "chat_id").unwrap_or_default(),
                text: param(r, "text").unwrap_or_default(),
            })
            .collect()
    }

    /// 그 대화로 보낸 글들.
    pub async fn sent_to(&self, chat: i64) -> Vec<String> {
        let chat = chat.to_string();
        self.sent()
            .await
            .into_iter()
            .filter(|s| s.chat == chat)
            .map(|s| s.text)
            .collect()
    }

    /// 그 대화로 보낸 글 가운데 조건에 맞는 것이 올 때까지 기다려 돌려준다.
    pub async fn wait_sent(&self, chat: i64, what: &str, check: impl Fn(&str) -> bool) -> String {
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

/// 페어링 답 글에서 코드를 꺼낸다(답의 앞부분 [`MESSENGER_PAIRING_REPLY`] 뒤의 낱말).
pub fn code_in(text: &str) -> Option<String> {
    let rest = text.split(MESSENGER_PAIRING_REPLY).nth(1)?;
    let code = rest.split_whitespace().next()?;
    let ok = code.chars().count() == CODE_LEN && code.chars().all(|c| CODE_ALPHABET.contains(c));
    ok.then(|| code.to_string())
}

/// 페어링 답 글인가.
pub fn is_pairing_reply(text: &str) -> bool {
    code_in(text).is_some()
}

// ───────────── 가짜 푸시 서비스 ─────────────

/// 브라우저 구독 키 자리: P-256 생성점(65바이트 비압축 점, base64url — round17과 같은 값).
const P256DH: &str =
    "BGsX0fLhLEJH-Lzm5WOkQPJ3A32BLeszoPShOUXYmMKWT-NC4v4af5uO5-tKfA-eFivOM1drMV7Oy7ZAaDe_UfU";
/// 브라우저 구독의 auth(16바이트, base64url).
const AUTH: &str = "AAECAwQFBgcICQoLDA0ODw";

/// 승인 요청 푸시의 `Urgency`(`pi_notify::NoticeKind::ApprovalRequest`).
pub const URGENT: &str = "high";
/// 웹 푸시 서비스 가짜. `/push/…`로 오는 POST에 201을 주고 받은 요청을 남긴다.
pub struct PushServer {
    server: MockServer,
}

impl PushServer {
    async fn start() -> PushServer {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path_regex("^/push/"))
            .respond_with(ResponseTemplate::new(201))
            .mount(&server)
            .await;
        PushServer { server }
    }

    /// 구독 주소(`https://`)의 scheme · host · port를 이 가짜 서버로 바꿔 보내는 길(`test-redirect`).
    fn transport(&self) -> Arc<dyn pi_notify::PushTransport> {
        let base = url::Url::parse(&self.server.uri()).unwrap();
        let transport = pi_notify::HttpTransport::new(Duration::from_secs(5))
            .unwrap()
            .redirect_to(base);
        Arc::new(transport)
    }

    /// 받은 푸시 가운데 `Urgency`가 그것인 수.
    pub async fn count(&self, urgency: &str) -> usize {
        self.server
            .received_requests()
            .await
            .unwrap_or_default()
            .iter()
            .filter(|r| r.url.path().starts_with("/push/"))
            .filter(|r| {
                r.headers
                    .get("urgency")
                    .and_then(|v| v.to_str().ok())
                    .is_some_and(|v| v.eq_ignore_ascii_case(urgency))
            })
            .count()
    }

    /// `Urgency`가 그것인 푸시가 `at_least`개가 될 때까지 기다린다.
    pub async fn wait(&self, urgency: &str, at_least: usize) {
        let deadline = Instant::now() + STEP;
        while self.count(urgency).await < at_least {
            assert!(
                Instant::now() < deadline,
                "시간 안에 {urgency} 푸시가 {at_least}개가 되지 않았다"
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }
}

// ───────────── 가짜 AI · 가짜 자료 ─────────────

/// 사무실 채팅 턴(멘션 답 · 공지 확인 답)의 머리(서버의 `[사무실 채팅]`).
const CHAT_HEAD: &str = "[사무실 채팅]";
/// strategy의 논의 글.
pub const STRATEGY_TALK: &str = "전략상 진입할 만하다.";
/// risk의 논의 글.
pub const RISK_TALK: &str = "손절 기준을 지키면 감당할 만하다.";
/// 팀장 판단(지켜보기)이 사무실 글로 바뀐 문장의 앞부분(`leader_sentence`).
pub const LEADER_WATCH: &str = "지켜보기로 했습니다";
/// 사무실 채팅 턴의 기본 답(역할 id 뒤에 붙는다).
pub const CHAT_DEFAULT: &str = "확인했습니다.";

/// 가짜 AI가 받은 요청 하나: 역할 id(`담당 id: …`)와 마지막 사용자 글.
#[derive(Debug, Clone)]
pub struct Asked {
    pub role: String,
    pub last: String,
}

fn last_user_text(body: &Value) -> String {
    let Some(message) = body["messages"]
        .as_array()
        .and_then(|messages| messages.iter().rev().find(|m| m["role"] == json!("user")))
    else {
        return String::new();
    };
    match &message["content"] {
        Value::String(text) => text.clone(),
        Value::Array(blocks) => blocks
            .iter()
            .filter_map(|b| b["text"].as_str())
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

/// 로컬 가짜 AI 서버(Anthropic 대화 요청). 받은 요청을 남기고 역할 · 턴별 글로 답한다.
pub struct FakeAi {
    server: MockServer,
    chats: Arc<Mutex<HashMap<String, String>>>,
    talks: Arc<Mutex<HashMap<String, String>>>,
    asked: Arc<Mutex<Vec<Asked>>>,
}

/// 정해 둔 답들: 사무실 채팅 답 · 논의 글(역할 id별).
struct Scripts {
    chats: Arc<Mutex<HashMap<String, String>>>,
    talks: Arc<Mutex<HashMap<String, String>>>,
}

fn ai_reply(role: &str, last: &str, scripts: &Scripts) -> String {
    let chats = &scripts.chats;
    if last.contains(CHAT_HEAD) {
        return chats
            .lock()
            .unwrap()
            .get(role)
            .cloned()
            .unwrap_or_else(|| format!("{role} {CHAT_DEFAULT}"));
    }
    if last.contains("[하루 판단]") {
        json!({"plan": "trade"}).to_string()
    } else if last.contains("[복기]") {
        json!({"mistakes": [], "note": "시험 복기"}).to_string()
    } else if last.contains("[점수]") {
        json!({"score": 70, "reason": "시험 점수"}).to_string()
    } else if last.contains("판단을 JSON 객체 하나로") {
        json!({"decision": "watch", "reason": "거래량이 적다", "revisit": "거래량이 늘면"})
            .to_string()
    } else if last.contains("[주문 안]") || last.contains("[팔기 주문 안]") {
        json!({"side": "buy", "note": "시험 주문 안"}).to_string()
    } else if let Some(talk) = scripts.talks.lock().unwrap().get(role) {
        talk.clone()
    } else {
        match role {
            "scout" => "새 자료를 확인했다. @strategy @risk 의견을 부탁한다".to_string(),
            "strategy" => STRATEGY_TALK.to_string(),
            "risk" => RISK_TALK.to_string(),
            _ => "(pass)".to_string(),
        }
    }
}

impl FakeAi {
    async fn start() -> FakeAi {
        let server = MockServer::start().await;
        let chats: Arc<Mutex<HashMap<String, String>>> = Arc::new(Mutex::new(HashMap::new()));
        let talks: Arc<Mutex<HashMap<String, String>>> = Arc::new(Mutex::new(HashMap::new()));
        let asked: Arc<Mutex<Vec<Asked>>> = Arc::new(Mutex::new(Vec::new()));
        let scripts = Scripts {
            chats: Arc::clone(&chats),
            talks: Arc::clone(&talks),
        };
        let log = Arc::clone(&asked);
        Mock::given(method("POST"))
            .respond_with(move |request: &Request| {
                let body: Value = serde_json::from_slice(&request.body).unwrap_or(Value::Null);
                let system = match &body["system"] {
                    Value::String(text) => text.clone(),
                    Value::Array(blocks) => blocks
                        .iter()
                        .filter_map(|b| b["text"].as_str())
                        .collect::<Vec<_>>()
                        .join("\n"),
                    _ => String::new(),
                };
                let role = system
                    .split("담당 id: ")
                    .nth(1)
                    .and_then(|rest| rest.split_whitespace().next())
                    .unwrap_or("leader")
                    .to_string();
                let last = last_user_text(&body);
                let text = ai_reply(&role, &last, &scripts);
                log.lock().unwrap().push(Asked { role, last });
                ResponseTemplate::new(200).set_body_json(json!({
                    "id": "msg_round42", "type": "message", "role": "assistant", "model": "test-model",
                    "content": [{"type": "text", "text": text}],
                    "stop_reason": "end_turn",
                    "usage": {"input_tokens": 1, "output_tokens": 1}
                }))
            })
            .mount(&server)
            .await;
        FakeAi {
            server,
            chats,
            talks,
            asked,
        }
    }

    fn uri(&self) -> String {
        self.server.uri()
    }

    /// 그 역할(id — `strategy` · `risk` · `scout` · `review` · `trader` · `leader`)의 사무실 채팅 답을 정한다.
    pub fn script_chat(&self, role: &str, reply: &str) {
        self.chats
            .lock()
            .unwrap()
            .insert(role.to_string(), reply.to_string());
    }

    /// 그 역할의 논의 글(종목 논의 방의 담당 턴 답)을 정한다.
    pub fn script_talk(&self, role: &str, reply: &str) {
        self.talks
            .lock()
            .unwrap()
            .insert(role.to_string(), reply.to_string());
    }

    /// 받은 요청들(받은 순서).
    pub fn asked(&self) -> Vec<Asked> {
        self.asked.lock().unwrap().clone()
    }

    /// 마지막 사용자 글에 `needle`이 든 요청 수.
    pub fn asked_with(&self, needle: &str) -> usize {
        self.asked()
            .iter()
            .filter(|a| a.last.contains(needle))
            .count()
    }

    /// 마지막 사용자 글에 `needle`이 든 요청의 역할들(받은 순서).
    pub fn roles_with(&self, needle: &str) -> Vec<String> {
        self.asked()
            .into_iter()
            .filter(|a| a.last.contains(needle))
            .map(|a| a.role)
            .collect()
    }

    /// 받은 요청 수가 300ms 동안 그대로일 때까지 기다려(가장 길게 3초) 그 수를 돌려준다. 서버가 켜지며 뒤에서 부르는
    /// 하루 판단이 "글을 보낸 뒤 늘어난 부름"으로 세어지지 않게 쓴다.
    pub async fn settled(&self) -> usize {
        let mut seen = self.asked().len();
        for _ in 0..10 {
            tokio::time::sleep(Duration::from_millis(300)).await;
            let now = self.asked().len();
            if now == seen {
                break;
            }
            seen = now;
        }
        seen
    }
}

/// [`HYNIX`]의 DART 고유번호.
const CORP_HYNIX: &str = "00164779";

fn data_ok(body: Value) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(body)
}

fn ecos_body(stat: &str, item: &str, name: &str, unit: &str, time: &str, value: &str) -> Value {
    json!({
        "StatisticSearch": {
            "list_total_count": 1,
            "row": [{
                "STAT_CODE": stat, "STAT_NAME": name, "ITEM_CODE1": item, "ITEM_NAME1": name,
                "ITEM_CODE2": null, "ITEM_NAME2": null, "ITEM_CODE3": null, "ITEM_NAME3": null,
                "ITEM_CODE4": null, "ITEM_NAME4": null, "UNIT_NAME": unit, "WGT": null,
                "TIME": time, "DATA_VALUE": value
            }]
        }
    })
}

/// 경로로 나눠 답한다(round22의 가짜 자료 서버와 같은 모양): DART 공시(월요일 공시 하나) · 재무, ECOS 금리 · 환율,
/// FRED 관측값, 네이버 뉴스(없음). 논의에 필요한 자료가 모두 있어야 담당들이 AI 턴을 돈다.
fn data_reply(request: &Request) -> ResponseTemplate {
    let path = request.url.path();
    match path {
        "/api/list.json" => data_ok(json!({
            "status": "000", "message": "정상", "page_no": 1, "page_count": 100,
            "total_count": 1, "total_page": 1, "list": [{
                "corp_cls": "Y", "corp_name": "시험회사", "corp_code": CORP_HYNIX, "stock_code": HYNIX,
                "report_nm": "주요사항보고서(자기주식취득결정)", "rcept_no": "20261116000042",
                "flr_nm": "시험회사", "rcept_dt": "20261116", "rm": "유"
            }]
        })),
        "/api/fnlttSinglAcnt.json" => data_ok(json!({
            "status": "000", "message": "정상",
            "list": [{
                "rcept_no": "20260310000789", "reprt_code": "11011", "bsns_year": "2025",
                "corp_code": CORP_HYNIX, "stock_code": "000000", "fs_div": "CFS", "fs_nm": "연결재무제표",
                "sj_div": "IS", "sj_nm": "손익계산서", "account_nm": "매출액", "thstrm_nm": "제 57 기",
                "thstrm_dt": "2025.01.01 ~ 2025.12.31", "thstrm_amount": "300,870,903,000,000",
                "frmtrm_nm": "제 56 기", "frmtrm_dt": "2024.01.01 ~ 2024.12.31",
                "frmtrm_amount": "1,000", "ord": "21", "currency": "KRW"
            }]
        })),
        "/fred/series/observations" => data_ok(json!({
            "observations": [{"date": "2026-11-12", "value": "4.10"}]
        })),
        "/v1/search/news.json" => data_ok(json!({
            "lastBuildDate": "Mon, 16 Nov 2026 10:00:00 +0900",
            "total": 0, "start": 1, "display": 0, "items": []
        })),
        _ if path.contains("/722Y001/") => data_ok(ecos_body(
            "722Y001",
            "0101000",
            "한국은행 기준금리",
            "연%",
            "202610",
            "2.25",
        )),
        _ if path.contains("/731Y001/") => data_ok(ecos_body(
            "731Y001",
            "0000001",
            "원/미국달러",
            "원",
            "20261113",
            "1391.5",
        )),
        _ => ResponseTemplate::new(404),
    }
}

/// 로컬 가짜 자료 서버(DART · ECOS · FRED · 네이버 뉴스 한 곳).
async fn fake_data() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(data_reply)
        .mount(&server)
        .await;
    server
}

/// 가짜 자료 서버를 쓰는 팀 조립 값(공식 팀 · 자료 주기 1시간 · [`HYNIX`]의 고유번호).
fn team_wiring(data: &MockServer) -> TeamWiring {
    let mut team = TeamWiring::official();
    let uri = data.uri();
    team.data = DataConfig::new(uri.clone(), uri.clone(), uri.clone());
    team.data.timeout = Duration::from_secs(3);
    team.corp_codes = HashMap::from([(Symbol::new(HYNIX), CorpCode::parse(CORP_HYNIX).unwrap())]);
    team.news = NaverNewsConfig::new(uri).with_timeout(Duration::from_secs(3));
    team.data_poll = Duration::from_secs(3600);
    team
}

// ───────────── 조립 ─────────────

fn market_rules(currency: Currency, scale: u32, ticks: Vec<TickBand>) -> MarketRules {
    MarketRules {
        currency,
        commission_rate: d("0.00015"),
        sell_tax_rate: d("0.0020"),
        rounding: MoneyRounding {
            scale,
            mode: RoundingMode::Down,
        },
        tick_table: ticks,
        price_limit_rate: Some(d("0.30")),
        settlement: Settlement {
            business_days: 2,
            proceeds_usable: ProceedsUsable::AfterSettlement,
        },
        market_slippage_rate: d("0.001"),
        limit_fill_price: LimitFillPrice::Limit,
        max_fill: MaxFill::Unlimited,
        sessions: SessionRules {
            limit: vec![SessionKind::Regular],
            market: vec![SessionKind::Regular],
        },
        high_value_confirm_from: None,
        max_order_amount: None,
    }
}

/// 주문 길 조립: 모의계좌 · 국내 달력, 업종표에 [`HYNIX`]만("반도체").
fn trading_wiring() -> TradingWiring {
    let tick = |below: Option<&str>, tick: &str| TickBand {
        below: below.map(d),
        tick: d(tick),
    };
    let paper = PaperConfig {
        initial_cash: InitialCash {
            krw: d("10000000"),
            usd: d("10000"),
        },
        kr: market_rules(
            Currency::Krw,
            0,
            vec![
                tick(Some("50000"), "50"),
                tick(Some("200000"), "100"),
                tick(None, "1000"),
            ],
        ),
        us: market_rules(Currency::Usd, 2, vec![tick(None, "0.01")]),
        stale: StalePolicy {
            max_age: Duration::from_secs(30),
        },
        event_buffer: 64,
    };
    let calendar: CalendarFn = Arc::new(kr_calendar);
    let mut rules = RulesConfig::default();
    let mut sectors = HashMap::new();
    sectors.insert(Symbol::new(HYNIX), "반도체".to_string());
    rules.sectors = sectors;
    TradingWiring {
        paper,
        rules,
        calendar: Some(calendar),
    }
}

/// 아무도 듣지 않는 로컬 주소(실제 바깥 주소로 나가지 않게).
fn dead_base_url() -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    format!("http://127.0.0.1:{port}")
}

/// 설정: 그 보관 폴더 · 빈 포트 · 짧은 시세 간격 · 매매 종목(시연 아님).
fn settings(home: &Path) -> Settings {
    let args: Vec<String> = [
        "--home",
        home.to_str().unwrap(),
        "--port",
        "0",
        "--interval-ms",
        "20",
        "--retry-initial-ms",
        "20",
        "--retry-max-ms",
        "40",
        "--symbols",
        HYNIX,
    ]
    .iter()
    .map(|s| (*s).to_string())
    .collect();
    Settings::parse(&args, &|_| None, None).expect("설정을 읽어야 한다")
}

/// 다른 기기로 볼 상대 주소(사설 대역).
const REMOTE_PEER: [u8; 4] = [100, 64, 0, 7];

fn peer_override(remote: Arc<AtomicBool>) -> pi_gate::PeerOverride {
    Arc::new(move |addr: SocketAddr| {
        if remote.load(Ordering::SeqCst) {
            SocketAddr::from((REMOTE_PEER, addr.port()))
        } else {
            addr
        }
    })
}

/// 가짜 증권사(토스): 토큰 · 현재가(시계의 지금 시각으로 [`HYNIX`] 한 종목, 값은 `price` — 처음은 진입가) · 국내 달력.
/// 시세 흐름이 바퀴마다 시세를 받게 한다(round17의 가짜 시세 서버와 같은 모양).
async fn fake_broker(clock: Arc<TestClock>, price: Arc<AtomicI64>) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/oauth2/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "access_token": "zz-round42-quote-token", "token_type": "Bearer", "expires_in": 86_400
        })))
        .mount(&server)
        .await;
    let price_clock = Arc::clone(&clock);
    Mock::given(method("GET"))
        .and(path("/api/v1/prices"))
        .respond_with(move |_: &Request| {
            let now = price_clock.now().with_timezone(&seoul());
            ResponseTemplate::new(200).set_body_json(json!({"result": [{
                "symbol": HYNIX, "timestamp": now.format("%Y-%m-%dT%H:%M:%S%.3f%:z").to_string(),
                "lastPrice": price.load(Ordering::SeqCst).to_string(), "currency": "KRW"
            }]}))
        })
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v1/market-calendar/KR"))
        .respond_with(move |_: &Request| {
            let today = clock.now().with_timezone(&seoul()).date_naive();
            ResponseTemplate::new(200).set_body_json(json!({"result": {
                "today": toss_day(today),
                "previousBusinessDay": toss_day(business_day(today, false)),
                "nextBusinessDay": toss_day(business_day(today, true)),
            }}))
        })
        .mount(&server)
        .await;
    server
}

fn seoul() -> chrono::FixedOffset {
    chrono::FixedOffset::east_opt(9 * 3600).unwrap()
}

/// 토스 달력 응답의 하루(round17과 같은 모양).
fn toss_day(date: NaiveDate) -> Value {
    let date = date.format("%Y-%m-%d").to_string();
    json!({
        "date": date,
        "integrated": {
            "preMarket": {
                "startTime": format!("{date}T08:00:00+09:00"),
                "singlePriceAuctionStartTime": format!("{date}T08:50:00+09:00"),
                "endTime": format!("{date}T09:00:00+09:00"),
            },
            "regularMarket": {
                "startTime": format!("{date}T09:00:00+09:00"),
                "singlePriceAuctionStartTime": format!("{date}T15:20:00+09:00"),
                "endTime": format!("{date}T15:30:00+09:00"),
            },
            "afterMarket": {
                "startTime": format!("{date}T15:30:00+09:00"),
                "singlePriceAuctionEndTime": format!("{date}T15:40:00+09:00"),
                "endTime": format!("{date}T20:00:00+09:00"),
            },
        }
    })
}

fn wiring(parts: &Parts) -> Wiring {
    let (clock, remote, broker) = (&parts.clock, &parts.remote, &parts.broker);
    Wiring {
        upstreams: Upstreams {
            toss: TossConfig {
                base_url: broker.uri(),
                timeout: Duration::from_secs(3),
                token_failure_cooldown: Duration::ZERO,
                ..TossConfig::default()
            },
            anthropic: AnthropicConfig {
                base_url: parts.ai.uri(),
                timeout: Duration::from_secs(3),
                max_retries: 0,
                retry_base_delay: Duration::from_millis(1),
            },
        },
        clock: Arc::clone(clock) as Arc<dyn Clock>,
        peer_override: Some(peer_override(Arc::clone(remote))),
        cli_locations: Vec::new(),
    }
}

/// 논의 글 묶음 창(시험 조립 값).
pub const TALK_BATCH: Duration = Duration::from_secs(2);

/// 시험 메신저 조립: 가짜 텔레그램 주소 · long polling 1초 · 요청 3초 · 물러서기 3→7초 · 보내기 3번 · 논의 글 묶음
/// [`TALK_BATCH`].
pub fn messenger_wiring(tg: &FakeTelegram) -> MessengerWiring {
    MessengerWiring {
        telegram_api_base: tg.uri(),
        poll_timeout_secs: 1,
        request_timeout: Duration::from_secs(3),
        backoff_initial: BACKOFF_INITIAL,
        backoff_max: BACKOFF_MAX,
        send_attempts: 3,
        talk_batch: TALK_BATCH,
    }
}

/// 무엇이든 받는 시험용 확인자(바깥으로 나가지 않는다).
pub struct AcceptAll;

impl Verifier for AcceptAll {
    async fn verify(&self, _kind: KeyKind, _values: &Credentials) -> Result<(), VerifyFailure> {
        Ok(())
    }

    async fn verify_slot(
        &self,
        _slot: KeySlot,
        _values: &Credentials,
    ) -> Result<(), VerifyFailure> {
        Ok(())
    }

    async fn verify_provider(
        &self,
        _keys: ProviderKeys,
        _values: &Credentials,
    ) -> Result<(), VerifyFailure> {
        Ok(())
    }
}

/// 필수 키(증권사 · AI)를 시험용 값으로 등록한 새 보관 폴더.
pub async fn fresh_home() -> TempDir {
    let dir = tempfile::tempdir().unwrap();
    let secrets = SecretStore::open(dir.path()).unwrap();
    let broker = Credentials::new()
        .with("client_id", CLIENT_ID)
        .with("client_secret", CLIENT_SECRET);
    secrets
        .register(KeyKind::Broker, broker, &AcceptAll)
        .await
        .unwrap();
    let ai = Credentials::new().with("api_key", AI_KEY);
    secrets
        .register(KeyKind::AiApiKey, ai, &AcceptAll)
        .await
        .unwrap();
    for (slot, value) in [
        (KeySlot::Dart, "zz-round42-dart-key-0004"),
        (KeySlot::Ecos, "zz-round42-ecos-key-0005"),
    ] {
        let values = Credentials::new().with(slot.fields()[0], value);
        secrets
            .register_slot(slot, values, &AcceptAll)
            .await
            .unwrap();
    }
    dir
}

/// 띄우기: 그 가짜 텔레그램으로 메신저를 잇고 폰 푸시는 가짜 푸시 서비스로, 백업은 닿지 않는 로컬 주소로 둔다
/// ([`App::launch_with_messenger`]).
async fn launch(home: &Path, parts: &Parts, tg: &FakeTelegram) -> App {
    let launch = Launch {
        trading: trading_wiring(),
        team: Some(team_wiring(&parts.data)),
        secrets: None,
    };
    let reach = Reach {
        push: Some(parts.push.transport()),
        backup: Some(BackupWiring {
            api_base: parts.dead_github.clone(),
            repo: "siheom/round42-backup".to_string(),
            recipient: parts.recipient.clone(),
        }),
    };
    let settings = settings(home);
    let wiring = wiring(parts);
    let messenger = messenger_wiring(tg);
    tokio::time::timeout(
        STEP,
        App::launch_with_messenger(settings, wiring, launch, reach, messenger),
    )
    .await
    .expect("서버가 시간 안에 떠야 한다")
    .expect("서버가 떠야 한다")
}

// ───────────── 띄운 서버 ─────────────

/// 다시 띄워도 그대로 쓰는 바깥 자리: 시계 · 상대 주소 · 가짜 증권사 · 가짜 푸시 서비스 · 닿지 않는 GitHub 자리 ·
/// 백업 수신자.
struct Parts {
    clock: Arc<TestClock>,
    remote: Arc<AtomicBool>,
    broker: MockServer,
    /// 가짜 증권사가 주는 [`HYNIX`]의 값(원).
    price: Arc<AtomicI64>,
    ai: FakeAi,
    data: MockServer,
    push: PushServer,
    dead_github: String,
    recipient: String,
}

/// 떠 있는 서버 하나와 그 보관 폴더 · 바깥 자리.
pub struct Bench {
    dir: TempDir,
    parts: Parts,
    app: App,
}

impl Bench {
    /// 새 보관 폴더(필수 키 등록)로 월요일 10시에 띄운다.
    pub async fn start(tg: &FakeTelegram) -> Bench {
        Bench::start_in(fresh_home().await, tg).await
    }

    /// 그 보관 폴더로 월요일 10시에 띄운다(띄우기 전에 보관 폴더에 파일을 둘 때).
    pub async fn start_in(dir: TempDir, tg: &FakeTelegram) -> Bench {
        let clock = TestClock::at(kst(monday(), 10, 0));
        let price = Arc::new(AtomicI64::new(ENTRY));
        let parts = Parts {
            remote: Arc::new(AtomicBool::new(false)),
            broker: fake_broker(Arc::clone(&clock), Arc::clone(&price)).await,
            price,
            ai: FakeAi::start().await,
            data: fake_data().await,
            push: PushServer::start().await,
            dead_github: dead_base_url(),
            recipient: age::x25519::Identity::generate().to_public().to_string(),
            clock,
        };
        let app = launch(dir.path(), &parts, tg).await;
        Bench { dir, parts, app }
    }

    /// 토큰을 등록하고 받는 중이 될 때까지 기다린 서버.
    pub async fn with_token(tg: &FakeTelegram) -> Bench {
        let bench = Bench::start(tg).await;
        let reply = bench.register(TOKEN).await;
        assert_eq!(reply.status, 200, "토큰 등록: {}", reply.text);
        bench.wait_state("polling").await;
        bench
    }

    /// 토큰 등록 · 그 사람 페어링 · 폰 구독까지 맞춘 서버(알림이 두 전달처로 갈 수 있다).
    pub async fn linked(tg: &FakeTelegram, owner: i64) -> Bench {
        let bench = Bench::with_token(tg).await;
        bench.pair(tg, owner).await;
        bench.subscribe().await;
        bench
    }

    /// 끄고, 같은 보관 폴더 · 같은 바깥 자리로 다시 띄운다(상대 주소는 그 컴퓨터로 돌린다).
    pub async fn restart(self, tg: &FakeTelegram) -> Bench {
        let Bench { dir, parts, app } = self;
        tokio::time::timeout(STEP, app.shutdown())
            .await
            .expect("시간 안에 꺼져야 한다")
            .expect("끄기가 성공해야 한다");
        parts.remote.store(false, Ordering::SeqCst);
        let app = launch(dir.path(), &parts, tg).await;
        Bench { dir, parts, app }
    }

    pub async fn close(self) {
        tokio::time::timeout(STEP, self.app.shutdown())
            .await
            .expect("시간 안에 꺼져야 한다")
            .expect("끄기가 성공해야 한다");
    }

    pub fn store(&self) -> Store {
        self.app.store()
    }

    pub fn addr(&self) -> SocketAddr {
        self.app.local_addr()
    }

    /// 가짜 푸시 서비스.
    pub fn push(&self) -> &PushServer {
        &self.parts.push
    }

    /// 가짜 AI 서버.
    pub fn ai(&self) -> &FakeAi {
        &self.parts.ai
    }

    /// 이어 띄운 투자 팀.
    pub fn crew(&self) -> Arc<Crew> {
        self.app.crew().expect("발판은 팀을 이어 띄운다")
    }

    /// 시계를 `seconds`초 옮긴다(실제로 자지 않는다).
    pub fn pass(&self, seconds: i64) {
        self.parts.clock.pass(seconds);
    }

    /// [`HYNIX`] 종목 논의를 한 번 돌린다(끝날 때까지 기다린다). 결과는 보지 않는다(팀장은 지켜보기로 판단한다).
    pub async fn discuss(&self, topic: &str) {
        let crew = self.crew();
        let done = tokio::time::timeout(STEP, crew.discuss(HYNIX, topic)).await;
        assert!(done.is_ok(), "논의가 시간 안에 끝나야 한다");
    }

    /// 저장된 이벤트 가운데 그 종류인 것의 내용(번호 순서).
    pub fn events(&self, kind: &str) -> Vec<Value> {
        self.store()
            .read_after(0, 100_000)
            .unwrap()
            .into_iter()
            .filter(|event| event.kind == kind)
            .map(|event| event.payload)
            .collect()
    }

    /// 하루 AI 호출 상한을 설정 저장 길로 정하고 같은 보관 폴더로 다시 켠다(상한은 켤 때 팀에 닿는다).
    pub async fn with_ai_limit(self, tg: &FakeTelegram, limit: u32) -> Bench {
        let reply = post(
            self.addr(),
            SETUP_AI_PATH,
            &json!({ AI_DAILY_LIMIT_KEY: limit }),
        )
        .await;
        assert_eq!(reply.status, 200, "상한을 정해야 한다: {}", reply.text);
        let bench = self.restart(tg).await;
        bench.wait_state("polling").await;
        bench
    }

    /// 손실 한도를 낮춘 스윙 템플릿(하루 손실 0.5% · 최대 낙폭 1%, 자동 주문 한도는 기본)으로 오늘 매매하기로 한다
    /// (round9의 최대 손실 시험과 같은 값).
    pub async fn ready_for_loss(&self) {
        let reply = post(self.addr(), TEMPLATE_PATH, &json!({"style": "swing"})).await;
        assert_eq!(reply.status, 200, "템플릿을 골라야 한다: {}", reply.body);
        let id = reply.body["id"].as_i64().expect("템플릿 번호");
        let changes = json!({"daily_loss_limit_pct": "0.5", "max_drawdown_pct": "1"});
        let body = json!({"id": id, "changes": changes});
        let reply = post(self.addr(), TEMPLATE_ADJUST_PATH, &body).await;
        assert_eq!(reply.status, 200, "상한 안의 값은 저장된다: {}", reply.body);
        let store = self.store();
        if store.team_state().unwrap() != TeamState::Operating {
            let _ = self.app.trading().decide_day(DayPlan::Trade);
        }
        wait_for("운용 중이 된다", || {
            store.team_state().unwrap() == TeamState::Operating
        })
        .await;
    }

    /// [`Bench::ready_for_loss`] 뒤에 [`HYNIX`] 5주를 사서 보유한다(자동 주문 한도 안 — 승인 없이 나가 체결된다).
    pub async fn hold(&self) {
        self.quote_at(ENTRY, 0).await;
        let at = self.parts.clock.now();
        let trade = self.judged();
        let placed = self
            .app
            .trading()
            .place(order(trade, at))
            .await
            .expect("주문 안을 처리해야 한다");
        assert!(
            matches!(placed, Placed::Ordered(_)),
            "한도 안의 사는 주문은 바로 나간다: {placed:?}"
        );
        self.quote_at(ENTRY, 1).await;
        let store = self.store();
        wait_for(
            "지정가 이하의 시세로 체결돼 보유중이 된다",
            || store.get_trade(trade).unwrap().state == TradeState::Holding,
        )
        .await;
    }

    /// 시계를 `seconds`초 옮기고 그 시각의 [`HYNIX`] 시세 하나를 넣는다.
    pub async fn quote_at(&self, price: i64, seconds: i64) {
        self.parts.clock.pass(seconds);
        let at = self.parts.clock.now();
        let quote = Quote {
            symbol: Symbol::new(HYNIX),
            price: Decimal::from(price),
            currency: Currency::Krw,
            exchange_time: Some(at),
            received_at: at,
        };
        self.app
            .trading()
            .on_quote(quote)
            .await
            .expect("시세를 받아야 한다");
    }

    /// 보유 중에 값이 22% 떨어진 시세를 넣어 최대 손실(1%)에 닿게 한다(5주 × 22,000원 = 평가액의 1.1%).
    pub async fn crash(&self) {
        self.parts.price.store(78_000, Ordering::SeqCst);
        self.quote_at(78_000, 5).await;
    }

    /// 이 컴퓨터에서 폰 하나를 구독한다(푸시 주소는 가짜 푸시 서비스로 돌아간다).
    pub async fn subscribe(&self) {
        let body = json!({
            "endpoint": "https://push.invalid/push/41",
            "p256dh": P256DH, "auth": AUTH, "device": "시험 폰",
        });
        let reply = post(self.addr(), PUSH_SUBSCRIBE_PATH, &body).await;
        assert_eq!(reply.status, 204, "구독을 받아야 한다: {}", reply.text);
    }

    /// 이 컴퓨터에서 토큰을 등록한다.
    pub async fn register(&self, token: &str) -> Reply {
        post(
            self.addr(),
            MESSENGER_TELEGRAM_PATH,
            &json!({ "token": token }),
        )
        .await
    }

    /// 이 컴퓨터에서 페어링 코드로 승인한다.
    pub async fn approve(&self, code: &str) -> Reply {
        post(
            self.addr(),
            MESSENGER_PAIRING_APPROVE_PATH,
            &json!({ "code": code }),
        )
        .await
    }

    /// 메신저 상태(이 컴퓨터의 세션).
    pub async fn status(&self) -> Value {
        let reply = get(self.addr(), MESSENGER_PATH).await;
        assert_eq!(reply.status, 200, "상태를 읽는다: {}", reply.text);
        reply.body
    }

    /// 텔레그램 상태가 그것이 될 때까지 기다린다.
    pub async fn wait_state(&self, state: &str) {
        let deadline = Instant::now() + STEP;
        loop {
            let status = self.status().await;
            if status["telegram"]["state"] == json!(state) {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "시간 안에 상태가 {state}가 되지 않았다: {status}"
            );
            tokio::time::sleep(Duration::from_millis(30)).await;
        }
    }

    /// 그 사람이 봇에게 처음 말을 걸어 받은 코드로 이 컴퓨터에서 승인한다. 받은 코드를 돌려준다.
    pub async fn pair(&self, tg: &FakeTelegram, sender: i64) -> String {
        tg.push_dm(sender, "안녕하세요");
        let reply = tg
            .wait_sent(sender, "페어링 코드 답", is_pairing_reply)
            .await;
        let code = code_in(&reply).unwrap();
        let approved = self.approve(&code).await;
        assert_eq!(approved.status, 200, "맞는 코드로 승인: {}", approved.text);
        code
    }

    /// 금액 한도를 낮춘 스윙 템플릿으로 오늘 매매하기로 한다(점검을 지난 사는 주문이 승인을 기다리게).
    pub async fn ready(&self) {
        let reply = post(self.addr(), TEMPLATE_PATH, &json!({"style": "swing"})).await;
        assert_eq!(reply.status, 200, "템플릿을 골라야 한다: {}", reply.body);
        let id = reply.body["id"].as_i64().expect("템플릿 번호");
        let changes = json!({"symbol_weight_pct": "10", "auto_order_limit_pct": "2"});
        let body = json!({"id": id, "changes": changes});
        let reply = post(self.addr(), TEMPLATE_ADJUST_PATH, &body).await;
        assert_eq!(reply.status, 200, "상한 안의 값은 저장된다: {}", reply.body);
        let store = self.store();
        if store.team_state().unwrap() != TeamState::Operating {
            let _ = self.app.trading().decide_day(DayPlan::Trade);
        }
        wait_for("운용 중이 된다", || {
            store.team_state().unwrap() == TeamState::Operating
        })
        .await;
    }

    /// 지금 시각의 시세를 넣고 판단됨인 매매를 만들어 [`HYNIX`] 5주 사는 주문 안을 낸다([`Bench::ready`] 뒤에).
    /// 금액 한도를 넘어 승인을 기다린다 — 그 승인 번호와 지문.
    pub async fn awaiting(&self) -> (ApprovalNumber, Fingerprint) {
        let at = self.parts.clock.now();
        let quote = Quote {
            symbol: Symbol::new(HYNIX),
            price: Decimal::from(ENTRY),
            currency: Currency::Krw,
            exchange_time: Some(at),
            received_at: at,
        };
        self.app
            .trading()
            .on_quote(quote)
            .await
            .expect("시세를 받아야 한다");
        let trade = self.judged();
        let placed = self
            .app
            .trading()
            .place(order(trade, at))
            .await
            .expect("주문 안을 처리해야 한다");
        match placed {
            Placed::AwaitingApproval {
                number,
                fingerprint,
            } => (number, fingerprint),
            other => panic!("승인을 기다려야 한다: {other:?}"),
        }
    }

    fn judged(&self) -> TradeId {
        let store = self.store();
        let at = ms(self.parts.clock.now());
        let trade = store
            .create_trade(NewTrade {
                symbol: HYNIX.to_string(),
                account: AccountKind::Paper,
                at,
            })
            .unwrap();
        let judgment = Judgment {
            decided_at: at,
            rationale: pi_store::Rationale {
                hypothesis: Some("실적 개선".to_string()),
                trigger: Some("거래량 증가".to_string()),
                entry: Some(Amount(ENTRY)),
                stop: Some(Amount(STOP)),
                target: Some(Amount(TARGET)),
                expected_value: Some(Amount(EDGE)),
            },
            sources: vec![SourceRef {
                name: "시세".to_string(),
                known_at: at,
            }],
            scores: vec![RoleScore {
                role: "risk".to_string(),
                score: 70,
                reason: "근거 충분".to_string(),
            }],
            consensus_score: 70,
        };
        store.judge_trade(trade.id, judgment).unwrap();
        trade.id
    }
}

/// 지정가 · 수량 5주의 사는 주문 안(VI · 거래정지 아님).
fn order(trade: TradeId, at: DateTime<Utc>) -> OrderRequest {
    let draft = OrderDraft {
        market: Market::Kr,
        symbol: Symbol::new(HYNIX),
        purpose: Purpose::NewEntry,
        order_type: OrderType::Limit,
        limit_price: Some(Decimal::from(ENTRY)),
        ai_quantity: Some(d("5")),
        funding: Funding::Cash,
        product: ProductKind::Stock,
        stage: TradeStage::Judged,
        decided_at: at,
        rationale: Rationale {
            hypothesis: Some("실적 개선".to_string()),
            trigger: Some("거래량 증가".to_string()),
            entry: Some(Decimal::from(ENTRY)),
            stop: Some(Decimal::from(STOP)),
            target: Some(Decimal::from(TARGET)),
            expected_value: Some(Decimal::from(EDGE)),
        },
        sources: vec![SourceStamp {
            name: "시세".to_string(),
            known_at: at,
        }],
    };
    OrderRequest {
        trade_id: trade,
        draft,
        reference_price: Decimal::from(ENTRY),
        status: SymbolStatus {
            vi: Some(false),
            halted: Some(false),
        },
    }
}

// ───────────── 기다리기 · 이벤트 · 파일 ─────────────

/// 조건이 될 때까지 기다린다(가장 길게 [`STEP`]).
pub async fn wait_for(what: &str, mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + STEP;
    while !done() {
        assert!(Instant::now() < deadline, "시간 안에 되지 않았다: {what}");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

// ───────────── HTTP ─────────────

fn origin_of(addr: SocketAddr) -> String {
    format!("http://127.0.0.1:{}", addr.port())
}

/// HTTP 응답 하나: 상태 · JSON 본문(JSON이 아니면 `Null`) · 본문 글.
pub struct Reply {
    pub status: u16,
    pub body: Value,
    pub text: String,
}

async fn send(request: reqwest::RequestBuilder) -> Reply {
    let reply = tokio::time::timeout(STEP, request.send())
        .await
        .expect("응답이 시간 안에 와야 한다")
        .expect("요청이 닿아야 한다");
    let status = reply.status().as_u16();
    let text = reply.text().await.expect("본문을 읽어야 한다");
    let body = serde_json::from_str(&text).unwrap_or(Value::Null);
    Reply { status, body, text }
}

fn cookie_of(reply: &reqwest::Response) -> String {
    let raw = reply
        .headers()
        .get("set-cookie")
        .expect("세션 쿠키가 있어야 한다")
        .to_str()
        .unwrap();
    raw.split(';').next().unwrap().trim().to_string()
}

/// 같은 출처에서 (그 컴퓨터의) 세션을 받아 `Cookie` 헤더 글자를 돌려준다.
pub async fn login(addr: SocketAddr) -> String {
    let origin = origin_of(addr);
    let reply = reqwest::Client::new()
        .post(format!("{origin}{}", pi_gate::SESSION_PATH))
        .header("Origin", &origin)
        .send()
        .await
        .expect("세션 요청이 닿아야 한다");
    assert!(reply.status().is_success(), "세션을 받아야 한다");
    cookie_of(&reply)
}

/// 그 세션으로 읽는다.
pub async fn get_with(addr: SocketAddr, path: &str, cookie: &str) -> Reply {
    let request = reqwest::Client::new()
        .get(format!("{}{path}", origin_of(addr)))
        .header("Cookie", cookie);
    send(request).await
}

/// 같은 출처(`Origin`)에서 그 세션으로 `method`를 보낸다(`body`는 JSON 글자, 없으면 몸통 없음).
pub async fn send_with(
    addr: SocketAddr,
    method: reqwest::Method,
    path: &str,
    cookie: &str,
    body: Option<&str>,
) -> Reply {
    let origin = origin_of(addr);
    let mut request = reqwest::Client::new()
        .request(method, format!("{origin}{path}"))
        .header("Origin", &origin)
        .header("Cookie", cookie);
    if let Some(body) = body {
        request = request
            .header("Content-Type", "application/json")
            .body(body.to_string());
    }
    send(request).await
}

/// 세션을 받아 읽는다.
pub async fn get(addr: SocketAddr, path: &str) -> Reply {
    let cookie = login(addr).await;
    get_with(addr, path, &cookie).await
}

/// 세션을 받아 같은 출처에서 JSON을 `POST`한다.
pub async fn post(addr: SocketAddr, path: &str, body: &Value) -> Reply {
    let cookie = login(addr).await;
    let body = body.to_string();
    send_with(addr, reqwest::Method::POST, path, &cookie, Some(&body)).await
}

/// 세션을 받아 같은 출처에서 몸통 없이 `POST`한다.
pub async fn post_empty(addr: SocketAddr, path: &str) -> Reply {
    let cookie = login(addr).await;
    send_with(addr, reqwest::Method::POST, path, &cookie, None).await
}

/// 승인 요청 알림의 메신저 글인가(거절 안내가 붙는다).
pub fn is_approval_alert(text: &str) -> bool {
    text.contains(price_inha::MESSENGER_REJECT_HINT)
}

/// 메신저 글의 한 줄 모양 `"<표시 이름>: <글>"`(41차 1 · 3번 — 표시 이름은 `OFFICE_ROSTER`).
pub fn said(name: &str, text: &str) -> String {
    format!("{name}: {text}")
}

/// 그 대화로 보낸 글 가운데 `needle`이 든 것의 수.
pub async fn sent_with(tg: &FakeTelegram, chat: i64, needle: &str) -> usize {
    tg.sent_to(chat)
        .await
        .iter()
        .filter(|t| t.contains(needle))
        .count()
}
