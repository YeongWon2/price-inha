//! 28차 시험의 공용 도우미: 시계 · 달력 · 모의계좌 설정 · 자료 가짜 서버 · 서버 띄우기 · HTTP · 매매 · 개선안.
//! round10의 도우미(가짜 AI로 복기해 개선안을 만든다)를 본떴다(그 폴더는 고치지 않는다).
//!
//! 서버는 임시 보관 폴더 · `127.0.0.1:0` · 시연 모드로 띄우고 투자 팀과 폰 · 사무실 경로까지 잇는다
//! ([`price_inha::App::launch_with`]). 상대 주소를 다른 기기로 바꿔 볼 수 있다(세션 없는 원격은 401).
//! AI는 시연 모드의 가짜 공급자([`price_inha::DemoAi`])뿐이고, 자료(DART · ECOS)는 로컬 가짜 서버(wiremock)뿐이다.
//! 토스 · Anthropic 자리는 아무도 듣지 않는 로컬 주소다.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, Datelike, NaiveDate, TimeZone, Utc, Weekday};
use pi_ai::AnthropicConfig;
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
use pi_secrets::{Credentials, KeyKind, KeySlot, SecretStore, Verifier, VerifyFailure};
use pi_store::{
    AccessKeyHash, AccountKind, Amount, Judgment, NewTrade, ProposalId, ProposalState, RoleScore,
    SourceRef, Store, TeamState, TradeId, TradeState, UnixMillis,
};
use price_inha::{
    APPROVAL_DECIDE_PATH, App, CalendarFn, Crew, DayPlan, DemoAi, DemoTurn, Launch, OrderRequest,
    PROPOSAL_ANSWER_PATH, PROPOSALS_PATH, Placed, RULES_VIEW_PATH, Reach, Settings,
    TEMPLATE_ADJUST_PATH, TEMPLATE_PATH, TeamWiring, Trading, TradingWiring, Upstreams, Wiring,
};
use rust_decimal::Decimal;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tempfile::TempDir;
use wiremock::matchers::{method, path, path_regex, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// 한 걸음을 기다리는 가장 긴 시간.
pub const STEP: Duration = Duration::from_secs(15);
/// 지켜볼 종목(시연 시세가 흐른다). 새 공시 보기(scan)의 대상이다.
pub const WATCHED: &str = "005930";
/// 논의 · 매매하는 종목(SK하이닉스). 지켜보지 않아 시연 시세가 흐르지 않고 테스트가 시세를 넣는다.
pub const HYNIX: &str = "000660";
/// 둘째로 사고파는 종목(NAVER).
pub const NAVER: &str = "035420";
/// 고유번호.
const CORP_WATCHED: &str = "00126380";
const CORP_HYNIX: &str = "00164779";
/// 가짜 공시의 접수번호와 보고서명.
pub const DISCLOSURE_NO: &str = "20261116000123";
pub const DISCLOSURE_NAME: &str = "주요사항보고서(자기주식취득결정)";
/// 시험용 자료 키(진짜 키처럼 보이지 않는다).
pub const DART_KEY: &str = "test-dart-key-round10-zz";
const ECOS_KEY: &str = "test-ecos-key-round10-zz";
/// 매매 근거의 진입가 · 손절값 · 목표값 · 주당 기대값(원).
pub const ENTRY: i64 = 100_000;
const STOP: i64 = 95_000;
const TARGET: i64 = 110_000;
const EDGE: i64 = 5_000;

/// 글자를 십진수로.
pub fn d(text: &str) -> Decimal {
    text.parse().unwrap()
}

// ───────────── 시계 · 달력 ─────────────

/// 테스트가 정하는 시계. 기다리기는 아주 짧게만 쉰다(뒤에서 도는 고리가 바쁘게 돌지 않게).
pub struct TestClock(AtomicI64);

impl TestClock {
    fn at(time: DateTime<Utc>) -> Arc<Self> {
        Arc::new(Self(AtomicI64::new(time.timestamp_millis())))
    }

    fn set(&self, time: DateTime<Utc>) {
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

/// 평일마다 정규장 09:00~15:30(한국 시각) 하나.
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
            "report_nm": DISCLOSURE_NAME, "rcept_no": DISCLOSURE_NO, "flr_nm": "시험회사",
            "rcept_dt": "20261116", "rm": "유"
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
    /// 자료가 모두 있는 가짜 서버.
    pub async fn start() -> DataServer {
        let server = MockServer::start().await;
        let ok = |body: Value| ResponseTemplate::new(200).set_body_json(body);
        for (corp, stock) in [(CORP_WATCHED, WATCHED), (CORP_HYNIX, HYNIX)] {
            let list = disclosure_body(corp, stock);
            Mock::given(method("GET"))
                .and(path("/api/list.json"))
                .and(query_param("corp_code", corp))
                .respond_with(ok(list))
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
    let sectors = HashMap::from([
        (Symbol::new(HYNIX), "반도체".to_string()),
        (Symbol::new(NAVER), "인터넷".to_string()),
    ]);
    let calendar: CalendarFn = Arc::new(kr_calendar);
    TradingWiring {
        paper,
        rules: RulesConfig {
            banned_symbols: Vec::new(),
            sectors,
        },
        calendar: Some(calendar),
    }
}

fn no_env(_name: &str) -> Option<String> {
    None
}

fn settings(home: &Path) -> Settings {
    let home = home.to_str().unwrap();
    let list = [
        "--home",
        home,
        "--port",
        "0",
        "--interval-ms",
        "20",
        "--symbols",
        WATCHED,
        "--demo",
    ];
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

/// 다른 기기의 접속 키(시험용, 글자 수 · 글자 모음은 `pi_gate`의 것).
const ACCESS_KEY: &str = "0123456789ABCDEFGHJK";

/// 다른 기기로 볼 상대 주소.
const REMOTE_PEER: [u8; 4] = [100, 64, 0, 7];

fn wiring(clock: Arc<TestClock>, remote: Arc<AtomicBool>) -> Wiring {
    let peer: pi_gate::PeerOverride = Arc::new(move |addr: SocketAddr| {
        if remote.load(Ordering::SeqCst) {
            SocketAddr::from((REMOTE_PEER, addr.port()))
        } else {
            addr
        }
    });
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
        peer_override: Some(peer),
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

/// 보관 폴더에 자료 키(DART · ECOS)를 등록해 둔다.
async fn register_data_keys(home: &Path) {
    let secrets = SecretStore::open(home).unwrap();
    for (slot, key) in [(KeySlot::Dart, DART_KEY), (KeySlot::Ecos, ECOS_KEY)] {
        let values = Credentials::new().with(slot.fields()[0], key);
        secrets
            .register_slot(slot, values, &AcceptAll)
            .await
            .unwrap();
    }
}

/// 떠 있는 서버 하나와 그 보관 폴더 · 시계 · 자료 가짜 서버.
pub struct Bench {
    dir: TempDir,
    clock: Arc<TestClock>,
    remote: Arc<AtomicBool>,
    app: App,
    /// 자료 가짜 서버(떠 있는 동안 살려 둔다).
    _data: DataServer,
}

impl Bench {
    /// 새 보관 폴더로 그 시각에 띄운다(자료는 모두 있다).
    pub async fn start(at: DateTime<Utc>) -> Bench {
        let dir = tempfile::tempdir().unwrap();
        register_data_keys(dir.path()).await;
        let data = DataServer::start().await;
        let clock = TestClock::at(at);
        let remote = Arc::new(AtomicBool::new(false));
        let launch = Launch {
            trading: trading_wiring(),
            team: Some(data.team()),
            secrets: None,
        };
        let reach = Reach {
            push: None,
            backup: None,
        };
        let start = App::launch_with(
            settings(dir.path()),
            wiring(Arc::clone(&clock), Arc::clone(&remote)),
            launch,
            reach,
        );
        let app = tokio::time::timeout(STEP, start)
            .await
            .expect("서버가 시간 안에 떠야 한다")
            .expect("서버가 떠야 한다");
        Bench {
            dir,
            clock,
            remote,
            app,
            _data: data,
        }
    }

    /// 끈다. 같은 보관 폴더로 다시 띄울 수 있게 폴더를 돌려준다.
    pub async fn close(self) -> TempDir {
        tokio::time::timeout(STEP, self.app.shutdown())
            .await
            .expect("시간 안에 꺼져야 한다")
            .expect("끄기가 성공해야 한다");
        self.dir
    }

    pub fn store(&self) -> Store {
        self.app.store()
    }

    /// 다른 기기로 보이게 한다(세션 없이).
    pub fn go_remote(&self) {
        self.remote.store(true, Ordering::SeqCst);
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
        self.go_remote();
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

    pub fn trading(&self) -> Arc<Trading> {
        self.app.trading()
    }

    pub fn crew(&self) -> Arc<Crew> {
        self.app.crew().expect("팀을 이어 띄웠다")
    }

    /// 시연 모드의 가짜 AI.
    pub fn ai(&self) -> Arc<DemoAi> {
        self.crew().demo_ai().expect("시연 모드다")
    }

    /// 가짜 AI의 그 턴 답을 정한다.
    pub fn script(&self, turn: DemoTurn, reply: &str) {
        self.ai().script(turn, reply);
    }

    pub fn addr(&self) -> SocketAddr {
        self.app.local_addr()
    }

    /// 시계를 그 시각으로.
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

    /// 매매하기 좋게 만든다: 스윙 템플릿(종목 비중 10%)을 고르고 오늘 매매하기로 한다.
    pub async fn ready(&self) -> i64 {
        let id = swing_with(self.addr(), json!({"symbol_weight_pct": "10"})).await;
        let state = self.trading().decide_day(DayPlan::Trade).unwrap();
        assert_eq!(state, TeamState::Operating, "매매하기로 하면 운용 중이다");
        id
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

    /// 판단됨인 매매 하나(근거 6칸이 다 참). 판단 시각은 지금이다.
    fn judged(&self, symbol: &str) -> TradeId {
        let store = self.store();
        let ms = UnixMillis(self.now().timestamp_millis());
        let trade = store
            .create_trade(NewTrade {
                symbol: symbol.to_string(),
                account: AccountKind::Paper,
                at: ms,
            })
            .unwrap();
        let judgment = Judgment {
            decided_at: ms,
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
                known_at: ms,
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

// ───────────── HTTP ─────────────

fn origin_of(addr: SocketAddr) -> String {
    format!("http://127.0.0.1:{}", addr.port())
}

/// HTTP 응답 하나: 상태와 JSON 본문(JSON이 아니면 `Null`).
pub struct Reply {
    pub status: u16,
    pub body: Value,
}

async fn send(request: reqwest::RequestBuilder) -> Reply {
    let reply = tokio::time::timeout(STEP, request.send())
        .await
        .expect("응답이 시간 안에 와야 한다")
        .expect("요청이 닿아야 한다");
    let status = reply.status().as_u16();
    let text = reply.text().await.expect("본문을 읽어야 한다");
    let body = serde_json::from_str(&text).unwrap_or(Value::Null);
    Reply { status, body }
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
    cookie_of(&reply)
}

/// 그 세션(`Cookie` 헤더 글자)으로 읽는다.
pub async fn get_with(addr: SocketAddr, path: &str, cookie: &str) -> Reply {
    let request = reqwest::Client::new()
        .get(format!("{}{path}", origin_of(addr)))
        .header("Cookie", cookie);
    send(request).await
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

/// 스윙 템플릿을 고르고 값을 맞춘다. 템플릿 번호를 돌려준다.
pub async fn swing_with(addr: SocketAddr, changes: Value) -> i64 {
    let reply = post(addr, TEMPLATE_PATH, &json!({"style": "swing"})).await;
    assert_eq!(reply.status, 200, "템플릿을 골라야 한다: {}", reply.body);
    let id = reply.body["id"].as_i64().expect("템플릿 번호");
    let body = json!({"id": id, "changes": changes});
    let reply = post(addr, TEMPLATE_ADJUST_PATH, &body).await;
    assert_eq!(reply.status, 200, "상한 안의 값은 저장된다: {}", reply.body);
    id
}

// ───────────── 개선안 ─────────────

/// 복기 담당의 답: 손절이 늦음 + 바꿀 규칙.
fn late_stop(change: Value) -> String {
    json!({"mistakes": ["late_stop"], "note": "손절이 늦었다", "change": change}).to_string()
}

/// 지금 쓰는 템플릿의 값.
pub fn params_of(store: &Store, template: i64) -> pi_template::RiskParams {
    let saved = store.get_template(pi_store::TemplateId(template)).unwrap();
    pi_template::Template::from_json(&saved.value.to_string())
        .unwrap()
        .params
}

/// 지금 쓰는 템플릿의 한 매매 위험(%).
pub fn risk_pct(store: &Store, template: i64) -> Decimal {
    params_of(store, template).risk_per_trade_pct
}

/// 스윙 템플릿을 고르고 팔린 매매 둘(같은 실수)을 가짜 AI로 복기해 개선안 하나를 낸다.
/// (템플릿 번호, 개선안 번호)를 돌려준다.
pub async fn proposed(bench: &Bench, change: Value) -> (i64, ProposalId) {
    let template = bench.ready().await;
    bench.sold(HYNIX).await;
    bench.sold(NAVER).await;
    bench.script(DemoTurn::Review, &late_stop(change));
    let reviewed = bench.crew().review_day().await.expect("복기한다");
    assert_eq!(reviewed.proposals.len(), 1, "같은 실수 2건이면 개선안 하나");
    (template, reviewed.proposals[0])
}

/// 개선안 목록(`GET /api/proposals`)에서 그 개선안.
pub async fn listed(bench: &Bench, id: ProposalId) -> Value {
    let reply = get(bench.addr(), PROPOSALS_PATH).await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    reply.body["proposals"]
        .as_array()
        .and_then(|list| list.iter().find(|p| p["id"] == json!(id.0)).cloned())
        .unwrap_or_else(|| panic!("목록에 있다: {}", reply.body))
}

/// 개선안 목록에서 그 개선안의 승인 번호와 지문.
pub async fn approval_of(bench: &Bench, id: ProposalId) -> (String, String) {
    let found = listed(bench, id).await;
    let number = found["number"].as_str().expect("승인 번호").to_string();
    let fingerprint = found["fingerprint"].as_str().expect("지문").to_string();
    (number, fingerprint)
}

/// 규칙 개선안 답하기 경로(`POST /api/proposals/answer`)로 답한다.
pub async fn answer_proposal(
    bench: &Bench,
    id: ProposalId,
    number: &str,
    fingerprint: &str,
    word: &str,
) -> Value {
    let body = json!({"id": id.0, "number": number, "fingerprint": fingerprint, "answer": word});
    let reply = post(bench.addr(), PROPOSAL_ANSWER_PATH, &body).await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    reply.body
}

/// 폰 · 사무실의 승인 카드 길(`POST /api/approvals/{번호}/approve` · `/reject`)로 답한다.
pub async fn card(bench: &Bench, number: &str, verb: &str, fingerprint: &str) -> Reply {
    let path = APPROVAL_DECIDE_PATH
        .replace("{id}", number)
        .replace("{verb}", verb);
    post(bench.addr(), &path, &json!({ "fingerprint": fingerprint })).await
}

/// 개선안의 저장소 상태.
pub fn proposal_state(bench: &Bench, id: ProposalId) -> ProposalState {
    bench.store().get_proposal(id).unwrap().state
}

/// 규칙 읽기(`GET /api/rules`).
pub async fn rules(bench: &Bench) -> Value {
    let reply = get(bench.addr(), RULES_VIEW_PATH).await;
    assert_eq!(reply.status, 200, "규칙을 읽는다: {}", reply.body);
    reply.body
}

/// 규칙 읽기의 그 칸.
pub fn field(view: &Value, key: &str) -> Value {
    view["fields"]
        .as_array()
        .and_then(|list| list.iter().find(|f| f["key"] == json!(key)).cloned())
        .unwrap_or_else(|| panic!("{key} 칸이 있다: {view}"))
}

/// 십진수 글자(또는 수)를 십진수로.
pub fn num(value: &Value) -> Decimal {
    match value {
        Value::String(text) => text.parse().unwrap_or_else(|_| panic!("수 글자: {value}")),
        Value::Number(n) => n
            .to_string()
            .parse()
            .unwrap_or_else(|_| panic!("수: {value}")),
        other => panic!("수가 아니다: {other}"),
    }
}

/// 바뀐 칸 목록(`[{key, before, after, widened}]`)이 그 한 칸뿐이고 앞 · 뒤 · 넓힘이 맞는지 본다.
pub fn only_change(changes: &Value, key: &str, before: &str, after: &str, widened: bool) {
    let list = changes
        .as_array()
        .unwrap_or_else(|| panic!("바뀐 칸 목록: {changes}"));
    assert_eq!(list.len(), 1, "바뀐 칸은 하나: {changes}");
    assert_eq!(list[0]["key"], json!(key), "{changes}");
    assert_eq!(num(&list[0]["before"]), d(before), "앞 값: {changes}");
    assert_eq!(num(&list[0]["after"]), d(after), "뒤 값: {changes}");
    assert_eq!(list[0]["widened"], json!(widened), "넓힘: {changes}");
}

/// 다음 거래일(화요일) 장 시작 뒤의 하루 판단: 냉각중 개선안이 반영된다.
pub async fn next_trading_day(bench: &Bench) {
    bench.set(kst(tuesday(), 9, 1));
    bench
        .crew()
        .plan_day()
        .await
        .expect("다음 거래일의 하루 판단");
}

/// 그 다음 거래일(화요일).
pub fn tuesday() -> NaiveDate {
    monday().succ_opt().unwrap()
}
