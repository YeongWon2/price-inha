//! round24 테스트가 함께 쓰는 도우미: 시계 · 달력 · 모의계좌 설정 · 가짜 서버(자료 · 시세 · AI · GitHub) ·
//! 서버 띄우기 · HTTP · 매매. round18의 공용 장치를 본떠 두고(그 폴더는 고치지 않는다) 21차에 맞게 줄이고 넓혔다:
//! 가짜 시세 서버가 종목을 "거절"(404)할 수 있고, 백업 시험용 가짜 GitHub · age 키가 있다.
//!
//! 서버는 임시 보관 폴더 · `127.0.0.1:0`으로 띄운다([`price_inha::App::launch_with`]). 시연 모드의 AI는
//! 시연용 가짜 공급자이고, 시연이 아닌 모드는 로컬 가짜 시세 서버(종목별 값을 테스트가 정한다)와 로컬 가짜
//! AI 서버(wiremock, 받은 요청을 센다)를 쓴다. 자료(DART · ECOS · FRED) · 네이버 뉴스도 로컬 가짜 서버 하나다.
//! 지켜볼 종목은 `--symbols`가 아니라 보관 폴더의 저장된 목록으로 준다(화면에서 바꿀 수 있게).

use std::collections::{BTreeMap, HashMap, HashSet};
use std::net::SocketAddr;
use std::path::Path;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use chrono::{DateTime, Datelike, FixedOffset, NaiveDate, TimeZone, Utc, Weekday};
use pi_ai::AnthropicConfig;
use pi_approval::{Approvals, WaitTimes};
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
    Credentials, KeyKind, KeySlot, ProviderKeys, SecretStore, SecretString, Verifier, VerifyFailure,
};
use pi_store::{
    AccountKind, Amount, Judgment, NewTrade, RoleScore, SourceRef, Store, TeamState, TradeId,
    TradeState, UnixMillis,
};
use price_inha::{
    App, BackupWiring, CalendarFn, DayPlan, Launch, OrderRequest, Placed, Reach, Settings,
    TEMPLATE_ADJUST_PATH, TEMPLATE_PATH, TeamWiring, Trading, TradingWiring, Upstreams, Wiring,
    save_watchlist,
};
use rust_decimal::Decimal;
use serde_json::{Value, json};
use tempfile::TempDir;
use wiremock::matchers::{method, path_regex};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

/// 한 걸음을 기다리는 가장 긴 시간.
pub const STEP: Duration = Duration::from_secs(15);
/// 시험용 GitHub 토큰 값(진짜 키처럼 보이지 않는다).
pub const GITHUB_TOKEN: &str = "test-github-token-round24";
/// 백업을 올리는 저장소 이름.
pub const REPO: &str = "owner/price-inha-backup";
/// 지켜볼 종목.
pub const WATCHED: &str = "005930";
/// 논의 · 매매하는 종목.
pub const HYNIX: &str = "000660";
/// 가짜 시세 서버가 [`WATCHED`]에 처음 주는 값(원).
pub const WATCHED_PRICE: i64 = 72_000;
const CORP_WATCHED: &str = "00126380";
const CORP_HYNIX: &str = "00164779";
const DART_KEY: &str = "test-dart-key-round18-zz";
const ECOS_KEY: &str = "test-ecos-key-round18-zz";
/// 매매 근거의 진입가 · 손절값 · 목표값 · 주당 기대값(원).
pub const ENTRY: i64 = 100_000;
pub const STOP: i64 = 95_000;
pub const TARGET: i64 = 110_000;
const EDGE: i64 = 5_000;

/// 글자를 십진수로.
pub fn d(text: &str) -> Decimal {
    text.parse().unwrap()
}

/// 팀장의 판단(근거 칸이 다 찬 것). `decision`은 `"buy"` 또는 `"sell"`.
pub fn verdict(decision: &str) -> String {
    json!({
        "decision": decision, "hypothesis": "자사주 매입으로 수급 개선", "trigger": "자기주식취득결정 공시",
        "entry": ENTRY, "stop": STOP, "target": TARGET, "expected_value": EDGE
    })
    .to_string()
}

/// 잠금을 쥔 채 죽은 스레드가 있어도 값을 읽는다.
fn locked<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

// ───────────── 시계 · 달력 ─────────────

/// 테스트가 정하는 시계.
pub struct TestClock(AtomicI64);

impl TestClock {
    fn at(time: DateTime<Utc>) -> Arc<Self> {
        Arc::new(Self(AtomicI64::new(time.timestamp_millis())))
    }

    fn set(&self, time: DateTime<Utc>) {
        self.0.store(time.timestamp_millis(), Ordering::SeqCst);
    }

    fn read(&self) -> DateTime<Utc> {
        Utc.timestamp_millis_opt(self.0.load(Ordering::SeqCst))
            .unwrap()
    }
}

#[async_trait]
impl Clock for TestClock {
    fn now(&self) -> DateTime<Utc> {
        self.read()
    }

    async fn sleep(&self, duration: Duration) {
        tokio::time::sleep(duration.min(Duration::from_millis(10))).await;
    }
}

/// 승인 모듈이 읽는 같은 시계.
struct ApprovalClock(Arc<TestClock>);

impl pi_approval::Clock for ApprovalClock {
    fn now(&self) -> UnixMillis {
        ms(self.0.read())
    }
}

/// 2026-11-16 월요일.
pub fn monday() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 11, 16).unwrap()
}

/// 2026-11-17 화요일.
pub fn tuesday() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 11, 17).unwrap()
}

fn seoul() -> FixedOffset {
    FixedOffset::east_opt(9 * 3600).unwrap()
}

/// 그 날의 한국 시각.
pub fn kst(date: NaiveDate, hour: u32, minute: u32) -> DateTime<Utc> {
    seoul()
        .from_local_datetime(&date.and_hms_opt(hour, minute, 0).unwrap())
        .unwrap()
        .with_timezone(&Utc)
}

/// `seconds`초 뒤.
pub fn plus(at: DateTime<Utc>, seconds: i64) -> DateTime<Utc> {
    at + chrono::Duration::seconds(seconds)
}

/// 유닉스 밀리초.
pub fn ms(at: DateTime<Utc>) -> UnixMillis {
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

/// 그 날의 국내 세션(정규장 09:00~15:30). 주말은 휴장(세션 없음)이다.
fn kr_day(date: NaiveDate) -> TradingDay {
    let sessions = if matches!(date.weekday(), Weekday::Sat | Weekday::Sun) {
        Vec::new()
    } else {
        vec![Session {
            kind: SessionKind::Regular,
            start: kst(date, 9, 0),
            end: kst(date, 15, 30),
        }]
    };
    TradingDay { date, sessions }
}

fn kr_calendar(date: NaiveDate) -> MarketCalendar {
    MarketCalendar {
        market: Market::Kr,
        today: kr_day(date),
        previous_business_day: kr_day(business_day(date, false)),
        next_business_day: kr_day(business_day(date, true)),
    }
}

// ───────────── 자료 · 뉴스 가짜 서버 ─────────────

/// 자료 가짜 서버(DART · ECOS · FRED · 네이버 뉴스 한 곳). 이 시험에서는 어느 자료도 없다고 답한다
/// (자료를 부르는 논의가 이 시험의 관심이 아니다).
pub struct DataServer {
    server: MockServer,
}

fn ok(body: Value) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(body)
}

fn query_of(request: &Request, name: &str) -> Option<String> {
    request
        .url
        .query_pairs()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value.into_owned())
}

/// 어느 자료 요청에도 "조회된 데이터가 없다"로 답한다.
fn no_data(_request: &Request) -> ResponseTemplate {
    ok(json!({"status": "013", "message": "조회된 데이타가 없습니다."}))
}

impl DataServer {
    async fn start() -> DataServer {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(no_data)
            .mount(&server)
            .await;
        DataServer { server }
    }

    /// 이 가짜 서버를 쓰는 팀 조립 값.
    fn team(&self, plan: &Plan) -> TeamWiring {
        let mut team = TeamWiring::official();
        let uri = self.server.uri();
        team.data = DataConfig::new(uri.clone(), uri.clone(), uri.clone());
        team.data.timeout = Duration::from_secs(3);
        team.corp_codes = HashMap::from([
            (Symbol::new(WATCHED), CorpCode::parse(CORP_WATCHED).unwrap()),
            (Symbol::new(HYNIX), CorpCode::parse(CORP_HYNIX).unwrap()),
        ]);
        // 그 밖의 지켜볼 종목에도 고유번호를 준다.
        for symbol in &plan.symbols {
            let code = CorpCode::parse(&format!("90{symbol}")).unwrap();
            team.corp_codes.entry(Symbol::new(*symbol)).or_insert(code);
        }
        team.news = NaverNewsConfig::new(uri).with_timeout(Duration::from_secs(3));
        team.data_poll = Duration::from_secs(3600);
        team
    }
}

// ───────────── 시세 · AI 가짜 서버(시연이 아닌 모드) ─────────────

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

/// 종목별 값(원). 없는 종목은 물어도 답에 싣지 않는다(그 종목의 시세가 끊긴다).
type Prices = Arc<Mutex<HashMap<String, i64>>>;

/// 증권사가 거절하는(404 `stock-not-found`) 종목들. 이 종목이 든 요청은 통째로 거절된다.
type Rejects = Arc<Mutex<HashSet<String>>>;

/// 가짜 시세 서버: 토큰 · 현재가(물은 종목 가운데 값이 있는 것만, 시계의 지금 시각으로) · 국내 달력.
async fn fake_quotes(clock: Arc<TestClock>, prices: Prices, rejects: Rejects) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "access_token": "test-quote-token-not-real", "token_type": "Bearer", "expires_in": 86_400
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .respond_with(move |request: &Request| {
            let now = clock.read().with_timezone(&seoul());
            if request.url.path() == "/api/v1/market-calendar/KR" {
                let today = now.date_naive();
                return ok(json!({"result": {
                    "today": toss_day(today),
                    "previousBusinessDay": toss_day(business_day(today, false)),
                    "nextBusinessDay": toss_day(business_day(today, true)),
                }}));
            }
            let asked = query_of(request, "symbols").unwrap_or_default();
            let refused = locked(&rejects).clone();
            if asked.split(',').any(|symbol| refused.contains(symbol)) {
                return ResponseTemplate::new(404).set_body_json(json!({
                    "error": {"requestId": "r", "code": "stock-not-found", "message": "m"}
                }));
            }
            let known = locked(&prices).clone();
            let stamp = now.format("%Y-%m-%dT%H:%M:%S%.3f%:z").to_string();
            let result: Vec<Value> = asked
                .split(',')
                .filter_map(|symbol| {
                    let price = known.get(symbol)?;
                    Some(json!({
                        "symbol": symbol, "timestamp": stamp,
                        "lastPrice": price.to_string(), "currency": "KRW"
                    }))
                })
                .collect();
            ok(json!({ "result": result }))
        })
        .mount(&server)
        .await;
    server
}

/// 요청 글로 턴을 골라 답한다(시연용 가짜 AI와 같은 규칙): 팀장은 언제나 사기로 판단한다.
fn ai_reply(system: &str, last: &str) -> String {
    let role = system
        .split("담당 id: ")
        .nth(1)
        .and_then(|rest| rest.split_whitespace().next())
        .unwrap_or("leader");
    if last.contains("[하루 판단]") {
        json!({"plan": "trade"}).to_string()
    } else if last.contains("[복기]") {
        json!({"mistakes": [], "note": "시험 복기"}).to_string()
    } else if last.contains("[점수]") {
        json!({"score": 70, "reason": "시험 점수"}).to_string()
    } else if last.contains("판단을 JSON 객체 하나로") {
        verdict("buy")
    } else if last.contains("[주문 안]") {
        json!({"side": "buy", "note": "시험 주문 안"}).to_string()
    } else {
        match role {
            "scout" => "새 자료를 확인했다. @strategy @risk 의견을 부탁한다".to_string(),
            "strategy" => "전략상 진입할 만하다.".to_string(),
            "risk" => "손절 기준을 지키면 감당할 만하다.".to_string(),
            _ => "(pass)".to_string(),
        }
    }
}

/// 가짜 AI 서버: 대화 요청(`POST /v1/messages`)에 [`ai_reply`]의 글 하나로 답한다.
async fn fake_ai() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(move |request: &Request| {
            let body: Value = serde_json::from_slice(&request.body).unwrap_or(Value::Null);
            let system = body["system"].as_str().unwrap_or_default();
            let last = body["messages"]
                .as_array()
                .and_then(|messages| messages.iter().rev().find(|m| m["role"] == json!("user")))
                .and_then(|message| message["content"].as_str())
                .unwrap_or_default();
            ok(json!({
                "id": "msg_test", "type": "message", "role": "assistant", "model": "test-model",
                "content": [{"type": "text", "text": ai_reply(system, last)}],
                "stop_reason": "end_turn",
                "usage": {"input_tokens": 1, "output_tokens": 1}
            }))
        })
        .mount(&server)
        .await;
    server
}

/// 시연이 아닌 모드의 가짜 바깥 서버들. 다시 켜도 그대로 쓴다.
struct Live {
    quotes: MockServer,
    ai: MockServer,
    prices: Prices,
    rejects: Rejects,
}

// ───────────── 서버 띄우기 ─────────────

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
    TradingWiring {
        paper,
        rules: RulesConfig {
            banned_symbols: Vec::new(),
            sectors: HashMap::from([(Symbol::new(HYNIX), "반도체".to_string())]),
        },
        calendar: Some(calendar),
    }
}

fn no_env(_name: &str) -> Option<String> {
    None
}

/// 설정: 임시 보관 폴더 · 빈 포트 · 짧은 시세 간격과 다시 시도 간격. 종목은 저장된 목록에서 읽는다.
fn settings(home: &Path, demo: bool) -> Settings {
    let home = home.to_str().unwrap();
    let mut list = vec![
        "--home",
        home,
        "--port",
        "0",
        "--interval-ms",
        "20",
        "--retry-initial-ms",
        "20",
        "--retry-max-ms",
        "40",
    ];
    if demo {
        list.push("--demo");
    }
    let args: Vec<String> = list.iter().map(|s| (*s).to_string()).collect();
    Settings::parse(&args, &no_env, None).expect("설정을 읽어야 한다")
}

/// 아무도 듣지 않는 로컬 주소(실제 바깥 주소로 나가지 않게).
fn dead_base_url() -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    format!("http://127.0.0.1:{port}")
}

/// 바깥 자리를 정한 조립 값. 주소가 없으면 아무도 듣지 않는 로컬 주소다.
fn wiring(clock: Arc<TestClock>, live: Option<&Live>) -> Wiring {
    let quotes = live.map_or_else(dead_base_url, |live| live.quotes.uri());
    let ai = live.map_or_else(dead_base_url, |live| live.ai.uri());
    Wiring {
        upstreams: Upstreams {
            toss: TossConfig {
                base_url: quotes,
                timeout: Duration::from_secs(3),
                token_failure_cooldown: Duration::ZERO,
                ..TossConfig::default()
            },
            anthropic: AnthropicConfig {
                base_url: ai,
                timeout: Duration::from_secs(3),
                max_retries: 0,
                retry_base_delay: Duration::from_millis(1),
            },
        },
        clock,
        peer_override: None,
        cli_locations: Vec::new(),
    }
}

/// 무엇이든 받는 시험용 확인자(바깥으로 나가지 않는다).
struct AcceptAll;

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

/// 띄우기 전 보관 폴더에 키를 등록하고 지켜볼 종목을 저장한다. 값은 진짜 키처럼 보이지 않는다.
async fn prepare(home: &Path, plan: &Plan) {
    let secrets = SecretStore::open(home).unwrap();
    let broker = Credentials::new()
        .with("client_id", "test-client-id-round18")
        .with("client_secret", "test-client-secret-round18");
    secrets
        .register(KeyKind::Broker, broker, &AcceptAll)
        .await
        .unwrap();
    let ai = Credentials::new().with("api_key", "test-ai-key-round18-zz");
    secrets
        .register(KeyKind::AiApiKey, ai, &AcceptAll)
        .await
        .unwrap();
    let mut slots = Vec::new();
    slots.extend([(KeySlot::Dart, DART_KEY), (KeySlot::Ecos, ECOS_KEY)]);
    if plan.backup.is_some() {
        slots.push((KeySlot::BackupGithubToken, GITHUB_TOKEN));
    }
    for (slot, value) in slots {
        let values = Credentials::new().with(slot.fields()[0], value);
        secrets
            .register_slot(slot, values, &AcceptAll)
            .await
            .unwrap();
    }
    let symbols: Vec<Symbol> = plan.symbols.iter().map(|s| Symbol::new(*s)).collect();
    save_watchlist(home, &symbols).unwrap();
}

/// 띄울 모양: 시연인가 · 지켜볼 종목 · 어떤 키를 등록하나 · 자료 간격 · 네이버 · DART 하루 상한.
pub struct Plan {
    live: bool,
    symbols: Vec<&'static str>,
    prices: Vec<(&'static str, i64)>,
    backup: Option<BackupPlan>,
}

/// 백업을 올릴 가짜 GitHub 주소와 age 수신자.
#[derive(Clone)]
struct BackupPlan {
    api_base: String,
    recipient: String,
}

impl Plan {
    /// 시연 모드, 지켜볼 종목 [`WATCHED`], 자료 키(DART · ECOS) 있음, 자료 간격 1시간.
    pub fn demo() -> Plan {
        Plan {
            live: false,
            symbols: vec![WATCHED],
            prices: Vec::new(),
            backup: None,
        }
    }

    /// 시연이 아닌 모드(로컬 가짜 시세 · AI 서버). 나머지는 [`Plan::demo`]와 같다.
    pub fn live() -> Plan {
        Plan {
            live: true,
            ..Plan::demo()
        }
    }

    /// 지켜볼 종목(저장된 목록).
    pub fn watching(self, symbols: &[&'static str]) -> Plan {
        Plan {
            symbols: symbols.to_vec(),
            ..self
        }
    }

    /// 띄우기 전에 가짜 시세 서버에 그 종목의 값을 넣는다([`WATCHED`] 말고도).
    pub fn priced(self, symbol: &'static str, price: i64) -> Plan {
        let mut prices = self.prices;
        prices.push((symbol, price));
        Plan { prices, ..self }
    }

    /// 백업을 그 가짜 GitHub에 그 age 키로 올린다(GitHub 토큰 칸을 등록해 둔다).
    pub fn backing_up(self, github: &FakeGithub, age: &AgeKeys) -> Plan {
        let backup = BackupPlan {
            api_base: github.server.uri(),
            recipient: age.recipient.clone(),
        };
        Plan {
            backup: Some(backup),
            ..self
        }
    }

    /// 이 모양으로 그 시각에 띄운다.
    pub async fn start(self, at: DateTime<Utc>) -> Bench {
        let dir = tempfile::tempdir().unwrap();
        prepare(dir.path(), &self).await;
        let clock = TestClock::at(at);
        let live = if self.live {
            let mut initial = HashMap::from([(WATCHED.to_string(), WATCHED_PRICE)]);
            for (symbol, price) in &self.prices {
                initial.insert((*symbol).to_string(), *price);
            }
            let prices: Prices = Arc::new(Mutex::new(initial));
            let rejects: Rejects = Arc::new(Mutex::new(HashSet::new()));
            let quotes = fake_quotes(
                Arc::clone(&clock),
                Arc::clone(&prices),
                Arc::clone(&rejects),
            )
            .await;
            Some(Live {
                quotes,
                ai: fake_ai().await,
                prices,
                rejects,
            })
        } else {
            None
        };
        let data = DataServer::start().await;
        let app = boot(dir.path(), &self, &clock, live.as_ref(), &data).await;
        Bench {
            dir,
            clock,
            app,
            _data: data,
            live,
        }
    }
}

async fn boot(
    home: &Path,
    plan: &Plan,
    clock: &Arc<TestClock>,
    live: Option<&Live>,
    data: &DataServer,
) -> App {
    let launch = Launch {
        trading: trading_wiring(),
        team: Some(data.team(plan)),
        secrets: None,
    };
    let backup = plan.backup.as_ref().map(|backup| BackupWiring {
        api_base: backup.api_base.clone(),
        repo: REPO.to_string(),
        recipient: backup.recipient.clone(),
    });
    let reach = Reach { push: None, backup };
    let start = App::launch_with(
        settings(home, !plan.live),
        wiring(Arc::clone(clock), live),
        launch,
        reach,
    );
    tokio::time::timeout(STEP, start)
        .await
        .expect("서버가 시간 안에 떠야 한다")
        .expect("서버가 떠야 한다")
}

/// 떠 있는 서버 하나와 그 보관 폴더 · 시계 · 가짜 서버들.
pub struct Bench {
    dir: TempDir,
    clock: Arc<TestClock>,
    pub app: App,
    /// 띄워 둔 자료 가짜 서버(살려 두기만 한다).
    _data: DataServer,
    live: Option<Live>,
}

impl Bench {
    /// 끈다.
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

    pub fn trading(&self) -> Arc<Trading> {
        self.app.trading()
    }

    fn live(&self) -> &Live {
        self.live.as_ref().expect("시연이 아닌 모드다")
    }

    /// 가짜 시세 서버가 그 종목에 줄 값을 정한다.
    pub fn set_price(&self, symbol: &str, price: i64) {
        locked(&self.live().prices).insert(symbol.to_string(), price);
    }

    /// 증권사가 그 종목을 거절하게 한다(그 종목이 든 시세 요청이 404로 통째로 거절된다).
    pub fn reject_symbol(&self, symbol: &str) {
        locked(&self.live().rejects).insert(symbol.to_string());
    }

    /// 가짜 시세 서버가 받은 현재가 요청들의 `symbols` 값(받은 순서).
    pub async fn asked_symbols(&self) -> Vec<String> {
        let received = self.live().quotes.received_requests().await;
        received
            .unwrap_or_default()
            .iter()
            .filter(|request| request.url.path() == "/api/v1/prices")
            .filter_map(|request| query_of(request, "symbols"))
            .collect()
    }

    pub fn addr(&self) -> SocketAddr {
        self.app.local_addr()
    }

    pub fn set(&self, at: DateTime<Utc>) {
        self.clock.set(at);
    }

    pub fn now(&self) -> DateTime<Utc> {
        self.clock.read()
    }

    pub fn team(&self) -> TeamState {
        self.store().team_state().unwrap()
    }

    /// 승인 모듈의 눈으로 같은 저장소를 본다(같은 시계).
    pub fn approvals(&self) -> Approvals {
        let clock = Arc::new(ApprovalClock(Arc::clone(&self.clock)));
        Approvals::new(self.store(), clock, WaitTimes::default())
    }

    /// 시계를 그 시각으로 옮기고 그 시각에 받은 시세 하나를 넣는다.
    pub async fn quote(&self, symbol: &str, price: i64, at: DateTime<Utc>) {
        self.set(at);
        let quote = Quote {
            symbol: Symbol::new(symbol),
            price: Decimal::from(price),
            currency: Currency::Krw,
            exchange_time: Some(at),
            received_at: at,
        };
        self.trading()
            .on_quote(quote)
            .await
            .expect("시세를 받아야 한다");
    }

    /// `seconds`초 뒤로 20초씩 나눠 간다(걸음마다 뒤에서 도는 일이 따라오게 잠깐 쉰다).
    pub async fn advance(&self, seconds: i64) {
        let from = self.now();
        let mut gone = 0;
        while gone < seconds {
            gone = (gone + 20).min(seconds);
            self.set(plus(from, gone));
            settle().await;
        }
    }

    pub fn trade_state(&self, id: TradeId) -> TradeState {
        self.store().get_trade(id).unwrap().state
    }

    /// 매매하기 좋게 만든다: 스윙 템플릿을 고르고 종목 비중 10%로 맞춘 뒤 오늘 매매하기로 한다.
    pub async fn ready(&self) {
        let reply = post(self.addr(), TEMPLATE_PATH, &json!({"style": "swing"})).await;
        assert_eq!(reply.status, 200, "템플릿을 골라야 한다: {}", reply.body);
        let id = reply.body["id"].as_i64().expect("템플릿 번호");
        let body = json!({"id": id, "changes": {"symbol_weight_pct": "10"}});
        let reply = post(self.addr(), TEMPLATE_ADJUST_PATH, &body).await;
        assert_eq!(reply.status, 200, "상한 안의 값은 저장된다: {}", reply.body);
        self.trade_today();
    }

    /// 오늘 매매하기로 한다(운용 중).
    pub fn trade_today(&self) {
        let state = self.trading().decide_day(DayPlan::Trade).unwrap();
        assert_eq!(state, TeamState::Operating, "매매하기로 하면 운용 중이다");
    }

    /// 판단됨인 매매 하나(근거 6칸이 다 참). 판단 시각은 지금이다.
    fn judged(&self, symbol: &str) -> TradeId {
        let store = self.store();
        let at = ms(self.now());
        let trade = store
            .create_trade(NewTrade {
                symbol: symbol.to_string(),
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

    pub async fn place(&self, request: OrderRequest) -> Placed {
        self.trading()
            .place(request)
            .await
            .expect("주문 안을 처리해야 한다")
    }

    /// 지금 시세 [`ENTRY`]로 5주를 사서 보유중까지 만든 매매(에이전트를 거치지 않는다). 시계는 1초 간다.
    pub async fn holding(&self, symbol: &str) -> TradeId {
        let at = self.now();
        self.quote(symbol, ENTRY, at).await;
        let trade = self.judged(symbol);
        let placed = self
            .place(order(trade, symbol, Purpose::NewEntry, ENTRY, at))
            .await;
        assert!(matches!(placed, Placed::Ordered(_)), "산다: {placed:?}");
        self.quote(symbol, ENTRY, plus(at, 1)).await;
        assert_eq!(self.trade_state(trade), TradeState::Holding);
        trade
    }
}

/// 지정가 · 수량 5주의 주문 안(VI · 거래정지는 없다고 안다).
pub fn order(
    trade: TradeId,
    symbol: &str,
    purpose: Purpose,
    price: i64,
    at: DateTime<Utc>,
) -> OrderRequest {
    let stage = if purpose.is_buy() {
        TradeStage::Judged
    } else {
        TradeStage::Holding
    };
    let draft = OrderDraft {
        market: Market::Kr,
        symbol: Symbol::new(symbol),
        purpose,
        order_type: OrderType::Limit,
        limit_price: Some(Decimal::from(price)),
        ai_quantity: Some(d("5")),
        funding: Funding::Cash,
        product: ProductKind::Stock,
        stage,
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

// ───────────── 백업: 가짜 GitHub · age 키 ─────────────

#[derive(Default)]
struct Remote {
    files: BTreeMap<String, Vec<u8>>,
}

/// 상태를 가진 가짜 GitHub contents API(토큰을 따진다).
pub struct FakeGithub {
    server: MockServer,
    state: Arc<Mutex<Remote>>,
}

struct Contents(Arc<Mutex<Remote>>);

fn entry(path: &str, bytes: &[u8]) -> Value {
    let name = path.rsplit('/').next().unwrap_or(path);
    let sha = format!(
        "{:016x}",
        bytes.len() * 7919 + usize::from(bytes.first().copied().unwrap_or(0))
    );
    json!({"type": "file", "name": name, "path": path, "sha": sha, "size": bytes.len()})
}

impl Respond for Contents {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        let reply = |status: u16, body: Value| ResponseTemplate::new(status).set_body_json(body);
        let auth = request
            .headers
            .get("authorization")
            .and_then(|v| v.to_str().ok());
        let good = [
            format!("Bearer {GITHUB_TOKEN}"),
            format!("token {GITHUB_TOKEN}"),
        ];
        if !auth.is_some_and(|a| good.iter().any(|g| g == a)) {
            return reply(401, json!({"message": "Bad credentials"}));
        }
        let prefix = format!("/repos/{REPO}/contents/");
        let Some(file) = request.url.path().strip_prefix(&prefix) else {
            return reply(404, json!({"message": "Not Found"}));
        };
        let file = file.to_owned();
        let body: Value = serde_json::from_slice(&request.body).unwrap_or(Value::Null);
        let mut state = locked(&self.0);
        match request.method.as_str() {
            "PUT" => {
                let text: String = body["content"]
                    .as_str()
                    .unwrap_or_default()
                    .chars()
                    .filter(|c| !c.is_whitespace())
                    .collect();
                let Ok(bytes) = STANDARD.decode(text) else {
                    return reply(422, json!({"message": "content is not valid Base64"}));
                };
                let made = entry(&file, &bytes);
                state.files.insert(file, bytes);
                reply(201, json!({"content": made, "commit": {"sha": "c0ffee"}}))
            }
            "GET" => {
                if let Some(bytes) = state.files.get(&file) {
                    return reply(200, entry(&file, bytes));
                }
                let dir = format!("{}/", file.trim_end_matches('/'));
                let listed: Vec<Value> = state
                    .files
                    .iter()
                    .filter(|(name, _)| name.starts_with(&dir))
                    .map(|(name, bytes)| entry(name, bytes))
                    .collect();
                if listed.is_empty() {
                    return reply(404, json!({"message": "Not Found"}));
                }
                reply(200, Value::Array(listed))
            }
            "DELETE" => {
                state.files.remove(&file);
                reply(200, json!({"content": null, "commit": {"sha": "c0ffee"}}))
            }
            _ => reply(405, json!({"message": "Method Not Allowed"})),
        }
    }
}

impl FakeGithub {
    pub async fn start() -> FakeGithub {
        let server = MockServer::start().await;
        let state = Arc::new(Mutex::new(Remote::default()));
        Mock::given(path_regex("^/"))
            .respond_with(Contents(Arc::clone(&state)))
            .mount(&server)
            .await;
        FakeGithub { server, state }
    }

    /// 원격 파일 내용.
    pub fn file(&self, path: &str) -> Vec<u8> {
        locked(&self.state)
            .files
            .get(path)
            .cloned()
            .expect("원격 파일")
    }
}

/// 시험용 age 키 한 쌍(테스트 안에서 만든다).
pub struct AgeKeys {
    recipient: String,
    pub identity: SecretString,
}

impl AgeKeys {
    pub fn generate() -> AgeKeys {
        let identity = age::x25519::Identity::generate();
        let recipient = identity.to_public().to_string();
        let secret = identity.to_string();
        let identity = SecretString::new(age::secrecy::ExposeSecret::expose_secret(&secret));
        AgeKeys {
            recipient,
            identity,
        }
    }
}

/// 보관 폴더의 저장소 파일에 직접 SQL을 낸다. 저장이 실패하는 일을 만들 때(그 종류의 쓰기를 막는 트리거를 달았다가
/// 떼는 일) 쓴다. 다른 연결이 쓰는 중이어도 잠깐 기다린다.
pub fn sql(home: &Path, statements: &str) {
    let conn = rusqlite::Connection::open(home.join(price_inha::DB_FILE)).unwrap();
    conn.busy_timeout(Duration::from_secs(5)).unwrap();
    conn.execute_batch(statements).unwrap();
}

// ───────────── 기다리기 ─────────────

/// 조건이 될 때까지 기다린다(가장 길게 [`STEP`]).
pub async fn wait_for(what: &str, mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + STEP;
    while !done() {
        assert!(Instant::now() < deadline, "시간 안에 되지 않았다: {what}");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// 뒤에서 도는 일(시세 흐름 · 주기 점검 · 깨우기)이 몇 바퀴 돌 만큼 잠깐 쉰다.
pub async fn settle() {
    tokio::time::sleep(Duration::from_millis(300)).await;
}

/// 저장된 이벤트 가운데 그 종류이고 그 날짜인 것의 수.
pub fn on_date(store: &Store, kind: &str, date: NaiveDate) -> usize {
    let date = json!(date.format("%Y-%m-%d").to_string());
    store
        .read_after(0, 100_000)
        .unwrap()
        .into_iter()
        .filter(|event| event.kind == kind && event.payload["date"] == date)
        .count()
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

/// 같은 출처에서 세션을 받아 `Cookie` 헤더 글자를 돌려준다.
async fn login(addr: SocketAddr) -> String {
    let origin = origin_of(addr);
    let reply = reqwest::Client::new()
        .post(format!("{origin}{}", pi_gate::SESSION_PATH))
        .header("Origin", &origin)
        .send()
        .await
        .expect("세션 요청이 닿아야 한다");
    assert!(reply.status().is_success(), "세션을 받아야 한다");
    let raw = reply
        .headers()
        .get("set-cookie")
        .expect("세션 쿠키가 있어야 한다")
        .to_str()
        .unwrap();
    raw.split(';').next().unwrap().trim().to_string()
}

async fn reply_of(request: reqwest::RequestBuilder) -> Reply {
    let reply = tokio::time::timeout(STEP, request.send())
        .await
        .expect("응답이 시간 안에 와야 한다")
        .expect("요청이 닿아야 한다");
    let status = reply.status().as_u16();
    let bytes = reply.bytes().await.expect("본문을 읽어야 한다");
    Reply {
        status,
        body: serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        text: String::from_utf8_lossy(&bytes).into_owned(),
    }
}

/// 세션을 받아 같은 출처에서 보낸다.
pub async fn post(addr: SocketAddr, path: &str, body: &Value) -> Reply {
    let cookie = login(addr).await;
    let request = reqwest::Client::new()
        .post(format!("{}{path}", origin_of(addr)))
        .header("Cookie", cookie)
        .header("Origin", origin_of(addr))
        .header("Content-Type", "application/json")
        .body(body.to_string());
    reply_of(request).await
}

/// 세션을 받아 같은 출처에서 읽는다.
pub async fn get(addr: SocketAddr, path: &str) -> Reply {
    let cookie = login(addr).await;
    let request = reqwest::Client::new()
        .get(format!("{}{path}", origin_of(addr)))
        .header("Cookie", cookie);
    reply_of(request).await
}
