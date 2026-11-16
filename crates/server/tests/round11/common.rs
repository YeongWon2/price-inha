//! round11 테스트가 함께 쓰는 도우미: 시계 · 달력 · 모의계좌 설정 · 가짜 서버(자료 · 웹 푸시 · GitHub) ·
//! 서버 띄우기 · HTTP · 매매 · 알림 구독.
//!
//! 서버는 임시 보관 폴더 · `127.0.0.1:0` · 시연 모드로 띄우고 투자 팀과 3차 조립을 잇는다
//! ([`price_inha::App::launch_with`]). AI는 시연 모드의 가짜 공급자뿐이고, 자료 · 웹 푸시 · GitHub는
//! 로컬 가짜 서버(wiremock)뿐이다. 토스 · Anthropic 자리는 아무도 듣지 않는 로컬 주소다.

use std::collections::{BTreeMap, HashMap};
use std::net::SocketAddr;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use chrono::{DateTime, Datelike, NaiveDate, TimeZone, Utc, Weekday};
use pi_ai::AnthropicConfig;
use pi_approval::{ApprovalNumber, Fingerprint};
use pi_data::{CorpCode, DataConfig};
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
    Credentials, KeyKind, KeySlot, SecretStore, SecretString, Verifier, VerifyFailure,
};
use pi_store::{
    AccountKind, Amount, Judgment, NewTrade, RoleScore, SourceRef, Store, TeamState, TradeId,
    TradeState, UnixMillis,
};
use price_inha::{
    App, BackupWiring, CalendarFn, Crew, DayPlan, DemoAi, DemoTurn, Launch, OrderRequest,
    PUSH_SUBSCRIBE_PATH, Placed, Reach, Settings, TEMPLATE_ADJUST_PATH, TEMPLATE_PATH, TeamWiring,
    Trading, TradingWiring, Upstreams, Wiring,
};
use rust_decimal::Decimal;
use serde_json::{Value, json};
use tempfile::TempDir;
use wiremock::matchers::{method, path, path_regex, query_param};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

/// 한 걸음을 기다리는 가장 긴 시간.
pub const STEP: Duration = Duration::from_secs(15);
/// 지켜볼 종목(시연 시세가 흐른다).
pub const WATCHED: &str = "005930";
/// 논의 · 매매하는 종목. 테스트가 시세를 넣는다.
pub const HYNIX: &str = "000660";
const CORP_WATCHED: &str = "00126380";
const CORP_HYNIX: &str = "00164779";
/// 시험용 자료 키(진짜 키처럼 보이지 않는다). 하루 동안 어디에도 새면 안 되는 글자로도 쓴다(V47).
pub const DART_KEY: &str = "test-dart-key-round11-zz";
const ECOS_KEY: &str = "test-ecos-key-round11-zz";
/// 시험용 GitHub 토큰(진짜 토큰 모양이 아니다).
pub const GITHUB_TOKEN: &str = "test-github-token-round11";
/// 가짜 GitHub의 저장소 이름.
pub const REPO: &str = "owner/price-inha-backup";
/// 매매 근거의 진입가 · 손절값 · 목표값 · 주당 기대값(원).
pub const ENTRY: i64 = 100_000;
const STOP: i64 = 95_000;
const TARGET: i64 = 110_000;
const EDGE: i64 = 5_000;
/// 브라우저 구독 키 자리: P-256 생성점(65바이트 비압축 점, base64url). 올바른 점이다.
pub const P256DH: &str =
    "BGsX0fLhLEJH-Lzm5WOkQPJ3A32BLeszoPShOUXYmMKWT-NC4v4af5uO5-tKfA-eFivOM1drMV7Oy7ZAaDe_UfU";
/// 브라우저 구독의 auth(16바이트, base64url).
pub const AUTH: &str = "AAECAwQFBgcICQoLDA0ODw";

/// 글자를 십진수로.
pub fn d(text: &str) -> Decimal {
    text.parse().unwrap()
}

/// 팀장의 사기 판단(근거 칸이 다 찬 것).
pub fn buy_verdict() -> String {
    json!({
        "decision": "buy", "hypothesis": "자사주 매입으로 수급 개선", "trigger": "자기주식취득결정 공시",
        "entry": ENTRY, "stop": STOP, "target": TARGET, "expected_value": EDGE
    })
    .to_string()
}

// ───────────── 시계 · 달력 ─────────────

/// 테스트가 정하는 시계.
pub struct TestClock(AtomicI64);

impl TestClock {
    pub fn at(time: DateTime<Utc>) -> Arc<Self> {
        Arc::new(Self(AtomicI64::new(time.timestamp_millis())))
    }

    pub fn set(&self, time: DateTime<Utc>) {
        self.0.store(time.timestamp_millis(), Ordering::SeqCst);
    }
}

#[async_trait]
impl Clock for TestClock {
    fn now(&self) -> DateTime<Utc> {
        Utc.timestamp_millis_opt(self.0.load(Ordering::SeqCst))
            .unwrap()
    }

    async fn sleep(&self, duration: Duration) {
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
pub struct DataServer {
    pub server: MockServer,
}

impl DataServer {
    pub async fn start() -> DataServer {
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

    /// 받은 요청들의 주소와 본문을 이은 글.
    pub async fn seen(&self) -> String {
        seen_text(&self.server).await
    }

    /// 이 가짜 서버를 쓰는 팀 조립 값.
    pub fn team(&self) -> TeamWiring {
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

/// 가짜 서버가 받은 요청들의 주소와 본문(글로 읽을 수 있는 만큼)을 이은 글.
pub async fn seen_text(server: &MockServer) -> String {
    let received = server.received_requests().await.unwrap_or_default();
    received
        .iter()
        .map(|r| format!("{} {}\n", r.url, String::from_utf8_lossy(&r.body)))
        .collect()
}

// ───────────── 웹 푸시 가짜 서버 ─────────────

/// 웹 푸시 서비스 가짜. `/push/…`로 오는 POST에 201을 준다.
pub struct PushServer {
    pub server: MockServer,
}

impl PushServer {
    pub async fn start() -> PushServer {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path_regex("^/push/"))
            .respond_with(ResponseTemplate::new(201))
            .mount(&server)
            .await;
        PushServer { server }
    }

    /// 구독 주소(`https://`)의 scheme · host · port를 이 가짜 서버로 바꿔 보내는 길.
    pub fn transport(&self) -> Arc<dyn pi_notify::PushTransport> {
        let base = url::Url::parse(&self.server.uri()).unwrap();
        let transport = pi_notify::HttpTransport::new(Duration::from_secs(5))
            .unwrap()
            .redirect_to(base);
        Arc::new(transport)
    }

    /// 받은 푸시 수.
    pub async fn count(&self) -> usize {
        let received = self.server.received_requests().await.unwrap_or_default();
        received
            .iter()
            .filter(|r| r.method.as_str() == "POST" && r.url.path().starts_with("/push/"))
            .count()
    }

    /// 푸시가 `n`개 이상 올 때까지 기다린다. 기다린 시간을 돌려준다.
    pub async fn wait_count(&self, n: usize, within: Duration) -> Duration {
        let started = Instant::now();
        while self.count().await < n {
            assert!(
                started.elapsed() < within,
                "{within:?} 안에 푸시 {n}개가 와야 한다"
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        started.elapsed()
    }
}

// ───────────── GitHub 가짜 서버 ─────────────

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
        bytes.len() * 7919 + bytes.first().copied().unwrap_or(0) as usize
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
        let mut state = self.0.lock().unwrap();
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

    /// 원격에 있는 파일 경로들.
    pub fn paths(&self) -> Vec<String> {
        self.state.lock().unwrap().files.keys().cloned().collect()
    }

    /// 원격 파일 내용.
    pub fn file(&self, path: &str) -> Vec<u8> {
        self.state
            .lock()
            .unwrap()
            .files
            .get(path)
            .cloned()
            .expect("원격 파일")
    }

    /// 받은 PUT 수.
    pub async fn puts(&self) -> usize {
        let received = self.server.received_requests().await.unwrap_or_default();
        received
            .iter()
            .filter(|r| r.method.as_str() == "PUT")
            .count()
    }
}

// ───────────── 설정 · 띄우기 ─────────────

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

/// 설정: 임시 보관 폴더 · 빈 포트 · 짧은 시세 간격. `demo`면 `--demo`.
pub fn settings(home: &Path, symbols: &str, demo: bool) -> Settings {
    let home = home.to_str().unwrap();
    let mut list = vec![
        "--home",
        home,
        "--port",
        "0",
        "--interval-ms",
        "20",
        "--symbols",
        symbols,
    ];
    if demo {
        list.push("--demo");
    }
    let args: Vec<String> = list.iter().map(|s| (*s).to_string()).collect();
    Settings::parse(&args, &no_env, None).expect("설정을 읽어야 한다")
}

/// 아무도 듣지 않는 로컬 주소(실제 바깥 주소로 나가지 않게).
pub fn dead_base_url() -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    format!("http://127.0.0.1:{port}")
}

/// 바깥 자리를 정한 조립 값. `toss`가 있으면 그 주소, 없으면 아무도 듣지 않는 주소다.
/// `remote`가 참인 동안 상대 주소를 `100.64.0.7`로 본다(다른 기기 흉내).
pub fn wiring(clock: Arc<TestClock>, toss: Option<String>, remote: Arc<AtomicBool>) -> Wiring {
    let dead = dead_base_url();
    let peer: pi_gate::PeerOverride = Arc::new(move |addr: SocketAddr| {
        if remote.load(Ordering::SeqCst) {
            SocketAddr::from(([100, 64, 0, 7], addr.port()))
        } else {
            addr
        }
    });
    Wiring {
        upstreams: Upstreams {
            toss: TossConfig {
                base_url: toss.unwrap_or_else(|| dead.clone()),
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
        clock,
        peer_override: Some(peer),
        cli_locations: Vec::new(),
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
}

/// 보관 폴더에 칸 하나의 키를 등록한다.
pub async fn register_slot(home: &Path, slot: KeySlot, value: &str) {
    let secrets = SecretStore::open(home).unwrap();
    let values = Credentials::new().with(slot.fields()[0], value);
    secrets
        .register_slot(slot, values, &AcceptAll)
        .await
        .unwrap();
}

/// 필수 키(증권사 · AI API 키)를 보관 폴더에 등록한다.
pub async fn register_required(home: &Path) {
    let secrets = SecretStore::open(home).unwrap();
    let broker = Credentials::new()
        .with("client_id", "test-client-id-round11")
        .with("client_secret", "test-client-secret-round11");
    secrets
        .register(KeyKind::Broker, broker, &AcceptAll)
        .await
        .unwrap();
    let ai = Credentials::new().with("api_key", "test-ai-key-round11-zz");
    secrets
        .register(KeyKind::AiApiKey, ai, &AcceptAll)
        .await
        .unwrap();
}

/// 시험용 age 키 한 쌍(테스트 안에서 만든다).
pub struct AgeKeys {
    pub recipient: String,
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

/// 떠 있는 서버 하나와 그 보관 폴더 · 시계 · 가짜 서버들.
pub struct Bench {
    dir: TempDir,
    clock: Arc<TestClock>,
    pub remote: Arc<AtomicBool>,
    pub app: App,
    pub data: DataServer,
    pub push: PushServer,
    pub github: FakeGithub,
    pub age: AgeKeys,
}

impl Bench {
    /// 새 보관 폴더로 그 시각에 띄운다.
    pub async fn start(at: DateTime<Utc>) -> Bench {
        Bench::with(tempfile::tempdir().unwrap(), at, None).await
    }

    /// 그 보관 폴더로 그 시각에 띄운다. 자료 키와 GitHub 토큰을 등록해 둔다.
    /// `age`가 있으면 그 키로 백업한다(되살린 뒤 다시 띄울 때).
    pub async fn with(dir: TempDir, at: DateTime<Utc>, age: Option<AgeKeys>) -> Bench {
        register_slot(dir.path(), KeySlot::Dart, DART_KEY).await;
        register_slot(dir.path(), KeySlot::Ecos, ECOS_KEY).await;
        register_slot(dir.path(), KeySlot::BackupGithubToken, GITHUB_TOKEN).await;
        let data = DataServer::start().await;
        let push = PushServer::start().await;
        let github = FakeGithub::start().await;
        let age = age.unwrap_or_else(AgeKeys::generate);
        let clock = TestClock::at(at);
        let remote = Arc::new(AtomicBool::new(false));
        let launch = Launch {
            trading: trading_wiring(),
            team: Some(data.team()),
            secrets: None,
        };
        let reach = Reach {
            push: Some(push.transport()),
            backup: Some(BackupWiring {
                api_base: github.server.uri(),
                repo: REPO.to_string(),
                recipient: age.recipient.clone(),
            }),
        };
        let wiring = wiring(Arc::clone(&clock), None, Arc::clone(&remote));
        let start = App::launch_with(settings(dir.path(), WATCHED, true), wiring, launch, reach);
        let app = tokio::time::timeout(STEP, start)
            .await
            .expect("서버가 시간 안에 떠야 한다")
            .expect("서버가 떠야 한다");
        Bench {
            dir,
            clock,
            remote,
            app,
            data,
            push,
            github,
            age,
        }
    }

    /// 끈다. 같은 보관 폴더와 age 키를 돌려준다.
    pub async fn close(self) -> (TempDir, AgeKeys) {
        tokio::time::timeout(STEP, self.app.shutdown())
            .await
            .expect("시간 안에 꺼져야 한다")
            .expect("끄기가 성공해야 한다");
        (self.dir, self.age)
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

    pub fn crew(&self) -> Arc<Crew> {
        self.app.crew().expect("팀을 이어 띄웠다")
    }

    pub fn ai(&self) -> Arc<DemoAi> {
        self.crew().demo_ai().expect("시연 모드다")
    }

    pub fn script(&self, turn: DemoTurn, reply: &str) {
        self.ai().script(turn, reply);
    }

    pub fn addr(&self) -> SocketAddr {
        self.app.local_addr()
    }

    pub fn set(&self, at: DateTime<Utc>) {
        self.clock.set(at);
    }

    pub fn now(&self) -> DateTime<Utc> {
        self.clock.now()
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

    pub fn trade_state(&self, id: TradeId) -> TradeState {
        self.store().get_trade(id).unwrap().state
    }

    /// 매매하기 좋게 만든다: 스윙 템플릿에 `changes`를 맞추고 오늘 매매하기로 한다.
    pub async fn ready_with(&self, changes: Value) {
        swing_with(self.addr(), changes).await;
        let state = self.trading().decide_day(DayPlan::Trade).unwrap();
        assert_eq!(state, TeamState::Operating, "매매하기로 하면 운용 중이다");
    }

    /// 종목 비중 10%로 매매하기 좋게 만든다.
    pub async fn ready(&self) {
        self.ready_with(json!({"symbol_weight_pct": "10"})).await;
    }

    /// 지금 시세 [`ENTRY`]로 그 종목을 논의한다(사기 판단을 정해 둔다).
    pub async fn discuss_buy(&self, symbol: &str) -> price_inha::Discussed {
        self.script(DemoTurn::Verdict, &buy_verdict());
        self.quote(symbol, ENTRY, self.now()).await;
        self.crew()
            .discuss(symbol, "자기주식취득결정 공시")
            .await
            .expect("논의가 끝나야 한다")
    }

    /// 판단됨 → 사서 보유 → 손절로 팔아 팔림까지 만든 매매(에이전트를 거치지 않는다).
    pub async fn sold(&self, symbol: &str) -> TradeId {
        let at = self.now();
        self.quote(symbol, ENTRY, at).await;
        let trade = self.judged(symbol);
        let placed = self
            .place(order(trade, symbol, Purpose::NewEntry, ENTRY, at))
            .await;
        assert!(matches!(placed, Placed::Ordered(_)), "산다: {placed:?}");
        let later = at + chrono::Duration::seconds(1);
        self.quote(symbol, ENTRY, later).await;
        assert_eq!(self.trade_state(trade), TradeState::Holding);
        let exit = later + chrono::Duration::seconds(1);
        self.quote(symbol, STOP, exit).await;
        let placed = self
            .place(order(trade, symbol, Purpose::StopLoss, STOP, exit))
            .await;
        assert!(matches!(placed, Placed::Ordered(_)), "판다: {placed:?}");
        self.quote(symbol, STOP, exit + chrono::Duration::seconds(1))
            .await;
        assert_eq!(self.trade_state(trade), TradeState::Sold, "팔림이다");
        trade
    }

    /// 자동 주문 금액 한도를 넘는 사는 주문 하나를 승인 대기로 만든다(한도를 2%로 낮춘 뒤 부른다).
    pub async fn awaiting(&self) -> (TradeId, ApprovalNumber, Fingerprint) {
        self.quote(HYNIX, ENTRY, self.now()).await;
        let trade = self.judged(HYNIX);
        let at = self.now();
        let placed = self
            .place(order(trade, HYNIX, Purpose::NewEntry, ENTRY, at))
            .await;
        let Placed::AwaitingApproval {
            number,
            fingerprint,
        } = placed
        else {
            panic!("금액 한도를 넘으면 승인을 기다린다: {placed:?}");
        };
        (trade, number, fingerprint)
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

    async fn place(&self, request: OrderRequest) -> Placed {
        self.trading()
            .place(request)
            .await
            .expect("주문 안을 처리해야 한다")
    }
}

/// 지정가 · 수량 5주의 주문 안.
fn order(
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

/// `done`이 참이 될 때까지 짧게 쉬며 다시 본다.
pub async fn wait_for(what: &str, mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + STEP;
    while !done() {
        assert!(Instant::now() < deadline, "시간 안에 되지 않았다: {what}");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// 저장된 이벤트 가운데 그 종류의 내용들(번호 순서).
pub fn events(store: &Store, kind: &str) -> Vec<Value> {
    store
        .read_after(0, 100_000)
        .unwrap()
        .into_iter()
        .filter(|event| event.kind == kind)
        .map(|event| event.payload)
        .collect()
}

/// 저장된 마지막 이벤트 번호.
pub fn last_seq(store: &Store) -> u64 {
    store
        .read_after(0, 100_000)
        .unwrap()
        .last()
        .map_or(0, |event| event.seq)
}

// ───────────── HTTP ─────────────

pub fn origin_of(addr: SocketAddr) -> String {
    format!("http://127.0.0.1:{}", addr.port())
}

/// HTTP 응답 하나: 상태 · `Content-Type` · 본문 바이트 · JSON 본문(JSON이 아니면 `Null`).
pub struct Reply {
    pub status: u16,
    pub content_type: String,
    pub bytes: Vec<u8>,
    pub body: Value,
}

impl Reply {
    /// 본문 글자.
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.bytes).into_owned()
    }
}

pub async fn send(request: reqwest::RequestBuilder) -> Reply {
    let reply = tokio::time::timeout(STEP, request.send())
        .await
        .expect("응답이 시간 안에 와야 한다")
        .expect("요청이 닿아야 한다");
    let status = reply.status().as_u16();
    let content_type = reply
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_string();
    let bytes = reply.bytes().await.expect("본문을 읽어야 한다").to_vec();
    let body = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    Reply {
        status,
        content_type,
        bytes,
        body,
    }
}

/// 같은 출처에서 세션을 받아 `Cookie` 헤더 글자를 돌려준다.
pub async fn login(addr: SocketAddr) -> String {
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

/// 세션 없이 읽는다.
pub async fn get_anonymous(addr: SocketAddr, path: &str) -> Reply {
    send(reqwest::Client::new().get(format!("{}{path}", origin_of(addr)))).await
}

/// 세션을 받아 읽는다.
pub async fn get(addr: SocketAddr, path: &str) -> Reply {
    let cookie = login(addr).await;
    let request = reqwest::Client::new()
        .get(format!("{}{path}", origin_of(addr)))
        .header("Cookie", cookie);
    send(request).await
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
    send(request).await
}

/// 스윙 템플릿을 고르고 값을 맞춘다.
pub async fn swing_with(addr: SocketAddr, changes: Value) {
    let reply = post(addr, TEMPLATE_PATH, &json!({"style": "swing"})).await;
    assert_eq!(reply.status, 200, "템플릿을 골라야 한다: {}", reply.body);
    let id = reply.body["id"].as_i64().expect("템플릿 번호");
    let body = json!({"id": id, "changes": changes});
    let reply = post(addr, TEMPLATE_ADJUST_PATH, &body).await;
    assert_eq!(reply.status, 200, "상한 안의 값은 저장된다: {}", reply.body);
}

/// 폰 알림을 켠다: 주소 `https://push.invalid/push/<n>`로 구독한다.
pub async fn subscribe(addr: SocketAddr, n: u32) {
    let body = json!({
        "endpoint": format!("https://push.invalid/push/{n}"),
        "p256dh": P256DH, "auth": AUTH, "device": "시험 폰",
    });
    let reply = post(addr, PUSH_SUBSCRIBE_PATH, &body).await;
    assert_eq!(reply.status, 204, "구독을 받아야 한다: {}", reply.text());
}
