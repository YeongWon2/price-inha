//! round17 테스트가 함께 쓰는 도우미: 시계 · 달력 · 모의계좌 설정 · 가짜 서버(자료 · 웹 푸시 · 시세 · AI) ·
//! 서버 띄우기 · HTTP · 매매.
//!
//! 서버는 임시 보관 폴더 · `127.0.0.1:0`으로 띄운다([`price_inha::App::launch_with`]). 시연 모드
//! ([`Bench::start`])의 AI는 시연용 가짜 공급자이고, 시연이 아닌 모드([`Bench::live`])는 로컬 가짜 시세 서버와
//! 로컬 가짜 AI 서버(wiremock)를 쓴다. 자료 · 웹 푸시도 로컬 가짜 서버뿐이다.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use chrono::{DateTime, Datelike, FixedOffset, NaiveDate, TimeZone, Utc, Weekday};
use pi_ai::AnthropicConfig;
use pi_approval::{Approvals, WaitTimes};
use pi_data::{CorpCode, DataConfig};
use pi_market::{
    Clock, Currency, Market, MarketCalendar, Quote, Session, SessionKind, StalePolicy, Symbol,
    TossConfig, TradingDay,
};
use pi_paper::{
    Broker, InitialCash, LimitFillPrice, MarketRules, MaxFill, MoneyRounding, Order, OrderGroup,
    OrderType, PaperConfig, ProceedsUsable, RoundingMode, SessionRules, Settlement, TickBand,
};
use pi_rules::{
    Funding, OrderDraft, ProductKind, Purpose, Rationale, RulesConfig, SourceStamp, SymbolStatus,
    TradeStage,
};
use pi_secrets::{Credentials, KeyKind, KeySlot, SecretStore, Verifier, VerifyFailure};
use pi_store::{
    AccountKind, Amount, Judgment, NewTrade, RoleScore, SourceRef, Store, TeamState, TradeId,
    TradeState, UnixMillis,
};
use price_inha::{
    App, CalendarFn, Crew, DayPlan, DemoTurn, Launch, OrderRequest, PUSH_SUBSCRIBE_PATH, Placed,
    Reach, Settings, TEMPLATE_ADJUST_PATH, TEMPLATE_PATH, TeamWiring, Trading, TradingWiring,
    Upstreams, Wiring,
};
use rust_decimal::Decimal;
use serde_json::{Value, json};
use tempfile::TempDir;
use wiremock::matchers::{method, path, path_regex, query_param};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

/// 한 걸음을 기다리는 가장 긴 시간.
pub const STEP: Duration = Duration::from_secs(15);
/// 지켜볼 종목(시세 흐름이 준다).
pub const WATCHED: &str = "005930";
/// 논의 · 매매하는 종목. 테스트가 시세를 넣는다.
pub const HYNIX: &str = "000660";
const CORP_WATCHED: &str = "00126380";
const CORP_HYNIX: &str = "00164779";
const DART_KEY: &str = "test-dart-key-round17-zz";
const ECOS_KEY: &str = "test-ecos-key-round17-zz";
/// 매매 근거의 진입가 · 손절값 · 목표값 · 주당 기대값(원).
pub const ENTRY: i64 = 100_000;
pub const STOP: i64 = 95_000;
pub const TARGET: i64 = 110_000;
const EDGE: i64 = 5_000;
/// 브라우저 구독 키 자리: P-256 생성점(65바이트 비압축 점, base64url).
const P256DH: &str =
    "BGsX0fLhLEJH-Lzm5WOkQPJ3A32BLeszoPShOUXYmMKWT-NC4v4af5uO5-tKfA-eFivOM1drMV7Oy7ZAaDe_UfU";
/// 브라우저 구독의 auth(16바이트, base64url).
const AUTH: &str = "AAECAwQFBgcICQoLDA0ODw";

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

// ───────────── 자료 가짜 서버 ─────────────

fn disclosure_body(corp: &str, stock: &str) -> Value {
    json!({
        "status": "000", "message": "정상", "page_no": 1, "page_count": 10,
        "total_count": 1, "total_page": 1,
        "list": [{
            "corp_cls": "Y", "corp_name": "시험회사", "corp_code": corp, "stock_code": stock,
            "report_nm": "주요사항보고서(자기주식취득결정)", "rcept_no": "20261116000123",
            "flr_nm": "시험회사", "rcept_dt": "20261116", "rm": "유"
        }]
    })
}

fn financials_body(corp: &str) -> Value {
    json!({
        "status": "000", "message": "정상",
        "list": [{
            "rcept_no": "20260310000789", "reprt_code": "11011", "bsns_year": "2025",
            "corp_code": corp, "stock_code": "000000", "fs_div": "CFS", "fs_nm": "연결재무제표",
            "sj_div": "IS", "sj_nm": "손익계산서", "account_nm": "매출액", "thstrm_nm": "제 57 기",
            "thstrm_dt": "2025.01.01 ~ 2025.12.31", "thstrm_amount": "300,870,903,000,000",
            "frmtrm_nm": "제 56 기", "frmtrm_dt": "2024.01.01 ~ 2024.12.31",
            "frmtrm_amount": "1,000", "ord": "21", "currency": "KRW"
        }]
    })
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

/// 자료 가짜 서버(DART · ECOS 한 곳). 공시 · 재무 · 금리 · 환율이 모두 있다.
struct DataServer {
    server: MockServer,
}

impl DataServer {
    async fn start() -> DataServer {
        let server = MockServer::start().await;
        let ok = |body: Value| ResponseTemplate::new(200).set_body_json(body);
        for (corp, stock) in [(CORP_WATCHED, WATCHED), (CORP_HYNIX, HYNIX)] {
            Mock::given(method("GET"))
                .and(path("/api/list.json"))
                .and(query_param("corp_code", corp))
                .respond_with(ok(disclosure_body(corp, stock)))
                .mount(&server)
                .await;
            Mock::given(method("GET"))
                .and(path("/api/fnlttSinglAcnt.json"))
                .and(query_param("corp_code", corp))
                .respond_with(ok(financials_body(corp)))
                .mount(&server)
                .await;
        }
        let rate = ecos_body(
            "722Y001",
            "0101000",
            "한국은행 기준금리",
            "연%",
            "202610",
            "2.25",
        );
        Mock::given(method("GET"))
            .and(path_regex("^/api/StatisticSearch/.*/722Y001/"))
            .respond_with(ok(rate))
            .mount(&server)
            .await;
        let fx = ecos_body(
            "731Y001",
            "0000001",
            "원/미국달러",
            "원",
            "20261113",
            "1391.5",
        );
        Mock::given(method("GET"))
            .and(path_regex("^/api/StatisticSearch/.*/731Y001/"))
            .respond_with(ok(fx))
            .mount(&server)
            .await;
        DataServer { server }
    }

    /// 모든 자료 요청이 500으로 실패하게 바꾼다(출처 장애).
    async fn fail(&self) {
        self.server.reset().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&self.server)
            .await;
    }

    /// 이 가짜 서버를 쓰는 팀 조립 값.
    fn team(&self) -> TeamWiring {
        let mut team = TeamWiring::official();
        let uri = self.server.uri();
        team.data = DataConfig::new(uri.clone(), uri.clone(), uri);
        team.data.timeout = Duration::from_secs(3);
        team.corp_codes = HashMap::from([
            (Symbol::new(WATCHED), CorpCode::parse(CORP_WATCHED).unwrap()),
            (Symbol::new(HYNIX), CorpCode::parse(CORP_HYNIX).unwrap()),
        ]);
        team
    }
}

// ───────────── 웹 푸시 가짜 서버 ─────────────

/// 웹 푸시 서비스 가짜. `/push/…`로 오는 POST에 201을 준다.
struct PushServer {
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

    /// 구독 주소(`https://`)의 scheme · host · port를 이 가짜 서버로 바꿔 보내는 길.
    fn transport(&self) -> Arc<dyn pi_notify::PushTransport> {
        let base = url::Url::parse(&self.server.uri()).unwrap();
        let transport = pi_notify::HttpTransport::new(Duration::from_secs(5))
            .unwrap()
            .redirect_to(base);
        Arc::new(transport)
    }
}

// ───────────── 시세 가짜 서버(시연이 아닌 모드) ─────────────

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

/// 가짜 시세 서버: 토큰 · 현재가(`failing`이면 503, 아니면 시계의 지금 시각으로 [`WATCHED`] 한 종목) · 국내 달력.
async fn fake_quotes(clock: Arc<TestClock>, failing: Arc<AtomicBool>) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/oauth2/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "access_token": "test-quote-token-not-real", "token_type": "Bearer", "expires_in": 86_400
        })))
        .mount(&server)
        .await;
    let price_clock = Arc::clone(&clock);
    Mock::given(method("GET"))
        .and(path("/api/v1/prices"))
        .respond_with(move |_: &Request| {
            if failing.load(Ordering::SeqCst) {
                return ResponseTemplate::new(503).set_body_json(json!({
                    "error": {"requestId": "r-1", "code": "maintenance", "message": "점검"}
                }));
            }
            let now = price_clock.read().with_timezone(&seoul());
            ResponseTemplate::new(200).set_body_json(json!({"result": [{
                "symbol": WATCHED, "timestamp": now.format("%Y-%m-%dT%H:%M:%S%.3f%:z").to_string(),
                "lastPrice": "72000", "currency": "KRW"
            }]}))
        })
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v1/market-calendar/KR"))
        .respond_with(move |_: &Request| {
            let today = clock.read().with_timezone(&seoul()).date_naive();
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

// ───────────── AI 가짜 서버(시연이 아닌 모드) ─────────────

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
        .and(path("/v1/messages"))
        .respond_with(|request: &Request| {
            let body: Value = serde_json::from_slice(&request.body).unwrap_or(Value::Null);
            let system = body["system"].as_str().unwrap_or_default();
            let last = body["messages"]
                .as_array()
                .and_then(|messages| messages.iter().rev().find(|m| m["role"] == json!("user")))
                .and_then(|message| message["content"].as_str())
                .unwrap_or_default();
            ResponseTemplate::new(200).set_body_json(json!({
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

/// 설정: 임시 보관 폴더 · 빈 포트 · 짧은 시세 간격과 다시 시도 간격 · 지켜볼 종목 [`WATCHED`]. `demo`면 `--demo`.
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
        "--symbols",
        WATCHED,
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
fn wiring(clock: Arc<TestClock>, quotes: Option<String>, ai: Option<String>) -> Wiring {
    Wiring {
        upstreams: Upstreams {
            toss: TossConfig {
                base_url: quotes.unwrap_or_else(dead_base_url),
                timeout: Duration::from_secs(3),
                token_failure_cooldown: Duration::ZERO,
                ..TossConfig::default()
            },
            anthropic: AnthropicConfig {
                base_url: ai.unwrap_or_else(dead_base_url),
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
}

/// 보관 폴더에 필수 키(증권사 · AI)를 등록한다. `data_keys`면 자료 키(DART · ECOS)도 등록한다(자료 키는
/// 선택이다). 값은 진짜 키처럼 보이지 않는다.
async fn register_keys(home: &Path, data_keys: bool) {
    let secrets = SecretStore::open(home).unwrap();
    let data = [(KeySlot::Dart, DART_KEY), (KeySlot::Ecos, ECOS_KEY)];
    for (slot, value) in data.into_iter().filter(|_| data_keys) {
        let values = Credentials::new().with(slot.fields()[0], value);
        secrets
            .register_slot(slot, values, &AcceptAll)
            .await
            .unwrap();
    }
    let broker = Credentials::new()
        .with("client_id", "test-client-id-round17")
        .with("client_secret", "test-client-secret-round17");
    secrets
        .register(KeyKind::Broker, broker, &AcceptAll)
        .await
        .unwrap();
    let ai = Credentials::new().with("api_key", "test-ai-key-round17-zz");
    secrets
        .register(KeyKind::AiApiKey, ai, &AcceptAll)
        .await
        .unwrap();
}

/// 시연이 아닌 모드의 가짜 바깥 서버들.
struct Live {
    quotes: MockServer,
    _ai: MockServer,
    failing: Arc<AtomicBool>,
}

/// 떠 있는 서버 하나와 그 보관 폴더 · 시계 · 가짜 서버들.
pub struct Bench {
    dir: TempDir,
    clock: Arc<TestClock>,
    pub app: App,
    data: DataServer,
    push: Option<PushServer>,
    live: Option<Live>,
}

impl Bench {
    /// 시연 모드로 그 시각에 띄운다(웹 푸시 없음).
    pub async fn start(at: DateTime<Utc>) -> Bench {
        Bench::launch(at, false, false).await
    }

    /// 시연 모드로 그 시각에 띄우고 웹 푸시 가짜 서버를 잇는다.
    pub async fn with_push(at: DateTime<Utc>) -> Bench {
        Bench::launch(at, true, false).await
    }

    /// 시연이 아닌 모드로 그 시각에 띄운다: 로컬 가짜 시세 · AI 서버를 잇는다.
    pub async fn live(at: DateTime<Utc>) -> Bench {
        Bench::launch(at, false, true).await
    }

    /// 시연이 아닌 모드로 띄우고 웹 푸시 가짜 서버를 잇는다(승인 요청 알림이 "갔다"로 남는다).
    pub async fn live_with_push(at: DateTime<Utc>) -> Bench {
        Bench::launch(at, true, true).await
    }

    /// 시연이 아닌 모드로, 자료 키(DART · ECOS)를 등록하지 않고 띄운다(자료 키는 선택이다).
    pub async fn live_without_data_keys(at: DateTime<Utc>) -> Bench {
        let dir = tempfile::tempdir().unwrap();
        register_keys(dir.path(), false).await;
        Bench::boot(dir, DataServer::start().await, at, false, true).await
    }

    /// 끄고, 같은 보관 폴더로 그 시각에 다시 띄운다(시연 모드, 웹 푸시 없음).
    pub async fn reopen(self, at: DateTime<Utc>) -> Bench {
        let Bench { dir, data, app, .. } = self;
        tokio::time::timeout(STEP, app.shutdown())
            .await
            .expect("시간 안에 꺼져야 한다")
            .expect("끄기가 성공해야 한다");
        Bench::boot(dir, data, at, false, false).await
    }

    async fn launch(at: DateTime<Utc>, push: bool, live: bool) -> Bench {
        let dir = tempfile::tempdir().unwrap();
        register_keys(dir.path(), true).await;
        Bench::boot(dir, DataServer::start().await, at, push, live).await
    }

    /// 그 보관 폴더 · 자료 가짜 서버로 띄운다.
    async fn boot(
        dir: TempDir,
        data: DataServer,
        at: DateTime<Utc>,
        push: bool,
        live: bool,
    ) -> Bench {
        let push = if push {
            Some(PushServer::start().await)
        } else {
            None
        };
        let clock = TestClock::at(at);
        let (live, quotes, ai) = if live {
            let failing = Arc::new(AtomicBool::new(false));
            let quotes = fake_quotes(Arc::clone(&clock), Arc::clone(&failing)).await;
            let ai = fake_ai().await;
            let (quotes_uri, ai_uri) = (quotes.uri(), ai.uri());
            let live = Live {
                quotes,
                _ai: ai,
                failing,
            };
            (Some(live), Some(quotes_uri), Some(ai_uri))
        } else {
            (None, None, None)
        };
        let launch = Launch {
            trading: trading_wiring(),
            team: Some(data.team()),
            secrets: None,
        };
        let reach = Reach {
            push: push.as_ref().map(PushServer::transport),
            backup: None,
        };
        let wiring = wiring(Arc::clone(&clock), quotes, ai);
        let start = App::launch_with(settings(dir.path(), live.is_none()), wiring, launch, reach);
        let app = tokio::time::timeout(STEP, start)
            .await
            .expect("서버가 시간 안에 떠야 한다")
            .expect("서버가 떠야 한다");
        Bench {
            dir,
            clock,
            app,
            data,
            push,
            live,
        }
    }

    /// 끈다.
    pub async fn close(self) {
        tokio::time::timeout(STEP, self.app.shutdown())
            .await
            .expect("시간 안에 꺼져야 한다")
            .expect("끄기가 성공해야 한다");
    }

    pub fn store(&self) -> Store {
        self.app.store()
    }

    pub fn trading(&self) -> Arc<Trading> {
        self.app.trading()
    }

    pub fn crew(&self) -> Arc<Crew> {
        self.app.crew().expect("팀을 이어 띄웠다")
    }

    /// 시연용 가짜 AI의 그 턴의 답을 정한다.
    pub fn script(&self, turn: DemoTurn, reply: &str) {
        let ai = self.crew().demo_ai().expect("시연 모드다");
        ai.script(turn, reply);
    }

    /// 가짜 시세 서버를 장애로 바꾼다(시연이 아닌 모드).
    pub fn fail_quotes(&self) {
        let live = self.live.as_ref().expect("시연이 아닌 모드다");
        live.failing.store(true, Ordering::SeqCst);
    }

    /// 자료 가짜 서버를 장애로 바꾼다(키가 있는 출처에 물어도 모두 실패한다).
    pub async fn fail_data(&self) {
        self.data.fail().await;
    }

    /// 웹 푸시 가짜 서버가 받은 알림 수.
    pub async fn pushes(&self) -> usize {
        let push = self.push.as_ref().expect("웹 푸시를 이어 띄웠다");
        let received = push.server.received_requests().await;
        received.unwrap_or_default().len()
    }

    /// 가짜 시세 서버를 되살리고, 시세 흐름이 지금 시각의 시세를 다시 받을 때까지 기다린다.
    pub async fn resume_quotes(&self) {
        let live = self.live.as_ref().expect("시연이 아닌 모드다");
        live.failing.store(false, Ordering::SeqCst);
        let served = || async {
            let received = live.quotes.received_requests().await.unwrap_or_default();
            let prices = received.iter().filter(|r| r.url.path() == "/api/v1/prices");
            prices.count()
        };
        let from = served().await;
        let deadline = Instant::now() + STEP;
        while served().await < from + 3 {
            assert!(Instant::now() < deadline, "시세를 다시 받아야 한다");
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        settle().await;
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

    /// `from`부터 `seconds`초 동안 20초마다 [`HYNIX`] 시세를 넣는다. 값은 [`ENTRY`]와 `ENTRY + swing`을
    /// 번갈아 낸다(`swing`이 0이면 그대로인 시세). 걸음마다 뒤에서 도는 일이 따라오게 잠깐 쉰다.
    /// 마지막에 넣은 시각을 돌려준다.
    pub async fn stream(&self, swing: i64, from: DateTime<Utc>, seconds: i64) -> DateTime<Utc> {
        let mut offset = 0;
        let mut flip = false;
        loop {
            let price = if flip { ENTRY + swing } else { ENTRY };
            self.quote(HYNIX, price, plus(from, offset)).await;
            tokio::time::sleep(Duration::from_millis(60)).await;
            if offset >= seconds {
                return plus(from, offset);
            }
            offset = (offset + 20).min(seconds);
            flip = !flip;
        }
    }

    pub fn trade_state(&self, id: TradeId) -> TradeState {
        self.store().get_trade(id).unwrap().state
    }

    /// 모의계좌의 그 묶음의 주문들.
    pub async fn orders(&self, group: OrderGroup) -> Vec<Order> {
        let trading = self.trading();
        Broker::orders(trading.paper(), group, None, self.now())
            .await
            .expect("모의계좌 주문을 읽어야 한다")
    }

    /// 매매하기 좋게 만든다: 스윙 템플릿에 `changes`를 맞추고 오늘 매매하기로 한다.
    pub async fn ready_with(&self, changes: Value) {
        swing_with(self.addr(), changes).await;
        self.trade_today();
    }

    /// 종목 비중 10%로 매매하기 좋게 만든다.
    pub async fn ready(&self) {
        self.ready_with(json!({"symbol_weight_pct": "10"})).await;
    }

    /// 오늘 매매하기로 한다(운용 중).
    pub fn trade_today(&self) {
        let state = self.trading().decide_day(DayPlan::Trade).unwrap();
        assert_eq!(state, TeamState::Operating, "매매하기로 하면 운용 중이다");
    }

    /// 판단됨인 매매 하나(근거 6칸이 다 참). 판단 시각은 지금이다.
    pub fn judged(&self, symbol: &str) -> TradeId {
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

// ───────────── 기다리기 · 이벤트 ─────────────

/// 조건이 될 때까지 기다린다(가장 길게 [`STEP`]).
pub async fn wait_for(what: &str, mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + STEP;
    while !done() {
        assert!(Instant::now() < deadline, "시간 안에 되지 않았다: {what}");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// 뒤에서 도는 일(시세 흐름 · 주기 점검)이 몇 바퀴 돌 만큼 잠깐 쉰다.
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

/// HTTP 응답 하나: 상태 · JSON 본문(JSON이 아니면 `Null`).
pub struct Reply {
    pub status: u16,
    pub body: Value,
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

/// 세션을 받아 같은 출처에서 보낸다.
pub async fn post(addr: SocketAddr, path: &str, body: &Value) -> Reply {
    let cookie = login(addr).await;
    let request = reqwest::Client::new()
        .post(format!("{}{path}", origin_of(addr)))
        .header("Cookie", cookie)
        .header("Origin", origin_of(addr))
        .header("Content-Type", "application/json")
        .body(body.to_string());
    let reply = tokio::time::timeout(STEP, request.send())
        .await
        .expect("응답이 시간 안에 와야 한다")
        .expect("요청이 닿아야 한다");
    let status = reply.status().as_u16();
    let bytes = reply.bytes().await.expect("본문을 읽어야 한다");
    Reply {
        status,
        body: serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    }
}

/// 세션을 받아 같은 출처에서 읽는다.
pub async fn get(addr: SocketAddr, path: &str) -> Reply {
    let cookie = login(addr).await;
    let request = reqwest::Client::new()
        .get(format!("{}{path}", origin_of(addr)))
        .header("Cookie", cookie);
    let reply = tokio::time::timeout(STEP, request.send())
        .await
        .expect("응답이 시간 안에 와야 한다")
        .expect("요청이 닿아야 한다");
    let status = reply.status().as_u16();
    let bytes = reply.bytes().await.expect("본문을 읽어야 한다");
    Reply {
        status,
        body: serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    }
}

/// 스윙 템플릿을 고르고 값을 맞춘다.
async fn swing_with(addr: SocketAddr, changes: Value) {
    let reply = post(addr, TEMPLATE_PATH, &json!({"style": "swing"})).await;
    assert_eq!(reply.status, 200, "템플릿을 골라야 한다: {}", reply.body);
    let id = reply.body["id"].as_i64().expect("템플릿 번호");
    let body = json!({"id": id, "changes": changes});
    let reply = post(addr, TEMPLATE_ADJUST_PATH, &body).await;
    assert_eq!(reply.status, 200, "상한 안의 값은 저장된다: {}", reply.body);
}

/// 폰의 웹 푸시 구독 하나(승인 요청 알림이 "갔다"로 남게).
pub async fn subscribe(addr: SocketAddr) {
    let body = json!({
        "endpoint": "https://push.invalid/push/1",
        "p256dh": P256DH, "auth": AUTH, "device": "시험 폰",
    });
    let reply = post(addr, PUSH_SUBSCRIBE_PATH, &body).await;
    assert_eq!(reply.status, 204, "구독을 받아야 한다: {}", reply.body);
}
