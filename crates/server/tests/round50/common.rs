//! 50차(근거 카드 · 승인 카드 보강) 시험의 공용 발판: 시계 · 달력 · 모의계좌 설정 · 자료 가짜 서버 · 서버 띄우기 ·
//! HTTP · 판단된 매매(담당 의견 · 팀장 요약이 있는 것과 없는 옛 모양) · 금액 한도를 넘는 주문의 승인 요청.
//! round31(히스토리) · round19(사무실 승인 카드)의 도우미를 본떴다(그 폴더는 고치지 않는다).
//!
//! 서버는 임시 보관 폴더 · `127.0.0.1:0` · 시연 모드로 띄우고 투자 팀과 폰 · 사무실 경로까지 잇는다
//! ([`price_inha::App::launch_with`]). AI는 시연 모드의 가짜 공급자뿐이고, 자료(DART · ECOS)는 로컬 가짜
//! 서버(wiremock)뿐이다. 토스 · Anthropic 자리는 아무도 듣지 않는 로컬 주소다. 주문은 모의계좌로만 간다.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, Datelike, NaiveDate, TimeZone, Utc, Weekday};
use pi_ai::AnthropicConfig;
use pi_approval::{ApprovalNumber, Approvals, Fingerprint, WaitTimes};
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
    AccountKind, Amount, Conviction, Judgment, NewTrade, RoleOpinion, RoleScore, SourceRef, Stance,
    Store, TeamState, TradeId, TradeSide, TradeState, TradeSummary, UnixMillis,
};
use price_inha::{
    App, CalendarFn, DayPlan, Launch, OrderRequest, Placed, Reach, Settings, TEMPLATE_ADJUST_PATH,
    TEMPLATE_PATH, TeamWiring, Trading, TradingWiring, Upstreams, Wiring,
};
use rust_decimal::Decimal;
use serde_json::{Value, json};
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

fn wiring(clock: Arc<TestClock>) -> Wiring {
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
    _dir: TempDir,
    clock: Arc<TestClock>,
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
            wiring(Arc::clone(&clock)),
            launch,
            reach,
        );
        let app = tokio::time::timeout(STEP, start)
            .await
            .expect("서버가 시간 안에 떠야 한다")
            .expect("서버가 떠야 한다");
        Bench {
            _dir: dir,
            clock,
            app,
            _data: data,
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

    /// 승인 모듈의 눈으로 같은 저장소를 본다(시각은 테스트 시계).
    pub fn approvals(&self) -> Approvals {
        let clock: Arc<dyn pi_approval::Clock> = Arc::new(ApprovalClock(Arc::clone(&self.clock)));
        Approvals::new(self.store(), clock, WaitTimes::default())
    }

    pub fn addr(&self) -> SocketAddr {
        self.app.local_addr()
    }

    pub fn now(&self) -> DateTime<Utc> {
        self.clock.now()
    }

    /// 그 시각에 받은 시세 하나를 넣는다(시계도 그 시각으로).
    pub async fn quote(&self, symbol: &str, price: i64) {
        let at = self.now();
        self.clock.set(at);
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

    /// 금액 한도를 낮춘 스윙 템플릿(종목 비중 10% · 자동 주문 금액 2%)을 고르고 오늘 매매하기로 한다.
    pub async fn ready_for_approval(&self) {
        swing_with(
            self.addr(),
            json!({"symbol_weight_pct": "10", "auto_order_limit_pct": "2"}),
        )
        .await;
        let state = self.trading().decide_day(DayPlan::Trade).unwrap();
        assert_eq!(state, TeamState::Operating, "매매하기로 하면 운용 중이다");
    }

    /// 논의중 매매 하나(지금 시각에 만든다).
    fn discussing(&self, symbol: &str) -> TradeId {
        self.store()
            .create_trade(NewTrade {
                symbol: symbol.to_string(),
                account: AccountKind::Paper,
                at: UnixMillis(self.now().timestamp_millis()),
            })
            .unwrap()
            .id
    }

    /// 옛 모양으로 판단됨인 매매(담당 점수만 — 의견 · 팀장 요약 기록이 없다, 방향 사기).
    pub fn judged_plain(&self, symbol: &str) -> TradeId {
        let trade = self.discussing(symbol);
        let at = UnixMillis(self.now().timestamp_millis());
        self.store()
            .judge_trade_with_side(trade, judgment(at), TradeSide::Buy)
            .unwrap();
        trade
    }

    /// 담당 의견([`OPINIONS`]의 넷, 팀 차례와 다르게 준다)과 팀장 요약을 함께 남긴 판단됨 매매.
    pub fn judged_with_opinions(&self, symbol: &str, side: TradeSide) -> TradeId {
        let trade = self.discussing(symbol);
        let at = UnixMillis(self.now().timestamp_millis());
        let summary = TradeSummary {
            summary: LEADER_SUMMARY.to_string(),
            reversal: LEADER_REVERSAL.to_string(),
        };
        self.store()
            .judge_trade_with_opinions(trade, judgment(at), side, &opinions(), &summary)
            .unwrap();
        trade
    }

    /// 판단됨 매매의 사기 주문 안(지정가 · AI 수량 `quantity`)을 낸다. 금액 한도를 넘어 승인을 기다려야 한다.
    pub async fn awaiting(&self, trade: TradeId, quantity: &str) -> (ApprovalNumber, Fingerprint) {
        self.quote(HYNIX, ENTRY).await;
        let request = order(trade, HYNIX, d(quantity), self.now());
        let placed = self
            .trading()
            .place(request)
            .await
            .expect("주문 안을 처리해야 한다");
        let Placed::AwaitingApproval {
            number,
            fingerprint,
        } = placed
        else {
            panic!("금액 한도를 넘으면 승인을 기다린다: {placed:?}");
        };
        (number, fingerprint)
    }
}

/// 담당 점수(의견을 붙일 담당 넷).
pub const SCORED: [(&str, i32); 4] = [("scout", 0), ("strategy", 80), ("risk", 60), ("review", 40)];
/// 팀장 요약 · 뒤집힐 조건.
pub const LEADER_SUMMARY: &str = "실적이 좋아지고 거래량이 늘었다. 비중은 작게 시작한다.";
pub const LEADER_REVERSAL: &str = "거래량이 다시 줄면 뒤집힌다";
/// 담당 의견의 이유 글.
pub const STRATEGY_REASON: &str = "실적 개선이 이어진다";
pub const RISK_REASON: &str = "변동성이 크다";
pub const REVIEW_REASON: &str = "지난번 같은 판단이 틀렸다";
pub const SCOUT_REASON: &str = "자료를 받지 못했다";
/// 매매 근거의 자료 이름.
pub const SOURCE_NAME: &str = "시세";

/// 담당 의견 넷. 일부러 팀 차례(뉴스 → 전략 → 리스크 → 복기)와 다르게 준다.
/// 뉴스 담당은 기권, 복기 담당은 보는 기간이 없다.
fn opinions() -> Vec<RoleOpinion> {
    let line = |role: &str, stance, conviction, reason: &str, horizon| RoleOpinion {
        role: role.to_string(),
        stance,
        conviction,
        reason: reason.to_string(),
        horizon_days: horizon,
        abstained: stance.is_none(),
    };
    vec![
        line(
            "risk",
            Some(Stance::Neutral),
            Some(Conviction::Medium),
            RISK_REASON,
            Some(5),
        ),
        line("scout", None, None, SCOUT_REASON, None),
        line(
            "review",
            Some(Stance::Bearish),
            Some(Conviction::Low),
            REVIEW_REASON,
            None,
        ),
        line(
            "strategy",
            Some(Stance::Bullish),
            Some(Conviction::High),
            STRATEGY_REASON,
            Some(20),
        ),
    ]
}

/// 근거 6칸이 다 찬 판단(판단 시각 `at`, 자료 시각도 `at`, 담당 점수는 [`SCORED`]).
fn judgment(at: UnixMillis) -> Judgment {
    Judgment {
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
            name: SOURCE_NAME.to_string(),
            known_at: at,
        }],
        scores: SCORED
            .iter()
            .map(|(role, score)| RoleScore {
                role: (*role).to_string(),
                score: *score,
                reason: "근거 충분".to_string(),
            })
            .collect(),
        consensus_score: 70,
    }
}

/// 승인 모듈이 읽는 시계: 테스트 시계 그대로.
struct ApprovalClock(Arc<TestClock>);

impl pi_approval::Clock for ApprovalClock {
    fn now(&self) -> UnixMillis {
        UnixMillis(self.0.now().timestamp_millis())
    }
}

/// 지정가 사기 주문 안(AI 수량 `quantity`).
fn order(trade: TradeId, symbol: &str, quantity: Decimal, at: DateTime<Utc>) -> OrderRequest {
    let draft = OrderDraft {
        market: Market::Kr,
        symbol: Symbol::new(symbol),
        purpose: Purpose::NewEntry,
        order_type: OrderType::Limit,
        limit_price: Some(Decimal::from(ENTRY)),
        ai_quantity: Some(quantity),
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
            name: SOURCE_NAME.to_string(),
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
    let deadline = std::time::Instant::now() + STEP;
    while !done() {
        assert!(
            std::time::Instant::now() < deadline,
            "시간 안에 되지 않았다: {what}"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// 저장된 그 종류 이벤트들의 내용.
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

/// 세션을 받아 읽는다.
pub async fn get(addr: SocketAddr, path: &str) -> Reply {
    let cookie = login(addr).await;
    let request = reqwest::Client::new()
        .get(format!("{}{path}", origin_of(addr)))
        .header("Cookie", cookie);
    send(request).await
}

/// 세션을 받아 같은 출처에서 보낸다.
async fn post(addr: SocketAddr, path: &str, body: &Value) -> Reply {
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
