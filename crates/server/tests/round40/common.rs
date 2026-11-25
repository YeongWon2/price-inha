//! round40 시험(server 39차)이 함께 쓰는 도우미: 시계(잔 시간을 적는다) · 달력 · 모의계좌 조립 · 가짜 텔레그램 서버 ·
//! 서버 띄우기 · 같은 보관 폴더로 다시 띄우기 · 다른 기기 세션 · HTTP · 사는 주문 안(승인 기다림) · 페어링.
//! round37(같은 홈으로 다시 띄우기 · 원격 세션 · 사는 주문 안)의 장치를 본떠 두고(그 폴더는 고치지 않는다) 39차에 맞게
//! 줄였다. 가짜 텔레그램은 `crates/channel/tests/suite/common.rs`의 모양(경로 `/bot<토큰>/<메서드>`, JSON 몸통)을 따른다.
//!
//! - [`Bench`]: 임시 보관 폴더 · `127.0.0.1:0` · **시연 아님**(필수 키는 시험용 값으로 미리 등록 — 토스 자리는
//!   [`HYNIX`] 시세를 시계의 지금 시각으로 주는 로컬 가짜 증권사, Anthropic 자리는 아무도 듣지 않는 로컬 주소)으로
//!   [`price_inha::App::launch_with_messenger`]를 띄운다. 시세가 흐르므로 이상 감시가 팀을 멈추지 않는다. 팀은 잇지 않는다
//!   (뒤에서 AI를 부르지 않는다). 메신저 조립 값은 가짜 텔레그램 주소 · 짧은 long polling · 물러서기 3초→7초다
//!   (시험 시계는 10ms만 자고 잔 시간을 적는다).
//! - [`FakeTelegram`]: 받을 갱신을 차례로 내주고(`offset` 이상만), 보낸 글(`sendMessage`)은 받은 요청으로 남는다.
//!   토큰이 다르면 401. 시험이 401 · 429 · 502 · 보내기 실패를 켤 수 있다.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use chrono::{DateTime, Datelike, NaiveDate, TimeZone, Utc, Weekday};
use pi_ai::AnthropicConfig;
use pi_approval::{ApprovalNumber, Fingerprint};
use pi_channel::{CODE_ALPHABET, CODE_LEN};
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
use pi_secrets::{Credentials, KeyKind, ProviderKeys, SecretStore, Verifier, VerifyFailure};
use pi_store::{
    AccessKeyHash, AccountKind, Amount, Judgment, NewTrade, RoleScore, SourceRef, Store, TeamState,
    TradeId, UnixMillis,
};
use price_inha::{
    App, CalendarFn, DayPlan, Launch, MESSENGER_PAIRING_APPROVE_PATH, MESSENGER_PAIRING_REPLY,
    MESSENGER_PATH, MESSENGER_TELEGRAM_PATH, MessengerWiring, OrderRequest, Placed, Reach,
    Settings, TEMPLATE_ADJUST_PATH, TEMPLATE_PATH, TradingWiring, Upstreams, Wiring,
};
use rust_decimal::Decimal;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tempfile::TempDir;
use wiremock::matchers::{any, method, path};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

/// 한 걸음을 기다리는 가장 긴 시간.
pub const STEP: Duration = Duration::from_secs(15);
/// "일어나지 않는다"를 지켜보는 짧은 시간.
pub const QUIET: Duration = Duration::from_millis(700);
/// 매매하는 종목(국내). 업종표에 "반도체"로 들어 있다.
pub const HYNIX: &str = "000660";
/// 시험용 봇 토큰(진짜 토큰처럼 보이지 않는다 — 텔레그램 토큰은 `숫자:글자` 모양이다).
pub const TOKEN: &str = "SIHEOM-TG-ROUND40-0001";
/// 가짜 텔레그램이 받지 않는 토큰.
pub const WRONG_TOKEN: &str = "SIHEOM-TG-ROUND40-WRONG";
/// 가짜 봇 이름(getMe의 `username`).
pub const BOT: &str = "siheom_round40_bot";
/// 시험용 증권사 · AI 키(진짜 키처럼 보이지 않는다).
const CLIENT_ID: &str = "zz-round40-client-id-0001";
const CLIENT_SECRET: &str = "zz-round40-client-secret-0002";
const AI_KEY: &str = "zz-round40-ai-key-qwmv-0003";
/// 다른 기기 세션의 접속 키(진짜 키처럼 보이지 않는다).
const ACCESS_KEY: &str = "zz-round40-access-key-qwmv";
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

    /// 그만큼 자려 한 적이 있는가.
    pub fn slept(&self, duration: Duration) -> bool {
        self.slept.lock().unwrap().contains(&duration)
    }

    /// 지금(유닉스 ms).
    pub fn now_ms(&self) -> i64 {
        self.at.load(Ordering::SeqCst)
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
                "id": 4_000_040, "is_bot": true, "first_name": "시험 봇", "username": BOT
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

    /// 이제부터 모든 요청에 401(토큰이 막혔다).
    pub fn revoke(&self) {
        self.state.revoked.store(true, Ordering::SeqCst);
    }

    /// 다음 `getUpdates` 한 번에 429(`retry_after` = `secs`).
    pub fn rate_limit_once(&self, secs: u64) {
        *self.state.rate_limit_next.lock().unwrap() = Some(secs);
    }

    /// 다음 `getUpdates` `times`번에 502.
    pub fn fail_updates(&self, times: u32) {
        self.state.fail_updates.store(times, Ordering::SeqCst);
    }

    /// `sendMessage`를 모두 500으로(켜고 끈다).
    pub fn fail_send(&self, on: bool) {
        self.state.fail_send.store(on, Ordering::SeqCst);
    }

    /// 받은 요청 모두.
    pub async fn requests(&self) -> Vec<Request> {
        self.server.received_requests().await.unwrap_or_default()
    }

    /// 그 메서드로 온 요청 수.
    pub async fn count(&self, method: &str) -> usize {
        let tail = format!("/{method}");
        self.requests()
            .await
            .iter()
            .filter(|r| r.url.path().ends_with(&tail))
            .count()
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

    /// `getUpdates` 요청들의 `offset`(차례대로, 없으면 `None`).
    pub async fn offsets(&self) -> Vec<Option<i64>> {
        self.requests()
            .await
            .iter()
            .filter(|r| r.url.path().ends_with("/getUpdates"))
            .map(|r| param(r, "offset").and_then(|o| o.parse().ok()))
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

    /// 그 메서드 요청이 `at_least`개가 될 때까지 기다린다.
    pub async fn wait_count(&self, method: &str, at_least: usize) {
        let deadline = Instant::now() + STEP;
        while self.count(method).await < at_least {
            assert!(
                Instant::now() < deadline,
                "시간 안에 {method} 요청이 {at_least}개가 되지 않았다"
            );
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

/// 가짜 증권사(토스): 토큰 · 현재가(시계의 지금 시각으로 [`HYNIX`] 한 종목, 진입가) · 국내 달력.
/// 시세 흐름이 바퀴마다 시세를 받게 한다(round17의 가짜 시세 서버와 같은 모양).
async fn fake_broker(clock: Arc<TestClock>) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/oauth2/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "access_token": "zz-round40-quote-token", "token_type": "Bearer", "expires_in": 86_400
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
                "lastPrice": ENTRY.to_string(), "currency": "KRW"
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

fn wiring(clock: &Arc<TestClock>, remote: &Arc<AtomicBool>, broker: &MockServer) -> Wiring {
    let dead = dead_base_url();
    Wiring {
        upstreams: Upstreams {
            toss: TossConfig {
                base_url: broker.uri(),
                timeout: Duration::from_secs(3),
                token_failure_cooldown: Duration::ZERO,
                ..TossConfig::default()
            },
            anthropic: AnthropicConfig {
                base_url: dead,
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

/// 시험 메신저 조립: 가짜 텔레그램 주소 · long polling 1초 · 요청 3초 · 물러서기 3→7초 · 보내기 3번.
pub fn messenger_wiring(tg: &FakeTelegram) -> MessengerWiring {
    MessengerWiring {
        telegram_api_base: tg.uri(),
        poll_timeout_secs: 1,
        request_timeout: Duration::from_secs(3),
        backoff_initial: BACKOFF_INITIAL,
        backoff_max: BACKOFF_MAX,
        send_attempts: 3,
        // 41차: 묶음 창 칸이 더해졌다(이 발판은 팀을 잇지 않아 논의 글이 없다) — 기본값.
        ..MessengerWiring::default()
    }
}

/// 무엇이든 받는 시험용 확인자(바깥으로 나가지 않는다).
pub struct AcceptAll;

impl Verifier for AcceptAll {
    async fn verify(&self, _kind: KeyKind, _values: &Credentials) -> Result<(), VerifyFailure> {
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
    dir
}

/// 띄우기 전 보관 폴더에 공급자 키 하나를 등록한다(가리기 대상이 된다).
pub async fn register_provider(home: &Path, keys: ProviderKeys, field: &str, value: &str) {
    let secrets = SecretStore::open(home).unwrap();
    let values = Credentials::new().with(field, value);
    secrets
        .register_provider(keys, values, &AcceptAll)
        .await
        .unwrap();
}

/// 띄우기: `tg`가 있으면 그 가짜 텔레그램으로 메신저를 잇고([`App::launch_with_messenger`]), 없으면 메신저 없이
/// 같은 조립으로 띄운다([`App::launch_with`] — 발판 자체를 볼 때).
async fn launch(
    home: &Path,
    clock: &Arc<TestClock>,
    remote: &Arc<AtomicBool>,
    broker: &MockServer,
    tg: Option<&FakeTelegram>,
) -> App {
    let launch = Launch {
        trading: trading_wiring(),
        team: None,
        secrets: None,
    };
    let reach = Reach {
        push: None,
        backup: None,
    };
    let settings = settings(home);
    let wiring = wiring(clock, remote, broker);
    let app = match tg {
        Some(tg) => {
            let messenger = messenger_wiring(tg);
            tokio::time::timeout(
                STEP,
                App::launch_with_messenger(settings, wiring, launch, reach, messenger),
            )
            .await
        }
        None => tokio::time::timeout(STEP, App::launch_with(settings, wiring, launch, reach)).await,
    };
    app.expect("서버가 시간 안에 떠야 한다")
        .expect("서버가 떠야 한다")
}

// ───────────── 띄운 서버 ─────────────

/// 떠 있는 서버 하나와 그 보관 폴더 · 시계 · 상대 주소 · 가짜 증권사.
pub struct Bench {
    dir: TempDir,
    clock: Arc<TestClock>,
    remote: Arc<AtomicBool>,
    broker: MockServer,
    app: App,
}

impl Bench {
    /// 새 보관 폴더(필수 키 등록)로 월요일 10시에 띄운다.
    pub async fn start(tg: &FakeTelegram) -> Bench {
        Bench::start_in(fresh_home().await, tg).await
    }

    /// 그 보관 폴더로 월요일 10시에 띄운다.
    pub async fn start_in(dir: TempDir, tg: &FakeTelegram) -> Bench {
        Bench::assemble(dir, Some(tg)).await
    }

    /// 메신저 없이 같은 조립(가짜 증권사 · 시계 · 모의계좌)으로 띄운다 — 발판 자체를 볼 때.
    pub async fn start_plain() -> Bench {
        Bench::assemble(fresh_home().await, None).await
    }

    async fn assemble(dir: TempDir, tg: Option<&FakeTelegram>) -> Bench {
        let clock = TestClock::at(kst(monday(), 10, 0));
        let remote = Arc::new(AtomicBool::new(false));
        let broker = fake_broker(Arc::clone(&clock)).await;
        let app = launch(dir.path(), &clock, &remote, &broker, tg).await;
        Bench {
            dir,
            clock,
            remote,
            broker,
            app,
        }
    }

    /// 토큰을 등록하고 받는 중이 될 때까지 기다린 서버.
    pub async fn with_token(tg: &FakeTelegram) -> Bench {
        let bench = Bench::start(tg).await;
        let reply = bench.register(TOKEN).await;
        assert_eq!(reply.status, 200, "토큰 등록: {}", reply.text);
        bench.wait_state("polling").await;
        bench
    }

    /// 끄고, 같은 보관 폴더 · 같은 시계로 다시 띄운다(상대 주소는 그 컴퓨터로 돌린다).
    pub async fn restart(self, tg: &FakeTelegram) -> Bench {
        let Bench {
            dir,
            clock,
            broker,
            app,
            ..
        } = self;
        tokio::time::timeout(STEP, app.shutdown())
            .await
            .expect("시간 안에 꺼져야 한다")
            .expect("끄기가 성공해야 한다");
        let remote = Arc::new(AtomicBool::new(false));
        let app = launch(dir.path(), &clock, &remote, &broker, Some(tg)).await;
        Bench {
            dir,
            clock,
            remote,
            broker,
            app,
        }
    }

    pub async fn close(self) {
        tokio::time::timeout(STEP, self.app.shutdown())
            .await
            .expect("시간 안에 꺼져야 한다")
            .expect("끄기가 성공해야 한다");
    }

    pub fn home(&self) -> &Path {
        self.dir.path()
    }

    pub fn store(&self) -> Store {
        self.app.store()
    }

    pub fn addr(&self) -> SocketAddr {
        self.app.local_addr()
    }

    pub fn clock(&self) -> &TestClock {
        &self.clock
    }

    /// 다른 기기의 원격 세션을 받는다(round37과 같은 길). 돌려준 뒤에도 다른 기기로 보인다.
    pub async fn remote_login(&self) -> String {
        let salt = vec![7_u8; 16];
        let mut hasher = Sha256::new();
        hasher.update(&salt);
        hasher.update(ACCESS_KEY.as_bytes());
        let hash = AccessKeyHash {
            sha256: hasher.finalize().into(),
            salt,
        };
        self.store()
            .set_access_key(hash, UnixMillis(0))
            .expect("접속 키를 둔다");
        self.remote.store(true, Ordering::SeqCst);
        let origin = origin_of(self.addr());
        let request = reqwest::Client::new()
            .post(format!("{origin}{}", pi_gate::SESSION_PATH))
            .header("Origin", &origin)
            .header("Content-Type", "application/json")
            .body(json!({ pi_gate::ACCESS_KEY_FIELD: ACCESS_KEY }).to_string());
        let reply = tokio::time::timeout(STEP, request.send())
            .await
            .expect("응답이 시간 안에 와야 한다")
            .expect("세션 요청이 닿아야 한다");
        assert!(reply.status().is_success(), "원격 세션을 받아야 한다");
        cookie_of(&reply)
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
        let at = self.clock.now();
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
        let at = ms(self.clock.now());
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

/// 승인 번호의 짧은 번호(앞 [`price_inha::MESSENGER_SHORT_ID_LEN`]자).
pub fn short(number: &ApprovalNumber) -> String {
    number
        .as_str()
        .chars()
        .take(price_inha::MESSENGER_SHORT_ID_LEN)
        .collect()
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

/// 저장된 그 종류 이벤트들의 내용(번호 순서).
pub fn events(store: &Store, kind: &str) -> Vec<Value> {
    store
        .read_after(0, 100_000)
        .unwrap()
        .into_iter()
        .filter(|event| event.kind == kind)
        .map(|event| event.payload)
        .collect()
}

pub fn files_under(dir: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return found;
    };
    for entry in entries {
        let path = entry.unwrap().path();
        if path.is_dir() {
            found.extend(files_under(&path));
        } else {
            found.push(path);
        }
    }
    found
}

/// 보관 폴더의 비밀 보관 파일이 아닌 파일 가운데 `needle`이 든 것.
pub fn plain_files_with(home: &Path, needle: &str) -> Vec<PathBuf> {
    files_under(home)
        .into_iter()
        .filter(|file| {
            file.file_name()
                .is_none_or(|name| name != pi_secrets::ENV_FILE)
        })
        .filter(|file| {
            std::fs::read(file)
                .map(|bytes| String::from_utf8_lossy(&bytes).contains(needle))
                .unwrap_or(false)
        })
        .collect()
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

/// 세션을 받아 같은 출처에서 `DELETE`한다.
pub async fn delete(addr: SocketAddr, path: &str) -> Reply {
    let cookie = login(addr).await;
    send_with(addr, reqwest::Method::DELETE, path, &cookie, None).await
}
