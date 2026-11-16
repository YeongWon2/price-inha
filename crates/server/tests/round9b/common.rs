//! round9b 테스트가 함께 쓰는 도우미: 시계 · 달력 · 모의계좌 설정 · 서버 띄우기 · HTTP · 매매 · 주문 안.
//!
//! 서버는 임시 보관 폴더로 `127.0.0.1:0`에 띄운다. 바깥 서버(토스 · Anthropic)는 아무도 듣지 않는 로컬
//! 주소로 둔다. 시세는 테스트가 [`price_inha::Trading::on_quote`]로 넣는다(시연 시세가 흐르는 종목과 다른 종목).

use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use chrono::{DateTime, Datelike, NaiveDate, TimeZone, Utc, Weekday};
use pi_ai::AnthropicConfig;
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
use pi_secrets::SecretStore;
use pi_store::{
    AccountKind, Amount, Judgment, NewTrade, RoleScore, SourceRef, Store, TeamState, TradeId,
    TradeState, UnixMillis,
};
use price_inha::{
    App, CalendarFn, DayPlan, OrderRequest, Placed, Settings, TEMPLATE_ADJUST_PATH, TEMPLATE_PATH,
    Trading, TradingWiring, Upstreams, Wiring,
};
use rust_decimal::Decimal;
use serde_json::{Value, json};
use tempfile::TempDir;

/// 한 걸음을 기다리는 가장 긴 시간.
pub const STEP: Duration = Duration::from_secs(10);
/// 사고파는 종목(SK하이닉스). 시연 시세가 흐르는 종목([`WATCHED`])과 다르다.
pub const HYNIX: &str = "000660";
/// 다른 업종 종목(NAVER).
pub const NAVER: &str = "035420";
/// 지켜볼 종목(시연 시세가 흐른다). 이 종목으로는 사고팔지 않는다.
const WATCHED: &str = "005930";
/// 매매 근거의 진입가 · 손절값 · 목표값 · 주당 기대값(원).
const ENTRY: i64 = 100_000;
const STOP: i64 = 95_000;
const TARGET: i64 = 110_000;
const EDGE: i64 = 5_000;
/// 주문 안의 AI 수량(규칙 엔진 값보다 작아 이 값이 된다).
const QUANTITY: &str = "5";

/// 글자를 십진수로.
fn d(text: &str) -> Decimal {
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

/// 2026-11-17 화요일.
pub fn tuesday() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 11, 17).unwrap()
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

/// 원화 10,000,000 · 달러 10,000으로 여는 모의계좌.
fn paper_config() -> PaperConfig {
    let tick = |below: Option<&str>, tick: &str| TickBand {
        below: below.map(d),
        tick: d(tick),
    };
    PaperConfig {
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
    }
}

fn trading_wiring() -> TradingWiring {
    let sectors = HashMap::from([
        (Symbol::new(HYNIX), "반도체".to_string()),
        (Symbol::new(NAVER), "인터넷".to_string()),
    ]);
    let calendar: CalendarFn = Arc::new(kr_calendar);
    TradingWiring {
        paper: paper_config(),
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

/// 시연 모드 설정(지켜볼 종목 하나, 포트 0).
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

/// 떠 있는 서버 하나와 그 보관 폴더 · 시계.
pub struct Bench {
    dir: TempDir,
    clock: Arc<TestClock>,
    app: App,
}

impl Bench {
    /// 새 보관 폴더로 시연 모드 서버를 그 시각에 띄운다.
    pub async fn demo(at: DateTime<Utc>) -> Bench {
        Bench::open(tempfile::tempdir().unwrap(), at).await
    }

    /// 그 보관 폴더로 시연 모드 서버를 그 시각에 띄운다(앱이 비밀 보관소를 스스로 연다).
    pub async fn open(dir: TempDir, at: DateTime<Utc>) -> Bench {
        let clock = TestClock::at(at);
        let start = App::start_with(
            settings(dir.path()),
            wiring(Arc::clone(&clock)),
            trading_wiring(),
        );
        Bench::finish(dir, clock, start).await
    }

    /// 그 보관 폴더로 띄우되, 부르는 쪽이 연 비밀 보관소를 앱에 준다.
    pub async fn sharing(dir: TempDir, secrets: Arc<SecretStore>, at: DateTime<Utc>) -> Bench {
        let clock = TestClock::at(at);
        let start = App::start_with_secrets(
            settings(dir.path()),
            wiring(Arc::clone(&clock)),
            trading_wiring(),
            secrets,
        );
        Bench::finish(dir, clock, start).await
    }

    async fn finish(
        dir: TempDir,
        clock: Arc<TestClock>,
        start: impl Future<Output = Result<App, price_inha::AppError>>,
    ) -> Bench {
        let app = tokio::time::timeout(STEP, start)
            .await
            .expect("서버가 시간 안에 떠야 한다")
            .expect("서버가 떠야 한다");
        Bench { dir, clock, app }
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

    pub fn trading(&self) -> Arc<Trading> {
        self.app.trading()
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

    /// 판단됨인 매매 하나(모의 계좌, 근거 6칸이 다 참). 판단 시각은 지금이다.
    pub fn judged(&self, symbol: &str) -> TradeId {
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
                role: "분석".to_string(),
                score: 70,
                reason: "근거 충분".to_string(),
            }],
            consensus_score: 70,
        };
        store.judge_trade(trade.id, judgment).unwrap();
        trade.id
    }

    pub fn trade_state(&self, id: TradeId) -> TradeState {
        self.store().get_trade(id).unwrap().state
    }

    pub fn team(&self) -> TeamState {
        self.store().team_state().unwrap()
    }

    /// 주문 안 하나를 낸다.
    pub async fn place(&self, request: OrderRequest) -> Placed {
        self.trading()
            .place(request)
            .await
            .expect("주문 안을 처리해야 한다")
    }

    /// 팀장의 오늘 판단: 매매한다. 운용 중이 돼야 한다.
    pub fn trade_today(&self) {
        let state = self.trading().decide_day(DayPlan::Trade).unwrap();
        assert_eq!(state, TeamState::Operating, "매매하기로 하면 운용 중이다");
    }

    /// 지금 시각에 시세 100,000원을 넣고 판단됨 매매를 사서 보유중까지 만든다(바로 나가는 한도 안).
    pub async fn hold(&self, symbol: &str) -> TradeId {
        let at = self.now();
        self.quote(symbol, ENTRY, at).await;
        let trade = self.judged(symbol);
        let placed = self.place(buy(trade, symbol, at)).await;
        assert!(
            matches!(placed, Placed::Ordered(_)),
            "한도 안의 사는 주문은 바로 나간다: {placed:?}"
        );
        self.fill(symbol, trade, ENTRY).await;
        trade
    }

    /// 1초 뒤에 그 값의 시세를 넣어 낸 주문을 체결시킨다(보유중이 된다).
    pub async fn fill(&self, symbol: &str, trade: TradeId, price: i64) {
        self.quote(symbol, price, self.now() + chrono::Duration::seconds(1))
            .await;
        assert_eq!(
            self.trade_state(trade),
            TradeState::Holding,
            "지정가에 닿는 시세가 오면 체결돼 보유중이다"
        );
    }

    /// 모의계좌의 주문 전부(열린 것과 끝난 것).
    pub async fn orders(&self) -> Vec<Order> {
        let trading = self.trading();
        let paper = trading.paper();
        let now = self.now();
        let mut all = Broker::orders(paper, OrderGroup::Open, None, now)
            .await
            .unwrap();
        all.extend(
            Broker::orders(paper, OrderGroup::Closed, None, now)
                .await
                .unwrap(),
        );
        all
    }
}

// ───────────── 주문 안 ─────────────

fn draft(
    symbol: &str,
    purpose: Purpose,
    price: i64,
    stage: TradeStage,
    at: DateTime<Utc>,
) -> OrderDraft {
    OrderDraft {
        market: Market::Kr,
        symbol: Symbol::new(symbol),
        purpose,
        order_type: OrderType::Limit,
        limit_price: Some(Decimal::from(price)),
        ai_quantity: Some(d(QUANTITY)),
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
    }
}

fn request(trade: TradeId, draft: OrderDraft, reference: i64) -> OrderRequest {
    OrderRequest {
        trade_id: trade,
        draft,
        reference_price: Decimal::from(reference),
        status: SymbolStatus {
            vi: Some(false),
            halted: Some(false),
        },
    }
}

/// 새로 사는 주문 안: 지정가 100,000원 5주.
pub fn buy(trade: TradeId, symbol: &str, at: DateTime<Utc>) -> OrderRequest {
    request(
        trade,
        draft(symbol, Purpose::NewEntry, ENTRY, TradeStage::Judged, at),
        ENTRY,
    )
}

/// 보유중인 매매를 파는 주문 안: 그 목적 · 지정가로 5주(기준가도 그 값).
pub fn sell(
    trade: TradeId,
    symbol: &str,
    purpose: Purpose,
    price: i64,
    at: DateTime<Utc>,
) -> OrderRequest {
    request(
        trade,
        draft(symbol, purpose, price, TradeStage::Holding, at),
        price,
    )
}

// ───────────── 기다리기 · 이벤트 ─────────────

/// `done`이 참이 될 때까지 짧게 쉬며 다시 본다. 시간 안에 안 되면 `what`으로 실패한다.
pub async fn wait_for(what: &str, mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + STEP;
    while !done() {
        assert!(Instant::now() < deadline, "시간 안에 되지 않았다: {what}");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// 그 날짜의 그 종류 이벤트가 몇 개인가.
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

/// HTTP 응답 하나: 상태와 JSON 본문(JSON이 아니면 `Null`).
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

/// 세션을 받아 같은 출처에서 JSON 몸통을 보낸다.
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
    let text = reply.text().await.expect("본문을 읽어야 한다");
    let body = serde_json::from_str(&text).unwrap_or(Value::Null);
    Reply { status, body }
}

/// 스윙 템플릿을 고르고 값을 맞춘다. 맞추기는 성공해야 한다.
pub async fn swing_with(addr: SocketAddr, changes: Value) {
    let reply = post(addr, TEMPLATE_PATH, &json!({"style": "swing"})).await;
    assert_eq!(reply.status, 200, "템플릿을 골라야 한다: {}", reply.body);
    let id = reply.body["id"]
        .as_i64()
        .expect("템플릿 번호가 있어야 한다");
    let body = json!({"id": id, "changes": changes});
    let reply = post(addr, TEMPLATE_ADJUST_PATH, &body).await;
    assert_eq!(reply.status, 200, "상한 안의 값은 저장된다: {}", reply.body);
}
