//! 결정 메모 카드 시험의 공용 도우미: 실행 파일의 `--demo` 조립으로 띄우기(하루 AI 상한) · 시계 · 바깥 주소 자리의
//! 로컬 가짜 서버 · 세션 · 요청 · 이벤트 읽기 · 판단된 매매 만들기 · 그날 종가 남기기.
//! round35의 도우미를 본떴다(그 폴더는 고치지 않는다). 바깥 주소 자리는 모두 로컬 가짜 서버다.
//!
//! 시계는 시험이 정한다(실제로 며칠을 기다리지 않는다). 시연 달력은 매일을 거래일로 본다. 종가는 서버가 그날
//! 정규장 마지막 시세를 남기는 저장 이벤트(`reference_prices`)와 같은 모양으로 저장소에 직접 남긴다. 시계를 옮기기
//! 전에 남겨, 그 날짜의 종가로 처음 남은 것이 되게 한다. 지켜볼 종목이 아닌 [`MEMO_SYMBOL`]로 논의해 시연 시세 ·
//! 시연 공시가 그 종목을 건드리지 않게 하고, 판단 전에 그 종목 시세 하나를 넣는다.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, FixedOffset, NaiveDate, TimeZone, Utc};
use pi_agent::Decision;
use pi_ai::AnthropicConfig;
use pi_data::{DataConfig, NaverNewsConfig};
use pi_knowledge::{Knowledge, SearchConfig, SearchOutput};
use pi_market::{Clock, Currency, Quote, Symbol, TossConfig};
use pi_secrets::{
    Credentials, KeyKind, KeySlot, ProviderKeys, SecretStore, Verifier, VerifyFailure,
};
use pi_store::{AccessKeyHash, Store, TradeId, UnixMillis};
use price_inha::{
    App, Check, CheckItem, CommandLine, CommandOutput, CommandRunner, Crew, DayPlan, DemoAi,
    DemoTurn, Os, RunAssembly, SETUP_SEMANTIC_PATH, Service, ServiceSetup, Settings,
    TEMPLATE_ADJUST_PATH, TEMPLATE_PATH, Upstreams, doctor, run_assembly, save_watchlist,
};
use rust_decimal::Decimal;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tempfile::TempDir;
use wiremock::matchers::{any, method, path};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

/// 한 걸음을 기다리는 가장 긴 시간.
pub const STEP: Duration = Duration::from_secs(20);
/// 결정 메모를 만드는 종목(지켜볼 종목이 아니다 · 시연 자료가 채워지는 고유번호 있는 종목). 시세는 시험이 넣는다.
pub const MEMO_SYMBOL: &str = "000660";
/// 다른 종목(다른 종목 교훈을 볼 때, 지켜볼 종목).
pub const OTHER_SYMBOL: &str = "005930";
/// 판단 전에 넣는 시세.
const PRICE: i64 = 100_000;
/// 다른 기기로 보일 때의 상대 주소(사설 대역, 실제로 닿지 않는다).
const REMOTE_PEER: [u8; 4] = [100, 64, 0, 52];
/// 결정 메모 경로.
pub const MEMOS: &str = "/api/memos";

/// 팀장의 사기 판단(가짜 AI 답).
pub fn buy_verdict() -> String {
    json!({
        "decision": "buy", "hypothesis": "시험 가설 실적 개선", "trigger": "시험 계기",
        "entry": 100_000, "stop": 95_000, "target": 110_000, "expected_value": 5_000
    })
    .to_string()
}

/// 복기 담당의 답(교훈 글은 `note`).
pub fn lesson_reply(note: &str) -> String {
    json!({"mistakes": [], "note": note}).to_string()
}

// ───────────── 시계 · 날짜 ─────────────

/// 시험이 정하는 시계. 자는 것은 짧게만 잔다.
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

/// 결정 날: 2026-11-21 토요일(시연 달력은 매일을 거래일로 본다).
pub fn decision_day() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 11, 21).unwrap()
}

/// 결정 날에서 `n`일 뒤(시연 달력에서는 `n`거래일 뒤).
pub fn days_after(n: u64) -> NaiveDate {
    decision_day()
        .checked_add_days(chrono::Days::new(n))
        .unwrap()
}

/// `YYYY-MM-DD` 글자.
pub fn ymd(date: NaiveDate) -> String {
    date.format("%Y-%m-%d").to_string()
}

/// 그 날의 한국 시각(UTC+9).
pub fn kst(date: NaiveDate, hour: u32, minute: u32) -> DateTime<Utc> {
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

/// 띄우기 전 보관 폴더에 증권사 · AI 키를 등록하고 지켜볼 종목을 저장한다. 값은 진짜 키처럼 보이지 않는다.
/// 자료 키는 두지 않는다(시연 자료 어댑터는 키가 있어도 묻지 않는다).
async fn prepare(home: &Path) {
    let secrets = SecretStore::open(home).unwrap();
    let broker = Credentials::new()
        .with("client_id", "test-client-id-round52")
        .with("client_secret", "test-client-secret-round52");
    secrets
        .register(KeyKind::Broker, broker, &AcceptAll)
        .await
        .unwrap();
    let ai = Credentials::new().with("api_key", "test-ai-key-round52-zz");
    secrets
        .register(KeyKind::AiApiKey, ai, &AcceptAll)
        .await
        .unwrap();
    save_watchlist(home, &[Symbol::new(OTHER_SYMBOL)]).unwrap();
}

/// 실행 파일 조립에서 시계 · 바깥 주소 · CLI 찾는 자리 · 상대 주소만 바꾼다.
/// (round64) 조립에서 시험이 바꾸는 값: 가짜 AI의 뜸(논의를 오래 열어 둔다), 뜻 검색 도구의 시간 제한.
#[derive(Clone, Copy)]
pub struct Knobs {
    pub ai_pause: Duration,
    pub semantic_timeout: Duration,
}

impl Default for Knobs {
    fn default() -> Self {
        Knobs {
            ai_pause: Duration::ZERO,
            semantic_timeout: Duration::from_secs(10),
        }
    }
}

fn bend(
    mut assembly: RunAssembly,
    clock: &Arc<TestClock>,
    outside: &Outside,
    remote: &Arc<AtomicBool>,
    knobs: Knobs,
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
    let flag = Arc::clone(remote);
    let peer: pi_gate::PeerOverride = Arc::new(move |addr: SocketAddr| {
        if flag.load(Ordering::SeqCst) {
            SocketAddr::from((REMOTE_PEER, addr.port()))
        } else {
            addr
        }
    });
    assembly.wiring.peer_override = Some(peer);
    if let Some(team) = assembly.launch.team.as_mut() {
        let data = outside.data.uri();
        team.data = DataConfig::new(data.clone(), data.clone(), data);
        team.data.timeout = Duration::from_secs(3);
        team.news = NaverNewsConfig::new(outside.news.uri()).with_timeout(Duration::from_secs(3));
        team.semantic_search_timeout = knobs.semantic_timeout;
    }
    assembly.demo_ai_pause = knobs.ai_pause;
    assembly
}

async fn launch(
    home: &Path,
    clock: &Arc<TestClock>,
    outside: &Outside,
    remote: &Arc<AtomicBool>,
    knobs: Knobs,
) -> App {
    let settings = settings(home);
    let assembly = bend(run_assembly(&settings, None), clock, outside, remote, knobs);
    tokio::time::timeout(STEP, App::launch_run(settings, assembly))
        .await
        .expect("서버가 시간 안에 떠야 한다")
        .expect("서버가 떠야 한다")
}

/// 떠 있는 서버 하나와 그 보관 폴더 · 시계 · 가짜 서버들.
pub struct Bench {
    dir: TempDir,
    outside: Outside,
    clock: Arc<TestClock>,
    remote: Arc<AtomicBool>,
    app: App,
}

impl Bench {
    /// 결정 날 10:00(한국)에 띄우고 팀장 판단을 사기로 정한다.
    pub async fn start() -> Bench {
        Bench::start_with(Knobs::default()).await
    }

    /// (round64) [`Bench::start`]와 같되 조립 값 `knobs`로 띄운다.
    pub async fn start_with(knobs: Knobs) -> Bench {
        let dir = tempfile::tempdir().unwrap();
        prepare(dir.path()).await;
        let clock = TestClock::at(kst(decision_day(), 10, 0));
        let outside = Outside::start().await;
        let remote = Arc::new(AtomicBool::new(false));
        let app = launch(dir.path(), &clock, &outside, &remote, knobs).await;
        let bench = Bench {
            dir,
            outside,
            clock,
            remote,
            app,
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

    pub fn addr(&self) -> SocketAddr {
        self.app.local_addr()
    }

    pub fn store(&self) -> Store {
        self.app.store()
    }

    pub fn crew(&self) -> Arc<Crew> {
        self.app.crew().expect("실행 파일 조립은 팀을 잇는다")
    }

    /// (round64) 보관 폴더.
    pub fn home(&self) -> &Path {
        self.dir.path()
    }

    /// (round64) 이 보관 폴더의 지식 창구(임베딩 없이, 문턱 없음).
    pub fn knowledge(&self) -> Knowledge {
        let config = SearchConfig {
            min_score_micros: 0,
            ..SearchConfig::default()
        };
        Knowledge::new(self.store(), config)
    }

    /// (round64) 뜻 검색 설정을 이 컴퓨터의 세션으로 바꾼다.
    pub async fn semantic(&self, enabled: bool, base_url: &str) -> Reply {
        let body = json!({"enabled": enabled, "base_url": base_url, "model": "bge-m3"});
        self.post(SETUP_SEMANTIC_PATH, &body).await
    }

    /// (round64) 검색 도구를 부른다.
    pub async fn search(&self, input: Value) -> SearchOutput {
        tokio::time::timeout(STEP, self.crew().search_knowledge(&input))
            .await
            .expect("검색이 시간 안에 끝나야 한다")
            .expect("검색은 오류가 아니다")
    }

    /// (round64) 다른 기기의 원격 세션을 받는다(round60과 같은 길). `Cookie` 헤더 글자를 돌려준다.
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
        reply
            .headers()
            .get("set-cookie")
            .and_then(|v| v.to_str().ok())
            .map(|raw| raw.split(';').next().unwrap().trim().to_string())
            .expect("세션 쿠키가 있어야 한다")
    }

    fn demo_ai(&self) -> Arc<DemoAi> {
        self.crew().demo_ai().expect("시연 모드다")
    }

    /// 가짜 AI의 답을 정한다.
    pub fn script(&self, turn: DemoTurn, reply: &str) {
        self.demo_ai().script(turn, reply);
    }

    pub fn now(&self) -> DateTime<Utc> {
        self.clock.read()
    }

    pub fn set(&self, at: DateTime<Utc>) {
        self.clock.set(at);
    }

    /// 다른 기기로 보이게 한다.
    pub fn go_remote(&self) {
        self.remote.store(true, Ordering::SeqCst);
    }

    /// AI 주소 자리의 가짜 서버가 받은 요청 수(시연 모드는 가짜 AI만 쓰므로 0이어야 한다).
    pub async fn outside_ai_requests(&self) -> usize {
        self.outside
            .ai
            .received_requests()
            .await
            .unwrap_or_default()
            .len()
    }

    /// 지금 시각에 받은 그 종목 시세 하나를 넣고, 오늘(시장 날짜) 매매하기로 정해 둔다(이미 정했으면 그대로).
    pub async fn quote(&self, symbol: &str) {
        let _today = self.app.trading().decide_day(DayPlan::Trade);
        let at = self.now();
        let quote = Quote {
            symbol: Symbol::new(symbol),
            price: Decimal::from(PRICE),
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

    /// 매매하기 좋게 만든다: 스윙 템플릿을 고르고 종목 비중 10%로 맞춘 뒤 오늘 매매하기로 한다(운용 중).
    async fn ready(&self) {
        let reply = self.post(TEMPLATE_PATH, &json!({"style": "swing"})).await;
        assert_eq!(reply.status, 200, "템플릿을 골라야 한다: {}", reply.text);
        let id = reply.body["id"].as_i64().expect("템플릿 번호");
        let body = json!({"id": id, "changes": {"symbol_weight_pct": "10"}});
        let reply = self.post(TEMPLATE_ADJUST_PATH, &body).await;
        assert_eq!(reply.status, 200, "상한 안의 값은 저장된다: {}", reply.text);
        self.app
            .trading()
            .decide_day(DayPlan::Trade)
            .expect("매매하기로 정할 수 있다");
    }

    /// 그 종목을 논의해 사기로 판단된 매매를 만든다(가짜 AI의 팀장 판단은 사기).
    pub async fn judged_buy(&self, symbol: &str) -> TradeId {
        self.quote(symbol).await;
        let discussed = tokio::time::timeout(STEP, self.crew().discuss(symbol, "시험 논의"))
            .await
            .expect("논의가 시간 안에 끝나야 한다")
            .expect("논의가 끝나야 한다");
        assert_eq!(discussed.outcome.decision, Decision::Buy, "사기로 판단된다");
        discussed.trade
    }

    /// 그 날 정규장의 마지막 시세(종가)를 남긴다(서버가 남기는 `reference_prices`와 같은 모양).
    pub fn close_on(&self, date: NaiveDate, symbol: &str, price: i64) {
        let payload = json!({"date": ymd(date), "prices": {symbol: price.to_string()}});
        let at = UnixMillis(self.now().timestamp_millis());
        self.store()
            .append_at("reference_prices", &payload, at)
            .expect("종가를 남긴다");
    }

    /// 하루 마감(복기 · 정산)을 부른다.
    pub async fn close_day(&self) {
        tokio::time::timeout(STEP, self.crew().review_day())
            .await
            .expect("마감이 시간 안에 끝나야 한다")
            .expect("마감이 끝나야 한다");
    }

    /// 새 세션으로 같은 출처에서 JSON을 보낸다.
    pub async fn post(&self, path: &str, body: &Value) -> Reply {
        let cookie = session(self.addr()).await;
        post(self.addr(), path, &cookie, body).await
    }

    /// 새 세션으로 읽는다.
    pub async fn get(&self, path: &str) -> Reply {
        let cookie = session(self.addr()).await;
        get(self.addr(), path, Some(&cookie)).await
    }

    /// 결정 메모 목록(`?days=`, 배열)에서 그 매매의 메모.
    pub async fn memo_of(&self, trade: TradeId) -> Value {
        let reply = self.get(&format!("{MEMOS}?days=7")).await;
        assert_eq!(reply.status, 200, "{}", reply.text);
        let list = reply.body.as_array().expect("배열이다").clone();
        list.into_iter()
            .find(|memo| memo["trade"] == trade.0)
            .unwrap_or_else(|| panic!("매매 {}의 메모가 있어야 한다: {}", trade.0, reply.text))
    }
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

/// 다른 기기의 접속 키(시험용, 글자 수 · 글자 모음은 `pi_gate`의 것).
const ACCESS_KEY: &str = "0123456789ABCDEFGHJK";

// ───────────── (round64) Ollama 가짜 서버 ─────────────

/// 가짜 Ollama가 돌려주는 벡터 길이(`bge-m3`, 결정 15번).
pub const EMBED_DIMS: usize = 1024;

/// Ollama 가짜 서버(루프백). `POST /api/embed`는 받은 글 수만큼 [`EMBED_DIMS`]차원 벡터(n번째 글은 n번째 칸이
/// 1인 벡터)를 주고, `GET /api/tags`는 `bge-m3:latest`를 준다. 답을 늦출 수 있다.
pub struct FakeOllama {
    server: MockServer,
    embed_delay: Arc<Mutex<Duration>>,
    tags_delay: Arc<Mutex<Duration>>,
}

impl FakeOllama {
    pub async fn start() -> FakeOllama {
        let server = MockServer::start().await;
        let embed_delay = Arc::new(Mutex::new(Duration::ZERO));
        let tags_delay = Arc::new(Mutex::new(Duration::ZERO));
        let delay = Arc::clone(&embed_delay);
        Mock::given(method("POST"))
            .and(path("/api/embed"))
            .respond_with(move |request: &Request| {
                let body: Value = serde_json::from_slice(&request.body).unwrap_or(Value::Null);
                let count = body["input"].as_array().map_or(1, Vec::len);
                let vectors: Vec<Vec<f32>> = (0..count)
                    .map(|n| {
                        let mut vector = vec![0.0_f32; EMBED_DIMS];
                        vector[n % EMBED_DIMS] = 1.0;
                        vector
                    })
                    .collect();
                ResponseTemplate::new(200)
                    .set_body_json(json!({
                        "model": body["model"], "embeddings": vectors,
                        "total_duration": 1_000, "load_duration": 100, "prompt_eval_count": count
                    }))
                    .set_delay(*delay.lock().unwrap())
            })
            .mount(&server)
            .await;
        let delay = Arc::clone(&tags_delay);
        Mock::given(method("GET"))
            .and(path("/api/tags"))
            .respond_with(move |_request: &Request| {
                ResponseTemplate::new(200)
                    .set_body_json(json!({"models": [
                        {"name": "bge-m3:latest", "model": "bge-m3:latest", "size": 1, "digest": "siheom"}
                    ]}))
                    .set_delay(*delay.lock().unwrap())
            })
            .mount(&server)
            .await;
        FakeOllama {
            server,
            embed_delay,
            tags_delay,
        }
    }

    pub fn uri(&self) -> String {
        self.server.uri()
    }

    /// 임베딩 답을 이만큼 늦춘다.
    pub fn slow_embeds(&self, delay: Duration) {
        *self.embed_delay.lock().unwrap() = delay;
    }

    /// 모델 목록 답을 이만큼 늦춘다.
    pub fn slow_tags(&self, delay: Duration) {
        *self.tags_delay.lock().unwrap() = delay;
    }

    /// 지금까지 받은 임베딩 요청 수(`POST /api/embed`).
    pub async fn embeds(&self) -> usize {
        let found = self.server.received_requests().await.unwrap_or_default();
        found
            .iter()
            .filter(|r| r.url.path() == "/api/embed")
            .count()
    }
}

// ───────────── (round64) 진단 ─────────────

/// 아무 명령도 부르지 않는 시험용 실행기(서비스는 등록하지 않은 것으로 보인다).
struct NoCommands;

impl CommandRunner for NoCommands {
    fn run(&self, _command: &CommandLine) -> std::io::Result<CommandOutput> {
        Ok(CommandOutput {
            success: false,
            stdout: String::new(),
            stderr: String::new(),
        })
    }
}

/// 그 보관 폴더를 진단해 Ollama 항목과 걸린 시간을 돌려준다(진단은 막는 함수라 따로 돈다).
pub async fn ollama_check(home: &Path) -> (Check, Duration) {
    let home: PathBuf = home.to_path_buf();
    tokio::task::spawn_blocking(move || {
        let defs = tempfile::tempdir().unwrap();
        let runner = NoCommands;
        let setup = ServiceSetup {
            os: Os::MacOs,
            exe: PathBuf::from("/opt/price-inha/bin/price-inha"),
            home: home.clone(),
            definition_dir: defs.path().to_path_buf(),
            uid: 501,
            path_env: "/usr/bin:/bin".to_string(),
            run_args: Vec::new(),
        };
        let service = Service::new(setup, &runner);
        let args: Vec<String> = ["--home", home.to_str().unwrap(), "--port", "0"]
            .iter()
            .map(|s| (*s).to_string())
            .collect();
        let settings = Settings::parse(&args, &no_env, None).expect("설정을 읽어야 한다");
        let started = std::time::Instant::now();
        let report = doctor(&settings, &service);
        let check = report
            .check(CheckItem::Ollama)
            .cloned()
            .expect("진단에 Ollama 항목이 있다");
        (check, started.elapsed())
    })
    .await
    .unwrap()
}
