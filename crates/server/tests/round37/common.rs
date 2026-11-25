//! round37 시험(server 35차)이 함께 쓰는 도우미: 시계 · 달력 · 모의계좌 설정 · 자료 가짜 서버 · 서버 띄우기 ·
//! 같은 보관 폴더로 다시 띄우기 · 다른 기기 세션 · HTTP · 사는 주문 안 · 실전중 템플릿.
//! round36(승인 도우미) · round33(같은 홈으로 다시 띄우기 · 원격 세션) · round29(실전중 템플릿) ·
//! round27(실행 파일 시연 조립)의 공용 장치를 본떠 두고(그 폴더들은 고치지 않는다) 35차에 맞게 줄였다.
//!
//! - [`Bench`]: 임시 보관 폴더 · `127.0.0.1:0` · 시연 설정으로 [`price_inha::App::launch_with`]를 띄운다. 조립의
//!   금지 종목 · 업종표는 **빈 표**다(기본 조립과 같다) — 파일([`RULES_CONFIG_FILE`])이 있으면 그 값이 쓰인다.
//!   AI는 시연 모드의 가짜 공급자뿐이고, 자료(DART · ECOS)는 로컬 가짜 서버(wiremock)뿐이다. 토스 · Anthropic
//!   자리는 아무도 듣지 않는 로컬 주소다.
//! - [`Demo`]: 실행 파일과 같은 [`run_assembly`](`--demo`)로 띄운다. 시계 · 바깥 주소(모두 받은 요청을 세는
//!   로컬 가짜 서버) · CLI 찾는 자리(비움) · 가짜 AI 뜸(0)만 바꾼다.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use chrono::{DateTime, Datelike, NaiveDate, TimeZone, Utc, Weekday};
use pi_ai::AnthropicConfig;
use pi_approval::{Approvals, Ask, Subject, WaitTimes};
use pi_data::{CorpCode, DataConfig, NAVER_NEWS_KEYS, NaverNewsConfig};
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
    AccessKeyHash, AccountKind, Amount, Judgment, NewTemplate, NewTrade, RoleScore, SourceRef,
    Store, TeamState, TemplateId, TemplateState, TradeId, UnixMillis,
};
use pi_template::{Style, Template};
use price_inha::{
    APPROVAL_ANSWER_PATH, App, AppError, CalendarFn, Crew, DayPlan, Launch, OrderRequest, Placed,
    RULES_CONFIG_FILE, Reach, RunAssembly, Settings, TEMPLATE_ADJUST_PATH, TEMPLATE_PATH,
    TeamWiring, TradingWiring, Upstreams, Wiring, run_assembly, save_watchlist,
};
use rust_decimal::Decimal;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tempfile::TempDir;
use wiremock::matchers::{any, method, path, path_regex, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// 한 걸음을 기다리는 가장 긴 시간.
pub const STEP: Duration = Duration::from_secs(15);
/// 시연 논의 · 주문을 기다리는 가장 긴 시간(뒤에서 하루 판단 AI가 함께 돌아도 넉넉하게).
pub const LONG: Duration = Duration::from_secs(40);
/// 지켜볼 종목(국내).
pub const WATCHED: &str = "005930";
/// 매매하는 종목. 시험이 시세를 넣는다.
pub const HYNIX: &str = "000660";
const CORP_WATCHED: &str = "00126380";
const CORP_HYNIX: &str = "00164779";
/// 시험용 자료 키(진짜 키처럼 보이지 않는다).
const DART_KEY: &str = "test-dart-key-round37-zz";
const ECOS_KEY: &str = "test-ecos-key-round37-zz";
/// 다른 기기 세션의 접속 키(진짜 키처럼 보이지 않는다).
const ACCESS_KEY: &str = "zz-round37-access-key-qwmv";
/// 매매 근거의 진입가 · 손절값 · 목표값 · 주당 기대값(원).
const ENTRY: i64 = 100_000;
const STOP: i64 = 95_000;
const TARGET: i64 = 110_000;
const EDGE: i64 = 5_000;

/// 글자를 십진수로.
fn d(text: &str) -> Decimal {
    text.parse().unwrap()
}

// ───────────── 시계 · 달력 ─────────────

/// 테스트가 정하는 시계. 자는 것은 짧게만 잔다.
pub struct TestClock(AtomicI64);

impl TestClock {
    fn at(time: DateTime<Utc>) -> Arc<Self> {
        Arc::new(Self(AtomicI64::new(time.timestamp_millis())))
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

/// 2026-11-21 토요일(시연 달력은 쉬는 날에도 연다).
pub fn saturday() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 11, 21).unwrap()
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

// ───────────── 자료 가짜 서버 ─────────────

fn ok(body: Value) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(body)
}

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

/// 자료 가짜 서버(DART · ECOS 한 곳). 두 종목 모두 공시 하나와 재무 · 금리 · 환율이 있다.
struct DataServer {
    server: MockServer,
}

impl DataServer {
    async fn start() -> DataServer {
        let server = MockServer::start().await;
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

/// 아무도 듣지 않는 로컬 주소(실제 바깥 주소로 나가지 않게).
fn dead_base_url() -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    format!("http://127.0.0.1:{port}")
}

// ───────────── 설정 · 조립 ─────────────

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

/// 주문 길 조립: 모의계좌 · 국내 달력, 금지 종목 · 업종표는 **빈 표**(기본 조립과 같다).
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
        rules: RulesConfig::default(),
        calendar: Some(calendar),
    }
}

fn no_env(_name: &str) -> Option<String> {
    None
}

fn parse(list: &[&str]) -> Settings {
    let args: Vec<String> = list.iter().map(|s| (*s).to_string()).collect();
    Settings::parse(&args, &no_env, None).expect("설정을 읽어야 한다")
}

/// 설정: 그 보관 폴더 · 빈 포트 · 짧은 시세 간격 · 지켜볼 종목 · `--demo`.
fn demo_settings(home: &Path) -> Settings {
    let home = home.to_str().unwrap();
    parse(&[
        "--home",
        home,
        "--port",
        "0",
        "--interval-ms",
        "20",
        "--symbols",
        WATCHED,
        "--demo",
    ])
}

/// 다른 기기로 볼 상대 주소(문서용 주소 대역이 아닌 사설 대역).
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

/// 바깥 자리: 토스 · Anthropic은 아무도 듣지 않는 로컬 주소다. 상대 주소는 `remote`가 켜지면 다른 기기다.
fn wiring(clock: Arc<TestClock>, remote: &Arc<AtomicBool>) -> Wiring {
    let dead = dead_base_url();
    Wiring {
        upstreams: Upstreams {
            toss: TossConfig {
                base_url: dead.clone(),
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
        peer_override: Some(peer_override(Arc::clone(remote))),
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

/// 보관 폴더에 칸 하나의 키를 등록한다.
async fn register_slot(home: &Path, slot: KeySlot, value: &str) {
    let secrets = SecretStore::open(home).unwrap();
    let values = Credentials::new().with(slot.fields()[0], value);
    secrets
        .register_slot(slot, values, &AcceptAll)
        .await
        .unwrap();
}

/// 보관 폴더에 금지 종목 · 업종표 파일을 글자 그대로 쓴다.
pub fn write_config(home: &Path, raw: &str) {
    std::fs::write(home.join(RULES_CONFIG_FILE), raw).unwrap();
}

/// 금지 종목 · 업종표 파일 모양의 값(`sectors`는 종목 → 업종 객체).
pub fn config_file(banned: &[(&str, &str)], sectors: &[(&str, &str)]) -> String {
    let banned: Vec<Value> = banned
        .iter()
        .map(|(market, symbol)| json!({"market": market, "symbol": symbol}))
        .collect();
    let sectors: serde_json::Map<String, Value> = sectors
        .iter()
        .map(|(symbol, sector)| ((*symbol).to_string(), json!(sector)))
        .collect();
    json!({"banned": banned, "sectors": sectors}).to_string()
}

// ───────────── 띄우기(App::launch_with) ─────────────

/// 그 보관 폴더로 띄우는 데 쓰는 것들.
struct Parts {
    settings: Settings,
    wiring: Wiring,
    launch: Launch,
    reach: Reach,
}

fn parts(
    home: &Path,
    clock: &Arc<TestClock>,
    remote: &Arc<AtomicBool>,
    data: &DataServer,
) -> Parts {
    Parts {
        settings: demo_settings(home),
        wiring: wiring(Arc::clone(clock), remote),
        launch: Launch {
            trading: trading_wiring(),
            team: Some(data.team()),
            secrets: None,
        },
        reach: Reach {
            push: None,
            backup: None,
        },
    }
}

/// 그 보관 폴더로 띄워 본다(실패도 돌려준다).
async fn try_launch(
    home: &Path,
    clock: &Arc<TestClock>,
    remote: &Arc<AtomicBool>,
    data: &DataServer,
) -> Result<App, AppError> {
    let Parts {
        settings,
        wiring,
        launch,
        reach,
    } = parts(home, clock, remote, data);
    tokio::time::timeout(STEP, App::launch_with(settings, wiring, launch, reach))
        .await
        .expect("시간 안에 끝나야 한다")
}

/// 새 보관 폴더(자료 키 등록).
async fn fresh_home() -> TempDir {
    let dir = tempfile::tempdir().unwrap();
    register_slot(dir.path(), KeySlot::Dart, DART_KEY).await;
    register_slot(dir.path(), KeySlot::Ecos, ECOS_KEY).await;
    dir
}

/// 금지 종목 · 업종표 파일을 글자 그대로 둔 새 보관 폴더로 띄워 본 결과의 오류(떠 버리면 실패).
pub async fn launch_error(raw: &str) -> AppError {
    let dir = fresh_home().await;
    write_config(dir.path(), raw);
    let clock = TestClock::at(kst(monday(), 10, 0));
    let remote = Arc::new(AtomicBool::new(false));
    let data = DataServer::start().await;
    match try_launch(dir.path(), &clock, &remote, &data).await {
        Ok(app) => {
            let _ = app.shutdown().await;
            panic!("깨진 파일로는 켜지면 안 된다");
        }
        Err(error) => error,
    }
}

/// 떠 있는 서버 하나와 그 보관 폴더 · 시계 · 상대 주소 · 가짜 자료 서버.
pub struct Bench {
    dir: TempDir,
    clock: Arc<TestClock>,
    remote: Arc<AtomicBool>,
    app: App,
    data: DataServer,
}

impl Bench {
    /// 새 보관 폴더(금지 종목 · 업종표 파일 없음)로 그 시각에 띄운다.
    pub async fn start(at: DateTime<Utc>) -> Bench {
        Bench::launch(fresh_home().await, at).await
    }

    /// 금지 종목 · 업종표 파일을 글자 그대로 둔 새 보관 폴더로 그 시각에 띄운다.
    pub async fn with_file(at: DateTime<Utc>, raw: &str) -> Bench {
        let dir = fresh_home().await;
        write_config(dir.path(), raw);
        Bench::launch(dir, at).await
    }

    async fn launch(dir: TempDir, at: DateTime<Utc>) -> Bench {
        let clock = TestClock::at(at);
        let remote = Arc::new(AtomicBool::new(false));
        let data = DataServer::start().await;
        let app = try_launch(dir.path(), &clock, &remote, &data)
            .await
            .expect("서버가 떠야 한다");
        Bench {
            dir,
            clock,
            remote,
            app,
            data,
        }
    }

    /// 끄고, 같은 보관 폴더로 같은 시각에 다시 띄운다(상대 주소는 그 컴퓨터로 돌린다).
    pub async fn restart(self) -> Bench {
        let Bench {
            dir,
            clock,
            app,
            data,
            ..
        } = self;
        tokio::time::timeout(STEP, app.shutdown())
            .await
            .expect("시간 안에 꺼져야 한다")
            .expect("끄기가 성공해야 한다");
        let remote = Arc::new(AtomicBool::new(false));
        let app = try_launch(dir.path(), &clock, &remote, &data)
            .await
            .expect("다시 떠야 한다");
        Bench {
            dir,
            clock,
            remote,
            app,
            data,
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

    fn now(&self) -> DateTime<Utc> {
        self.clock.now()
    }

    /// 다른 기기의 원격 세션을 받는다: 접속 키를 두고, 다른 기기로 보이게 한 뒤 그 키로 들어간다.
    /// `Cookie` 헤더 글자를 돌려준다. 돌려준 뒤에도 다른 기기로 보인다.
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

    /// 지금 시각에 받은 시세 하나를 넣고, 판단됨인 매매를 만들어 그 종목의 사는 주문 안을 낸다
    /// ([`Bench::ready`] 뒤에). 매매 번호와 주문 길의 결과.
    pub async fn buy(&self, symbol: &str) -> (TradeId, Placed) {
        let at = self.now();
        let quote = Quote {
            symbol: Symbol::new(symbol),
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
        let trade = self.judged(symbol);
        let placed = self
            .app
            .trading()
            .place(order(trade, symbol, at))
            .await
            .expect("주문 안을 처리해야 한다");
        (trade, placed)
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

    /// 승인 모듈의 눈으로 같은 저장소를 본다.
    fn approvals(&self) -> Approvals {
        let clock: Arc<dyn pi_approval::Clock> = Arc::new(ApprovalClock(Arc::clone(&self.clock)));
        Approvals::new(self.store(), clock, WaitTimes::default())
    }

    /// 실전중인 스윙 템플릿: 승격대기를 만들고 그 실전 승격 승인 요청을 지금 승인 길로 승인한다.
    pub async fn live_template(&self) -> TemplateId {
        let store = self.store();
        let value: Value = serde_json::from_str(&Template::new(Style::Swing).to_json()).unwrap();
        let at = ms(self.now());
        let saved = store.create_template(NewTemplate { value, at }).unwrap();
        let pending = store
            .submit_template_for_promotion(saved.id, &json!({ "passed": true }), at)
            .unwrap();
        assert_eq!(pending.state, TemplateState::PromotionPending);
        let requested = self
            .approvals()
            .request(Ask {
                subject: Subject::LivePromotion {
                    template: pending.id,
                    revision: pending.revision,
                },
                reason: "시험: 승격 기준을 모두 넘었다".to_string(),
            })
            .expect("실전 승격 승인 요청");
        let body = json!({
            "number": requested.number.as_str(),
            "fingerprint": requested.fingerprint.as_str(),
            "answer": "approve",
        });
        let reply = post(self.addr(), APPROVAL_ANSWER_PATH, &body).await;
        assert_eq!(reply.body["result"], json!("approved"), "{}", reply.text);
        assert_eq!(
            store.get_template(pending.id).unwrap().state,
            TemplateState::Live,
            "실전중이다"
        );
        pending.id
    }
}

/// 승인 모듈이 읽는 시계: 시험 시계 그대로.
struct ApprovalClock(Arc<TestClock>);

impl pi_approval::Clock for ApprovalClock {
    fn now(&self) -> UnixMillis {
        UnixMillis(self.0.now().timestamp_millis())
    }
}

/// 지정가 · 수량 5주의 사는 주문 안(VI · 거래정지 아님).
fn order(trade: TradeId, symbol: &str, at: DateTime<Utc>) -> OrderRequest {
    let draft = OrderDraft {
        market: Market::Kr,
        symbol: Symbol::new(symbol),
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

/// 막혔으면 그 까닭 글, 아니면 없음.
pub fn blocked_detail(placed: &Placed) -> Option<String> {
    match placed {
        Placed::Blocked(blocked) => Some(blocked.detail()),
        _ => None,
    }
}

/// 주문 전 점검을 지났는가(주문됨 또는 승인 기다림).
pub fn passed(placed: &Placed) -> bool {
    matches!(placed, Placed::Ordered(_) | Placed::AwaitingApproval { .. })
}

// ───────────── 시연 조립(run_assembly --demo) ─────────────

/// 무엇을 물어도 503으로 답하고 받은 요청을 세는 로컬 가짜 서버.
async fn counting_server() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(any())
        .respond_with(ResponseTemplate::new(503))
        .mount(&server)
        .await;
    server
}

/// 증권사(토스) · AI(Anthropic) · 자료(DART · ECOS · FRED) · 네이버 뉴스 주소 자리의 가짜 서버들.
pub struct Outside {
    broker: MockServer,
    ai: MockServer,
    data: MockServer,
    news: MockServer,
}

impl Outside {
    async fn start() -> Outside {
        Outside {
            broker: counting_server().await,
            ai: counting_server().await,
            data: counting_server().await,
            news: counting_server().await,
        }
    }

    async fn received(server: &MockServer) -> usize {
        server
            .received_requests()
            .await
            .map_or(0, |list| list.len())
    }

    /// (이름, 받은 요청 수) 넷.
    pub async fn counts(&self) -> Vec<(&'static str, usize)> {
        vec![
            ("증권사", Outside::received(&self.broker).await),
            ("AI", Outside::received(&self.ai).await),
            ("자료", Outside::received(&self.data).await),
            ("네이버", Outside::received(&self.news).await),
        ]
    }
}

/// 띄우기 전 보관 폴더에 모든 키를 등록하고 지켜볼 종목을 저장한다. 값은 진짜 키처럼 보이지 않는다.
async fn prepare_demo(home: &Path) {
    let secrets = SecretStore::open(home).unwrap();
    let broker = Credentials::new()
        .with("client_id", "test-client-id-round37")
        .with("client_secret", "test-client-secret-round37");
    secrets
        .register(KeyKind::Broker, broker, &AcceptAll)
        .await
        .unwrap();
    let ai = Credentials::new().with("api_key", "test-ai-key-round37-zz");
    secrets
        .register(KeyKind::AiApiKey, ai, &AcceptAll)
        .await
        .unwrap();
    let slots = [
        (KeySlot::Dart, DART_KEY),
        (KeySlot::Ecos, ECOS_KEY),
        (KeySlot::Fred, "test-fred-key-round37-zz"),
    ];
    for (slot, value) in slots {
        let values = Credentials::new().with(slot.fields()[0], value);
        secrets
            .register_slot(slot, values, &AcceptAll)
            .await
            .unwrap();
    }
    let naver = Credentials::new()
        .with("client_id", "test-naver-id-round37")
        .with("client_secret", "test-naver-secret-round37");
    secrets
        .register_provider(NAVER_NEWS_KEYS.provider_keys(), naver, &AcceptAll)
        .await
        .unwrap();
    save_watchlist(home, &[Symbol::new(WATCHED)]).unwrap();
}

/// 실행 파일 설정: 그 보관 폴더 · 빈 포트 · 짧은 시세 간격과 다시 시도 간격(`demo`면 `--demo`).
/// 종목은 저장된 목록에서 읽는다.
pub fn run_settings(home: &Path, demo: bool) -> Settings {
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
    parse(&list)
}

/// 지켜볼 종목만 저장한 새 보관 폴더(키 없음 — 조립 값만 볼 때).
pub fn watched_home() -> TempDir {
    let dir = tempfile::tempdir().unwrap();
    save_watchlist(dir.path(), &[Symbol::new(WATCHED)]).unwrap();
    dir
}

/// 실행 파일 조립에서 시계 · 바깥 주소 · CLI 찾는 자리 · 가짜 AI 뜸(0)만 바꾼다.
fn bend(mut assembly: RunAssembly, clock: &Arc<TestClock>, outside: &Outside) -> RunAssembly {
    assembly.wiring.clock = Arc::clone(clock) as Arc<dyn Clock>;
    assembly.wiring.cli_locations = Vec::new();
    assembly.wiring.upstreams = Upstreams {
        toss: TossConfig {
            base_url: outside.broker.uri(),
            timeout: Duration::from_secs(3),
            token_failure_cooldown: Duration::ZERO,
            ..TossConfig::default()
        },
        anthropic: AnthropicConfig {
            base_url: outside.ai.uri(),
            timeout: Duration::from_secs(3),
            max_retries: 0,
            retry_base_delay: Duration::from_millis(1),
        },
    };
    if let Some(team) = assembly.launch.team.as_mut() {
        let data = outside.data.uri();
        team.data = DataConfig::new(data.clone(), data.clone(), data);
        team.data.timeout = Duration::from_secs(3);
        team.news = NaverNewsConfig::new(outside.news.uri()).with_timeout(Duration::from_secs(3));
    }
    assembly.demo_ai_pause = Duration::ZERO;
    assembly
}

/// 실행 파일의 `--demo` 조립으로 띄운 서버 하나와 그 보관 폴더 · 시계 · 가짜 서버들.
pub struct Demo {
    dir: TempDir,
    pub outside: Outside,
    app: App,
}

impl Demo {
    /// 모든 키와 지켜볼 종목을 둔 새 보관 폴더로 그 시각에 띄운다.
    pub async fn start(at: DateTime<Utc>) -> Demo {
        let dir = tempfile::tempdir().unwrap();
        prepare_demo(dir.path()).await;
        let clock = TestClock::at(at);
        let outside = Outside::start().await;
        let settings = run_settings(dir.path(), true);
        let assembly = bend(run_assembly(&settings, None), &clock, &outside);
        let app = tokio::time::timeout(STEP, App::launch_run(settings, assembly))
            .await
            .expect("서버가 시간 안에 떠야 한다")
            .expect("서버가 떠야 한다");
        Demo { dir, outside, app }
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

    pub fn crew(&self) -> Arc<Crew> {
        self.app.crew().expect("실행 파일 조립은 팀을 잇는다")
    }

    /// 매매하기 좋게 만든다: 스윙 템플릿을 고르고 종목 비중 10%로 맞춘 뒤 오늘 매매하기로 한다
    /// (논의는 고른 템플릿이 있어야 끝까지 돈다).
    pub async fn ready(&self) {
        let addr = self.app.local_addr();
        let reply = post(addr, TEMPLATE_PATH, &json!({"style": "swing"})).await;
        assert_eq!(reply.status, 200, "템플릿을 골라야 한다: {}", reply.body);
        let id = reply.body["id"].as_i64().expect("템플릿 번호");
        let body = json!({"id": id, "changes": {"symbol_weight_pct": "10"}});
        let reply = post(addr, TEMPLATE_ADJUST_PATH, &body).await;
        assert_eq!(reply.status, 200, "상한 안의 값은 저장된다: {}", reply.body);
        self.app
            .trading()
            .decide_day(DayPlan::Trade)
            .expect("매매하기로 정할 수 있다");
    }
}

// ───────────── 기다리기 · 이벤트 ─────────────

/// 조건이 될 때까지 기다린다(가장 길게 `limit`).
pub async fn wait_until(what: &str, limit: Duration, mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + limit;
    while !done() {
        assert!(Instant::now() < deadline, "시간 안에 되지 않았다: {what}");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// 조건이 될 때까지 기다린다(가장 길게 [`STEP`]).
pub async fn wait_for(what: &str, done: impl FnMut() -> bool) {
    wait_until(what, STEP, done).await;
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

/// 그 세션으로 읽는다. `cookie`가 없으면 세션 없이 읽는다.
pub async fn get_with(addr: SocketAddr, path: &str, cookie: Option<&str>) -> Reply {
    let mut request = reqwest::Client::new().get(format!("{}{path}", origin_of(addr)));
    if let Some(cookie) = cookie {
        request = request.header("Cookie", cookie);
    }
    send(request).await
}

/// 같은 출처(`Origin`)에서 JSON 글자를 `POST`한다. `cookie`가 없으면 세션 없이 보낸다.
pub async fn post_raw(addr: SocketAddr, path: &str, cookie: Option<&str>, body: &str) -> Reply {
    let origin = origin_of(addr);
    let mut request = reqwest::Client::new()
        .post(format!("{origin}{path}"))
        .header("Origin", &origin)
        .header("Content-Type", "application/json")
        .body(body.to_string());
    if let Some(cookie) = cookie {
        request = request.header("Cookie", cookie);
    }
    send(request).await
}

/// 세션을 받아 읽는다.
pub async fn get(addr: SocketAddr, path: &str) -> Reply {
    let cookie = login(addr).await;
    get_with(addr, path, Some(&cookie)).await
}

/// 세션을 받아 같은 출처에서 보낸다.
pub async fn post(addr: SocketAddr, path: &str, body: &Value) -> Reply {
    let cookie = login(addr).await;
    post_raw(addr, path, Some(&cookie), &body.to_string()).await
}
