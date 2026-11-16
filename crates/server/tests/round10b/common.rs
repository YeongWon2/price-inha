//! round10b 테스트가 함께 쓰는 도우미: 시계 · 달력 · 모의계좌 설정 · 서버 띄우기 · HTTP · 매매.
//!
//! 서버는 임시 보관 폴더 · `127.0.0.1:0` · 시연 모드로 띄우고 투자 팀을 잇는다([`price_inha::App::launch`]).
//! AI는 시연 모드의 가짜 공급자뿐이고, 자료 자리는 아무것도 답하지 않는 로컬 가짜 서버(wiremock)뿐이다.
//! 토스 · Anthropic 자리는 아무도 듣지 않는 로컬 주소다.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, Datelike, NaiveDate, TimeZone, Utc, Weekday};
use pi_ai::AnthropicConfig;
use pi_data::DataConfig;
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
use pi_store::{
    AccountKind, Amount, Judgment, NewTrade, RoleScore, SourceRef, Store, TeamState, TradeId,
    TradeState, UnixMillis,
};
use price_inha::{
    App, CalendarFn, Crew, DayPlan, DemoAi, DemoTurn, Launch, OrderRequest, Placed, Settings,
    TEMPLATE_ADJUST_PATH, TEMPLATE_PATH, TeamWiring, Trading, TradingWiring, Upstreams, Wiring,
};
use rust_decimal::Decimal;
use serde_json::{Value, json};
use tempfile::TempDir;
use wiremock::MockServer;

/// 한 걸음을 기다리는 가장 긴 시간.
pub const STEP: Duration = Duration::from_secs(15);
/// 지켜볼 종목(시연 시세가 흐른다).
const WATCHED: &str = "005930";
/// 매매하는 종목(SK하이닉스). 지켜보지 않아 테스트가 시세를 넣는다.
pub const HYNIX: &str = "000660";
/// 둘째로 매매하는 종목(NAVER).
pub const NAVER: &str = "035420";
/// 매매 근거의 진입가 · 손절값 · 목표값 · 주당 기대값(원).
const ENTRY: i64 = 100_000;
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

/// 떠 있는 서버 하나와 그 보관 폴더 · 시계 · 자료 자리의 빈 가짜 서버.
pub struct Bench {
    dir: TempDir,
    clock: Arc<TestClock>,
    pub app: App,
    _data: MockServer,
}

impl Bench {
    /// 새 보관 폴더로 그 시각에 띄운다.
    pub async fn start(at: DateTime<Utc>) -> Bench {
        let dir = tempfile::tempdir().unwrap();
        let data = MockServer::start().await;
        let uri = data.uri();
        let mut team = TeamWiring::official();
        team.data = DataConfig::new(uri.clone(), uri.clone(), uri);
        team.data.timeout = Duration::from_secs(3);
        team.corp_codes = HashMap::new();
        let clock = TestClock::at(at);
        let launch = Launch {
            trading: trading_wiring(),
            team: Some(team),
            secrets: None,
        };
        let start = App::launch(settings(dir.path()), wiring(Arc::clone(&clock)), launch);
        let app = tokio::time::timeout(STEP, start)
            .await
            .expect("서버가 시간 안에 떠야 한다")
            .expect("서버가 떠야 한다");
        Bench {
            dir,
            clock,
            app,
            _data: data,
        }
    }

    /// 끈다.
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
