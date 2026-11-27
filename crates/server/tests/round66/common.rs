//! round66(백로그 T) 시연 발판: 실행 파일의 `--demo` 조립으로 띄우기 · 시계 · 바깥 주소 자리의 로컬 가짜 서버 · 세션 ·
//! 요청 · 이벤트 읽기 · 팔린 매매와 복기 넣기 · 지침 장부 파일 읽고 쓰기 · 논의로 사서 보유중 → 익절 · 손절로 팔기.
//! round60(지침)과 round53(익절 · 손절 연출)의 도우미를 본떴다(그 폴더들은 고치지 않는다). 바깥 주소 자리는 모두
//! 로컬 가짜 서버다. 계좌는 모의계좌뿐이다.
//!
//! 시계는 시험이 정한다(결정 날 2026-11-21 토요일 10:00 — 시연 달력은 매일을 거래일로 본다). 시세는 시험이 넣는다.

use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, FixedOffset, NaiveDate, TimeZone, Utc};
use pi_agent::Decision;
use pi_ai::AnthropicConfig;
use pi_data::{DataConfig, NaverNewsConfig};
use pi_journal::{GuidelineBook, Journal, Mistake};
use pi_market::{Clock, Currency, Market, Quote, Symbol, TossConfig};
use pi_paper::OrderType;
use pi_rules::{
    Funding, OrderDraft, ProductKind, Purpose, Rationale as DraftRationale, SourceStamp,
    SymbolStatus, TradeStage,
};
use pi_secrets::{
    Credentials, KeyKind, KeySlot, ProviderKeys, SecretStore, Verifier, VerifyFailure,
};
use pi_store::{
    AccountKind, Amount, Judgment, NewTrade, Rationale, Store, TradeId, TradeState, UnixMillis,
};
use price_inha::{
    ACCOUNT_PATH, App, Crew, DayPlan, DemoAi, DemoTurn, Discussed, KIND_DAY_START, MessengerWiring,
    OrderRequest, Placed, RULES_CONFIG_PATH, RunAssembly, Settings, TEAM_GUIDELINES_FILE,
    TEMPLATE_ADJUST_PATH, TEMPLATE_PATH, TeamError, Upstreams, run_assembly, save_watchlist,
};
use rust_decimal::Decimal;
use serde_json::{Value, json};
use tempfile::TempDir;
use wiremock::matchers::any;
use wiremock::{Mock, MockServer, ResponseTemplate};

/// 한 걸음을 기다리는 가장 긴 시간.
pub const STEP: Duration = Duration::from_secs(20);
/// 사고파는 종목(지켜볼 종목이 아니다 · 시연 자료가 채워지는 고유번호 있는 종목). 시세는 시험이 넣는다.
pub const HYNIX: &str = "000660";
/// 지켜볼 종목.
const WATCHED: &str = "005930";
/// 진입가(원). 팀장 판단 · 주문 안 · 시세가 이 값이다.
pub const ENTRY: i64 = 100_000;
/// 사는 주문 안의 AI 수량(논의가 주문을 내지 않았을 때만 쓴다).
const QUANTITY: i64 = 5;

/// 팀장의 사기 판단(가짜 AI 답).
fn buy_verdict() -> String {
    json!({
        "decision": "buy", "hypothesis": "시험 가설 실적 개선", "trigger": "시험 계기",
        "entry": ENTRY, "stop": 95_000, "target": 110_000, "expected_value": 5_000
    })
    .to_string()
}

// ───────────── 시계 · 날짜 ─────────────

/// 시험이 정하는 시계. 자는 것은 짧게만 잔다.
pub struct TestClock(AtomicI64);

impl TestClock {
    fn at(time: DateTime<Utc>) -> Arc<Self> {
        Arc::new(Self(AtomicI64::new(time.timestamp_millis())))
    }

    fn read(&self) -> DateTime<Utc> {
        Utc.timestamp_millis_opt(self.0.load(Ordering::SeqCst))
            .unwrap()
    }

    fn set(&self, time: DateTime<Utc>) {
        self.0.store(time.timestamp_millis(), Ordering::SeqCst);
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

/// 결정 날: 2026-11-21 토요일(시연 달력은 매일을 거래일로 본다).
pub fn decision_day() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 11, 21).unwrap()
}

fn kst(date: NaiveDate, hour: u32, minute: u32) -> DateTime<Utc> {
    FixedOffset::east_opt(9 * 3600)
        .unwrap()
        .from_local_datetime(&date.and_hms_opt(hour, minute, 0).unwrap())
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
pub struct Outside {
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

/// 설정. `demo`면 `--demo`(시연 모드 — 메신저 루프는 돌지 않는다).
fn settings(home: &Path, demo: bool) -> Settings {
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
    ];
    let mut args: Vec<String> = list.iter().map(|s| (*s).to_string()).collect();
    if demo {
        args.push("--demo".to_string());
    }
    Settings::parse(&args, &no_env, None).expect("설정을 읽어야 한다")
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

    async fn verify_provider(
        &self,
        _keys: ProviderKeys,
        _values: &Credentials,
    ) -> Result<(), VerifyFailure> {
        Ok(())
    }
}

/// 띄우기 전 보관 폴더에 증권사 · AI 키를 등록하고 지켜볼 종목을 저장한다. 값은 진짜 키처럼 보이지 않는다.
async fn prepare(home: &Path) {
    let secrets = SecretStore::open(home).unwrap();
    let broker = Credentials::new()
        .with("client_id", "test-client-id-round66")
        .with("client_secret", "test-client-secret-round66");
    secrets
        .register(KeyKind::Broker, broker, &AcceptAll)
        .await
        .unwrap();
    let ai = Credentials::new().with("api_key", "test-ai-key-round66-zz");
    secrets
        .register(KeyKind::AiApiKey, ai, &AcceptAll)
        .await
        .unwrap();
    save_watchlist(home, &[Symbol::new(WATCHED)]).unwrap();
}

/// 실행 파일 조립에서 시계 · 바깥 주소 · CLI 찾는 자리만 바꾼다.
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

/// 띄운 서버와 그 보관 폴더 · 바깥 자리 · 시계.
pub struct Launched {
    pub dir: TempDir,
    pub outside: Outside,
    pub clock: Arc<TestClock>,
    pub app: App,
}

impl Launched {
    /// 키를 등록한 새 보관 폴더로 결정 날 10:00(한국)에 띄운다. `messenger`가 있으면 그 메신저 조립으로.
    pub async fn start(demo: bool, messenger: Option<MessengerWiring>) -> Launched {
        let dir = tempfile::tempdir().unwrap();
        prepare(dir.path()).await;
        let clock = TestClock::at(kst(decision_day(), 10, 0));
        let outside = Outside::start().await;
        let settings = settings(dir.path(), demo);
        let mut assembly = bend(run_assembly(&settings, None), &clock, &outside);
        if let Some(messenger) = messenger {
            assembly.messenger = messenger;
        }
        let app = tokio::time::timeout(STEP, App::launch_run(settings, assembly))
            .await
            .expect("서버가 시간 안에 떠야 한다")
            .expect("서버가 떠야 한다");
        Launched {
            dir,
            outside,
            clock,
            app,
        }
    }
}

/// 떠 있는 서버 하나와 그 보관 폴더 · 시계 · 가짜 서버들.
pub struct Bench {
    dir: TempDir,
    _outside: Outside,
    clock: Arc<TestClock>,
    app: App,
}

impl Bench {
    /// 결정 날 10:00(한국)에 띄우고 팀장 판단을 사기로 정한 뒤 매매하기 좋게 만든다.
    pub async fn start() -> Bench {
        let launched = Launched::start(true, None).await;
        let bench = Bench {
            dir: launched.dir,
            _outside: launched.outside,
            clock: launched.clock,
            app: launched.app,
        };
        bench.script(DemoTurn::Verdict, &buy_verdict());
        bench.ready().await;
        bench
    }

    /// 끈다.
    pub async fn close(self) {
        tokio::time::timeout(STEP, self.app.shutdown())
            .await
            .expect("시간 안에 꺼져야 한다")
            .expect("끄기가 성공해야 한다");
    }

    fn addr(&self) -> SocketAddr {
        self.app.local_addr()
    }

    pub fn store(&self) -> Store {
        self.app.store()
    }

    fn crew(&self) -> Arc<Crew> {
        self.app.crew().expect("실행 파일 조립은 팀을 잇는다")
    }

    pub fn demo_ai(&self) -> Arc<DemoAi> {
        self.crew().demo_ai().expect("시연 모드다")
    }

    /// 가짜 AI의 답을 정한다.
    pub fn script(&self, turn: DemoTurn, reply: &str) {
        self.demo_ai().script(turn, reply);
    }

    /// 가짜 AI가 받은 글 가운데 `needle`이 든 것의 수.
    pub fn prompts_with(&self, needle: &str) -> usize {
        self.demo_ai()
            .prompts()
            .iter()
            .filter(|p| p.contains(needle))
            .count()
    }

    fn now(&self) -> DateTime<Utc> {
        self.clock.read()
    }

    /// 매매하기 좋게 만든다: 스윙 템플릿 · 종목 비중 10% · 사고팔 종목의 업종 · 오늘 매매하기(운용 중).
    async fn ready(&self) {
        let reply = self.post(TEMPLATE_PATH, &json!({"style": "swing"})).await;
        assert_eq!(reply.status, 200, "템플릿을 골라야 한다: {}", reply.text);
        let id = reply.body["id"].as_i64().expect("템플릿 번호");
        let body = json!({"id": id, "changes": {"symbol_weight_pct": "10"}});
        let reply = self.post(TEMPLATE_ADJUST_PATH, &body).await;
        assert_eq!(reply.status, 200, "상한 안의 값은 저장된다: {}", reply.text);
        let rules = json!({"banned": [], "sectors": [{"symbol": HYNIX, "sector": "반도체"}]});
        let reply = self.post(RULES_CONFIG_PATH, &rules).await;
        assert_eq!(reply.status, 200, "업종표를 저장한다: {}", reply.text);
        self.app
            .trading()
            .decide_day(DayPlan::Trade)
            .expect("매매하기로 정할 수 있다");
    }

    /// 시계를 1초 옮기고 그 시각에 받은 그 종목 시세 하나를 넣는다.
    pub async fn quote(&self, symbol: &str, price: i64) {
        self.clock.set(self.now() + chrono::Duration::seconds(1));
        let at = self.now();
        let quote = Quote {
            symbol: Symbol::new(symbol),
            price: Decimal::from(price),
            currency: Currency::Krw,
            exchange_time: Some(at),
            received_at: at,
        };
        self.app
            .trading()
            .on_quote(quote)
            .await
            .expect("시세를 받아야 한다");
    }

    /// 진입가 시세를 넣고 그 종목을 논의한다(가짜 AI의 팀장 판단은 사기).
    pub async fn discuss(&self, symbol: &str) -> Result<Discussed, TeamError> {
        self.quote(symbol, ENTRY).await;
        tokio::time::timeout(STEP, self.crew().discuss(symbol, "시험 논의"))
            .await
            .expect("논의가 시간 안에 끝나야 한다")
    }

    /// 시세를 넣지 않고 그 종목을 논의한다(사기로 판단된 뒤 주문 안을 만들 시세가 없어 논의가 오류로 끝난다).
    pub async fn discuss_unquoted(&self, symbol: &str) -> Result<Discussed, TeamError> {
        tokio::time::timeout(STEP, self.crew().discuss(symbol, "시험 논의"))
            .await
            .expect("논의가 시간 안에 끝나야 한다")
    }

    /// 논의로 사기로 판단된 매매를 보유중까지 만든다: 논의가 주문을 냈으면 그대로, 아니면 사는 주문 안을 낸다.
    /// 진입가 시세가 오면 체결돼 보유중이다.
    pub async fn hold_from_discussion(&self, symbol: &str) -> TradeId {
        let discussed = self.discuss(symbol).await.expect("논의가 끝나야 한다");
        assert_eq!(discussed.outcome.decision, Decision::Buy, "사기로 판단된다");
        let trade = discussed.trade;
        if !matches!(discussed.placed, Some(Placed::Ordered(_))) {
            assert_eq!(self.trade_state(trade), TradeState::Judged);
            let request = order(
                trade,
                symbol,
                Purpose::NewEntry,
                ENTRY,
                QUANTITY,
                self.now(),
            );
            let placed = self.place(request).await;
            assert!(matches!(placed, Placed::Ordered(_)), "{placed:?}");
        }
        self.quote(symbol, ENTRY).await;
        assert_eq!(
            self.trade_state(trade),
            TradeState::Holding,
            "체결돼 보유중이다"
        );
        trade
    }

    /// 보유한 수량 모두를 그 목적 · 지정가로 판다. 그 값의 시세가 오면 팔림이다.
    pub async fn sell(&self, trade: TradeId, symbol: &str, purpose: Purpose, price: i64) {
        self.quote(symbol, price).await;
        let held = self.held(symbol).await;
        let request = order(trade, symbol, purpose, price, held, self.now());
        let placed = self.place(request).await;
        assert!(matches!(placed, Placed::Ordered(_)), "{placed:?}");
        self.quote(symbol, price).await;
        assert_eq!(self.trade_state(trade), TradeState::Sold, "팔림이다");
    }

    /// 계좌 화면이 보이는 그 종목의 보유 수량.
    async fn held(&self, symbol: &str) -> i64 {
        let reply = self.get(ACCOUNT_PATH).await;
        assert_eq!(reply.status, 200, "{}", reply.text);
        let holdings = reply.body["holdings"].as_array().expect("보유 목록");
        let row = holdings
            .iter()
            .find(|h| h["symbol"] == json!(symbol))
            .unwrap_or_else(|| panic!("보유에 {symbol}: {}", reply.body));
        let quantity = row["quantity"].as_str().expect("수량 글자");
        quantity.parse::<Decimal>().unwrap().try_into().unwrap()
    }

    async fn place(&self, request: OrderRequest) -> Placed {
        self.app
            .trading()
            .place(request)
            .await
            .expect("주문 안을 처리해야 한다")
    }

    pub fn trade_state(&self, id: TradeId) -> TradeState {
        self.store().get_trade(id).unwrap().state
    }

    /// 하루 마감(복기 · 정산 · 지침)을 부른다.
    pub async fn close_day(&self) {
        tokio::time::timeout(STEP, self.crew().review_day())
            .await
            .expect("마감이 시간 안에 끝나야 한다")
            .expect("마감이 끝나야 한다");
    }

    /// 새 세션으로 같은 출처에서 JSON을 보낸다.
    async fn post(&self, path: &str, body: &Value) -> Reply {
        let cookie = session(self.addr()).await;
        post(self.addr(), path, &cookie, body).await
    }

    /// 새 세션으로 읽는다.
    pub async fn get(&self, path: &str) -> Reply {
        let cookie = session(self.addr()).await;
        get(self.addr(), path, Some(&cookie)).await
    }

    /// 지침 장부 파일의 JSON. 없으면 `Null`.
    pub fn ledger(&self) -> Value {
        std::fs::read(self.dir.path().join(TEAM_GUIDELINES_FILE))
            .map(|bytes| serde_json::from_slice(&bytes).expect("장부는 JSON이다"))
            .unwrap_or(Value::Null)
    }

    /// 지침 장부 파일을 그 장부로 둔다(다음 하루 정리가 읽는다).
    pub fn write_ledger(&self, book: &GuidelineBook) {
        std::fs::write(
            self.dir.path().join(TEAM_GUIDELINES_FILE),
            serde_json::to_vec(book).unwrap(),
        )
        .unwrap();
    }

    /// 서버가 그 날들의 정규장 세션을 본 것처럼 하루 시작 기록을 남긴다(하루 정리는 돌지 않은 날).
    pub fn mark_days(&self, dates: &[String]) {
        for date in dates {
            self.store()
                .append(KIND_DAY_START, &json!({ "date": date }))
                .unwrap();
        }
    }

    /// 팔린 매매 `n`건을 만들고 각각 그 실수로 복기를 남긴다. 매매 번호들을 돌려준다.
    pub fn reviewed_with(&self, n: usize, mistake: &Mistake) -> Vec<TradeId> {
        let store = self.store();
        let journal = Journal::new(store.clone());
        let at = UnixMillis(self.now().timestamp_millis());
        (0..n)
            .map(|_| {
                let trade = store
                    .create_trade(NewTrade {
                        symbol: HYNIX.to_string(),
                        account: AccountKind::Paper,
                        at,
                    })
                    .unwrap();
                store.judge_trade(trade.id, judgment(at)).unwrap();
                for step in [TradeState::Ordered, TradeState::Holding, TradeState::Sold] {
                    store.transition_trade(trade.id, step, at).unwrap();
                }
                journal
                    .review_trade(trade.id, vec![mistake.clone()], "시험 복기", at)
                    .unwrap();
                trade.id
            })
            .collect()
    }
}

/// 근거가 다 찬 판단(사기).
fn judgment(at: UnixMillis) -> Judgment {
    Judgment {
        rationale: Rationale {
            hypothesis: Some("시험 가설".into()),
            trigger: Some("시험 계기".into()),
            entry: Some(Amount(ENTRY)),
            stop: Some(Amount(95_000)),
            target: Some(Amount(110_000)),
            expected_value: Some(Amount(5_000)),
        },
        decided_at: at,
        sources: Vec::new(),
        scores: Vec::new(),
        consensus_score: 210,
    }
}

/// 주문 안: 그 목적 · 지정가 · 수량(기준가도 그 값). 사는 주문이면 판단됨, 아니면 보유중 단계다.
fn order(
    trade: TradeId,
    symbol: &str,
    purpose: Purpose,
    price: i64,
    quantity: i64,
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
        ai_quantity: Some(Decimal::from(quantity)),
        funding: Funding::Cash,
        product: ProductKind::Stock,
        stage,
        decided_at: at,
        rationale: DraftRationale {
            hypothesis: Some("시험 가설 실적 개선".to_string()),
            trigger: Some("시험 계기".to_string()),
            entry: Some(Decimal::from(ENTRY)),
            stop: Some(Decimal::from(95_000)),
            target: Some(Decimal::from(110_000)),
            expected_value: Some(Decimal::from(5_000)),
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

/// 저장된 이벤트 가운데 그 종류인 것의 내용(번호 순서).
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

/// 받은 응답: 상태 · 본문 글자 · JSON 본문(JSON이 아니면 `Null`).
pub struct Reply {
    pub status: u16,
    pub text: String,
    pub body: Value,
}

async fn send(request: reqwest::RequestBuilder) -> Reply {
    let reply = tokio::time::timeout(STEP, request.send())
        .await
        .expect("응답이 시간 안에 와야 한다")
        .expect("요청이 닿아야 한다");
    let status = reply.status().as_u16();
    let text = reply.text().await.expect("본문");
    let body = serde_json::from_str(&text).unwrap_or(Value::Null);
    Reply { status, text, body }
}

fn origin_of(addr: SocketAddr) -> String {
    format!("http://{addr}")
}

/// 같은 출처에서 세션을 받아 `Cookie` 헤더 글자를 돌려준다.
pub async fn session(addr: SocketAddr) -> String {
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

/// `GET` (쿠키가 있으면 그 세션으로).
pub async fn get(addr: SocketAddr, path: &str, cookie: Option<&str>) -> Reply {
    let mut request = reqwest::Client::new().get(format!("{}{path}", origin_of(addr)));
    if let Some(cookie) = cookie {
        request = request.header("Cookie", cookie);
    }
    send(request).await
}

/// 같은 출처 · 그 세션으로 JSON을 `POST`한다.
pub async fn post(addr: SocketAddr, path: &str, cookie: &str, body: &Value) -> Reply {
    let origin = origin_of(addr);
    let request = reqwest::Client::new()
        .post(format!("{}{path}", origin_of(addr)))
        .header("Content-Type", "application/json")
        .header("Cookie", cookie)
        .header("Origin", origin)
        .body(body.to_string());
    send(request).await
}
