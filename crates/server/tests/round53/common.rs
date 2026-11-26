//! 24차(순찰 · 익절 손절 연출 · "일하는 중" 풀기) 시험의 공용 도우미: 실행 파일 조립(`--demo`)으로 띄우기 · 가짜 AI 뜸 ·
//! 시계 · 바깥 주소 자리의 로컬 가짜 서버 · 세션 · 요청 · 이벤트 읽기 · 매매(판단됨 → 보유중 → 팔림).
//! round35의 도우미와 round9b의 주문 안 도우미를 본떴다(그 폴더들은 고치지 않는다). 바깥 주소 자리는 모두 로컬
//! 가짜 서버다. 실제 증권사 · AI 서버를 부르지 않는다. 계좌는 모의계좌뿐이다.
//!
//! 시간(300초 등)은 넣어 주는 시계([`TestClock`])를 옮겨 잰다 — 실제로 자지 않는다. 가짜 AI 뜸과 같은 길이만 실제로 잔다.

use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use chrono::{DateTime, FixedOffset, NaiveDate, TimeZone, Utc};
use pi_ai::AnthropicConfig;
use pi_data::{DataConfig, NAVER_NEWS_KEYS, NaverNewsConfig};
use pi_market::{Clock, Currency, Market, Quote, Symbol, TossConfig};
use pi_paper::OrderType;
use pi_rules::{
    Funding, OrderDraft, ProductKind, Purpose, Rationale, SourceStamp, SymbolStatus, TradeStage,
};
use pi_secrets::{
    Credentials, KeyKind, KeySlot, ProviderKeys, SecretStore, Verifier, VerifyFailure,
};
use pi_store::{
    AccountKind, Amount, Judgment, NewTrade, RoleScore, SourceRef, Store, TradeId, TradeState,
    UnixMillis,
};
use price_inha::{
    App, DayPlan, OrderRequest, Placed, RULES_CONFIG_PATH, RunAssembly, Settings,
    TEMPLATE_ADJUST_PATH, TEMPLATE_PATH, Trading, Upstreams, run_assembly, save_watchlist,
};
use rust_decimal::Decimal;
use serde_json::{Value, json};
use tempfile::TempDir;
use wiremock::matchers::any;
use wiremock::{Mock, MockServer, ResponseTemplate};

/// 한 걸음을 기다리는 가장 긴 시간.
pub const STEP: Duration = Duration::from_secs(15);
/// 일어나지 않아야 하는 일을 지켜보는 시간.
pub const QUIET: Duration = Duration::from_millis(900);
/// 지켜볼 종목(시연 시세가 흐른다). 이 종목으로는 사고팔지 않는다.
const WATCHED: &str = "005930";
/// 사고파는 종목(SK하이닉스).
pub const HYNIX: &str = "000660";
/// 매매 근거의 진입가 · 손절값 · 목표값 · 주당 기대값(원).
pub const ENTRY: i64 = 100_000;
pub const STOP: i64 = 95_000;
const TARGET: i64 = 110_000;
const EDGE: i64 = 5_000;
/// 주문 안의 AI 수량.
const QUANTITY: i64 = 5;

// ───────────── 시계 · 날짜 ─────────────

/// 시험이 정하는 시계. 자는 것은 짧게만 잔다. 시험이 정한 가짜 AI 뜸과 같은 길이는 그만큼 실제로 잔다.
pub struct TestClock {
    now: AtomicI64,
    pause: Duration,
}

impl TestClock {
    fn at(time: DateTime<Utc>, pause: Duration) -> Arc<Self> {
        Arc::new(Self {
            now: AtomicI64::new(time.timestamp_millis()),
            pause,
        })
    }

    fn set(&self, time: DateTime<Utc>) {
        self.now.store(time.timestamp_millis(), Ordering::SeqCst);
    }

    fn read(&self) -> DateTime<Utc> {
        Utc.timestamp_millis_opt(self.now.load(Ordering::SeqCst))
            .unwrap()
    }
}

#[async_trait]
impl Clock for TestClock {
    fn now(&self) -> DateTime<Utc> {
        self.read()
    }

    async fn sleep(&self, duration: Duration) {
        if !self.pause.is_zero() && duration == self.pause {
            tokio::time::sleep(duration).await;
        } else {
            tokio::time::sleep(duration.min(Duration::from_millis(10))).await;
        }
    }
}

/// 시험의 기본 시각: 2026-11-21 토요일 10:00(한국). 시연 달력은 매일을 거래일로 본다.
pub fn morning() -> DateTime<Utc> {
    let date = NaiveDate::from_ymd_opt(2026, 11, 21).unwrap();
    FixedOffset::east_opt(9 * 3600)
        .unwrap()
        .from_local_datetime(&date.and_hms_opt(10, 0, 0).unwrap())
        .unwrap()
        .with_timezone(&Utc)
}

// ───────────── "실제" 주소 자리의 가짜 서버 ─────────────

/// 무엇을 물어도 503으로 답하는 로컬 가짜 서버.
async fn refusing_server() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(any())
        .respond_with(ResponseTemplate::new(503))
        .mount(&server)
        .await;
    server
}

/// 증권사 · AI · 자료 · 뉴스 주소 자리의 가짜 서버들.
struct Outside {
    broker: MockServer,
    ai: MockServer,
    data: MockServer,
    news: MockServer,
}

impl Outside {
    async fn start() -> Outside {
        Outside {
            broker: refusing_server().await,
            ai: refusing_server().await,
            data: refusing_server().await,
            news: refusing_server().await,
        }
    }
}

// ───────────── 보관 폴더 · 설정 · 조립 ─────────────

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
        "--retry-initial-ms",
        "20",
        "--retry-max-ms",
        "40",
        "--demo",
    ];
    let args: Vec<String> = list.iter().map(|s| (*s).to_string()).collect();
    Settings::parse(&args, &no_env, None).expect("설정을 읽어야 한다")
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

/// 띄우기 전 보관 폴더에 모든 키를 등록하고 지켜볼 종목을 저장한다. 값은 진짜 키처럼 보이지 않는다.
async fn prepare(home: &Path) {
    let secrets = SecretStore::open(home).unwrap();
    let broker = Credentials::new()
        .with("client_id", "test-client-id-round53")
        .with("client_secret", "test-client-secret-round53");
    secrets
        .register(KeyKind::Broker, broker, &AcceptAll)
        .await
        .unwrap();
    let ai = Credentials::new().with("api_key", "test-ai-key-round53-zz");
    secrets
        .register(KeyKind::AiApiKey, ai, &AcceptAll)
        .await
        .unwrap();
    let slots = [
        (KeySlot::Dart, "test-dart-key-round53-zz"),
        (KeySlot::Ecos, "test-ecos-key-round53-zz"),
        (KeySlot::Fred, "test-fred-key-round53-zz"),
    ];
    for (slot, value) in slots {
        let values = Credentials::new().with(slot.fields()[0], value);
        secrets
            .register_slot(slot, values, &AcceptAll)
            .await
            .unwrap();
    }
    let naver = Credentials::new()
        .with("client_id", "test-naver-id-round53")
        .with("client_secret", "test-naver-secret-round53");
    secrets
        .register_provider(NAVER_NEWS_KEYS.provider_keys(), naver, &AcceptAll)
        .await
        .unwrap();
    save_watchlist(home, &[Symbol::new(WATCHED)]).unwrap();
}

/// 실행 파일 조립에서 시계 · 바깥 주소 · CLI 찾는 자리 · 가짜 AI 뜸만 바꾼다.
fn bend(
    mut assembly: RunAssembly,
    clock: &Arc<TestClock>,
    outside: &Outside,
    pause: Duration,
) -> RunAssembly {
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
    assembly.demo_ai_pause = pause;
    assembly
}

/// 떠 있는 서버 하나와 그 보관 폴더 · 시계 · 가짜 서버들.
pub struct Bench {
    _dir: TempDir,
    _outside: Outside,
    clock: Arc<TestClock>,
    app: App,
}

impl Bench {
    /// 가짜 AI 뜸(없으면 0)을 두고 토요일 10:00에 띄운다.
    pub async fn start(pause: Duration) -> Bench {
        let dir = tempfile::tempdir().unwrap();
        prepare(dir.path()).await;
        let clock = TestClock::at(morning(), pause);
        let outside = Outside::start().await;
        let settings = settings(dir.path());
        let assembly = bend(run_assembly(&settings, None), &clock, &outside, pause);
        let app = tokio::time::timeout(STEP, App::launch_run(settings, assembly))
            .await
            .expect("서버가 시간 안에 떠야 한다")
            .expect("서버가 떠야 한다");
        Bench {
            _dir: dir,
            _outside: outside,
            clock,
            app,
        }
    }

    /// 끈다.
    pub async fn close(self) {
        tokio::time::timeout(STEP, self.app.shutdown())
            .await
            .expect("시간 안에 꺼져야 한다")
            .expect("끄기가 성공해야 한다");
    }

    pub fn addr(&self) -> SocketAddr {
        self.app.local_addr()
    }

    pub fn store(&self) -> Store {
        self.app.store()
    }

    pub fn trading(&self) -> Arc<Trading> {
        self.app.trading()
    }

    pub fn now(&self) -> DateTime<Utc> {
        self.clock.read()
    }

    /// 시계를 `seconds`초 옮긴다(실제로 자지 않는다).
    pub fn pass(&self, seconds: i64) {
        self.clock
            .set(self.now() + chrono::Duration::seconds(seconds));
    }

    /// 매매하기 좋게 만든다: 스윙 템플릿을 고르고 종목 비중 10%로 맞추고, 사고팔 종목의 업종을 정한 뒤 오늘 매매하기로
    /// 한다(운용 중).
    pub async fn ready(&self) {
        let reply = self.post(TEMPLATE_PATH, &json!({"style": "swing"})).await;
        assert_eq!(reply.status, 200, "템플릿을 골라야 한다: {}", reply.text);
        let id = reply.body["id"].as_i64().expect("템플릿 번호");
        let body = json!({"id": id, "changes": {"symbol_weight_pct": "10"}});
        let reply = self.post(TEMPLATE_ADJUST_PATH, &body).await;
        assert_eq!(reply.status, 200, "상한 안의 값은 저장된다: {}", reply.text);
        let rules = json!({"banned": [], "sectors": [{"symbol": HYNIX, "sector": "반도체"}]});
        let reply = self.post(RULES_CONFIG_PATH, &rules).await;
        assert_eq!(reply.status, 200, "업종표를 저장한다: {}", reply.text);
        self.trading()
            .decide_day(DayPlan::Trade)
            .expect("매매하기로 정할 수 있다");
    }

    /// 새 세션으로 같은 출처에서 JSON을 보낸다.
    pub async fn post(&self, path: &str, body: &Value) -> Reply {
        let cookie = session(self.addr()).await;
        post(self.addr(), path, &cookie, body).await
    }

    // ───────────── 매매 ─────────────

    /// 시계를 `seconds`초 옮기고 그 시각에 받은 시세 하나를 넣는다.
    pub async fn quote(&self, symbol: &str, price: i64, seconds: i64) {
        self.pass(seconds);
        let at = self.now();
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

    /// 주문 안 하나를 낸다.
    pub async fn place(&self, request: OrderRequest) -> Placed {
        self.trading()
            .place(request)
            .await
            .expect("주문 안을 처리해야 한다")
    }

    /// 시세 100,000원을 넣고 판단됨 매매를 사서 보유중까지 만든다.
    pub async fn hold(&self, symbol: &str) -> TradeId {
        self.quote(symbol, ENTRY, 1).await;
        let trade = self.judged(symbol);
        let placed = self
            .place(order(trade, symbol, Purpose::NewEntry, ENTRY, self.now()))
            .await;
        assert!(
            matches!(placed, Placed::Ordered(_)),
            "한도 안의 사는 주문은 바로 나간다: {placed:?}"
        );
        self.quote(symbol, ENTRY, 1).await;
        assert_eq!(
            self.trade_state(trade),
            TradeState::Holding,
            "지정가에 닿는 시세가 오면 체결돼 보유중이다"
        );
        trade
    }
}

// ───────────── 주문 안 ─────────────

/// 주문 안: 그 목적 · 지정가로 5주(기준가도 그 값). 사는 주문이면 판단됨, 아니면 보유중 단계다.
pub fn order(
    trade: TradeId,
    symbol: &str,
    purpose: Purpose,
    price: i64,
    at: DateTime<Utc>,
) -> OrderRequest {
    let stage = if purpose == Purpose::NewEntry {
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
        ai_quantity: Some(Decimal::from(QUANTITY)),
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
        reference_price: Decimal::from(price),
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

/// [`QUIET`] 동안 조건이 한 번도 참이 되지 않는지 본다(참이 되면 바로 실패).
pub async fn stays_false(what: &str, mut happened: impl FnMut() -> bool) {
    let deadline = Instant::now() + QUIET;
    while Instant::now() < deadline {
        assert!(!happened(), "일어나면 안 되는 일이 일어났다: {what}");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(!happened(), "일어나면 안 되는 일이 일어났다: {what}");
}

/// 저장된 이벤트 가운데 그 종류인 것의 (번호, 내용)(번호 순서).
pub fn events(store: &Store, kind: &str) -> Vec<(u64, Value)> {
    store
        .read_after(0, 100_000)
        .unwrap()
        .into_iter()
        .filter(|event| event.kind == kind)
        .map(|event| (event.seq, event.payload))
        .collect()
}

// ───────────── HTTP ─────────────

/// 받은 응답: 상태 · 본문 글자 · JSON 본문(JSON이 아니면 `Null`).
pub struct Reply {
    pub status: u16,
    pub text: String,
    pub body: Value,
}

fn origin_of(addr: SocketAddr) -> String {
    format!("http://{addr}")
}

/// 같은 출처에서 세션을 받아 `Cookie` 헤더 글자를 돌려준다.
async fn session(addr: SocketAddr) -> String {
    let origin = origin_of(addr);
    let reply = tokio::time::timeout(
        STEP,
        reqwest::Client::new()
            .post(format!("{origin}{}", pi_gate::SESSION_PATH))
            .header("Origin", &origin)
            .send(),
    )
    .await
    .expect("응답이 시간 안에 와야 한다")
    .expect("요청이 닿아야 한다");
    assert!(reply.status().is_success(), "세션을 받아야 한다");
    reply
        .headers()
        .get("set-cookie")
        .and_then(|v| v.to_str().ok())
        .map(|raw| raw.split(';').next().unwrap().trim().to_string())
        .expect("세션 쿠키가 있어야 한다")
}

/// 같은 출처 · 그 세션으로 JSON을 `POST`한다.
async fn post(addr: SocketAddr, path: &str, cookie: &str, body: &Value) -> Reply {
    let origin = origin_of(addr);
    let request = reqwest::Client::new()
        .post(format!("{}{path}", origin_of(addr)))
        .header("Content-Type", "application/json")
        .header("Cookie", cookie)
        .header("Origin", &origin)
        .body(body.to_string());
    let reply = tokio::time::timeout(STEP, request.send())
        .await
        .expect("응답이 시간 안에 와야 한다")
        .expect("요청이 닿아야 한다");
    let status = reply.status().as_u16();
    let text = reply.text().await.expect("본문");
    let body = serde_json::from_str(&text).unwrap_or(Value::Null);
    Reply { status, text, body }
}
