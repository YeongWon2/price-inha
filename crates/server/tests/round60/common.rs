//! 팀 지침(기능 L · 결정 30) 시험의 공용 도우미: 실행 파일의 `--demo` 조립으로 띄우기 · 시계 · 바깥 주소 자리의
//! 로컬 가짜 서버 · 세션(원격 세션 포함) · 요청 · 이벤트 읽기 · 팔린 매매와 복기 넣기 · 지침 장부 파일 읽기.
//! round52의 도우미를 본떴다(그 폴더는 고치지 않는다). 바깥 주소 자리는 모두 로컬 가짜 서버다.
//!
//! 시계는 시험이 정한다. 시연 달력은 매일을 거래일로 본다. 복기는 저장소의 매매를 팔림까지 옮긴 뒤
//! `pi_journal::Journal::review_trade`로 직접 남긴다(시연 기록으로 실수를 넣는 길).

use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, FixedOffset, NaiveDate, TimeZone, Utc};
use pi_agent::Decision;
use pi_ai::AnthropicConfig;
use pi_data::{DataConfig, NaverNewsConfig};
use pi_journal::{Journal, Mistake};
use pi_market::{Clock, Currency, Quote, Symbol, TossConfig};
use pi_secrets::{
    Credentials, KeyKind, KeySlot, ProviderKeys, SecretStore, Verifier, VerifyFailure,
};
use pi_store::{
    AccessKeyHash, AccountKind, Amount, Judgment, NewTrade, Rationale, Store, TradeId, TradeState,
    UnixMillis,
};
use price_inha::{
    App, Crew, DayPlan, DemoAi, DemoTurn, GUIDELINES_PATH, RunAssembly, Settings,
    TEAM_GUIDELINES_FILE, TEMPLATE_ADJUST_PATH, TEMPLATE_PATH, Upstreams, run_assembly,
    save_watchlist,
};
use rust_decimal::Decimal;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tempfile::TempDir;
use wiremock::matchers::any;
use wiremock::{Mock, MockServer, ResponseTemplate};

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

/// 팀장의 사기 판단(가짜 AI 답).
pub fn buy_verdict() -> String {
    json!({
        "decision": "buy", "hypothesis": "시험 가설 실적 개선", "trigger": "시험 계기",
        "entry": 100_000, "stop": 95_000, "target": 110_000, "expected_value": 5_000
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
        .with("client_id", "test-client-id-round60")
        .with("client_secret", "test-client-secret-round60");
    secrets
        .register(KeyKind::Broker, broker, &AcceptAll)
        .await
        .unwrap();
    let ai = Credentials::new().with("api_key", "test-ai-key-round60-zz");
    secrets
        .register(KeyKind::AiApiKey, ai, &AcceptAll)
        .await
        .unwrap();
    save_watchlist(home, &[Symbol::new(OTHER_SYMBOL)]).unwrap();
}

/// 실행 파일 조립에서 시계 · 바깥 주소 · CLI 찾는 자리 · 상대 주소만 바꾼다.
fn bend(
    mut assembly: RunAssembly,
    clock: &Arc<TestClock>,
    outside: &Outside,
    remote: &Arc<AtomicBool>,
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
    }
    assembly.demo_ai_pause = Duration::ZERO;
    assembly
}

async fn launch(
    home: &Path,
    clock: &Arc<TestClock>,
    outside: &Outside,
    remote: &Arc<AtomicBool>,
) -> App {
    let settings = settings(home);
    let assembly = bend(run_assembly(&settings, None), clock, outside, remote);
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
        let dir = tempfile::tempdir().unwrap();
        prepare(dir.path()).await;
        let clock = TestClock::at(kst(decision_day(), 10, 0));
        let outside = Outside::start().await;
        let remote = Arc::new(AtomicBool::new(false));
        let app = launch(dir.path(), &clock, &outside, &remote).await;
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

    fn demo_ai(&self) -> Arc<DemoAi> {
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

    pub fn now(&self) -> DateTime<Utc> {
        self.clock.read()
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

    /// 보관 폴더.
    pub fn home(&self) -> &Path {
        self.dir.path()
    }

    /// 지침 장부 파일의 JSON. 없으면 `Null`.
    pub fn ledger(&self) -> Value {
        std::fs::read(self.home().join(TEAM_GUIDELINES_FILE))
            .map(|bytes| serde_json::from_slice(&bytes).expect("장부는 JSON이다"))
            .unwrap_or(Value::Null)
    }

    /// 지침 목록(`GET` [`GUIDELINES_PATH`]).
    pub async fn guidelines(&self) -> Value {
        let reply = self.get(GUIDELINES_PATH).await;
        assert_eq!(reply.status, 200, "{}", reply.text);
        reply.body
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
                        symbol: MEMO_SYMBOL.to_string(),
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
}

/// 다른 기기의 접속 키(시험용, 글자 수 · 글자 모음은 `pi_gate`의 것).
const ACCESS_KEY: &str = "0123456789ABCDEFGHJK";

/// 근거가 다 찬 판단(사기).
fn judgment(at: UnixMillis) -> Judgment {
    Judgment {
        rationale: Rationale {
            hypothesis: Some("시험 가설".into()),
            trigger: Some("시험 계기".into()),
            entry: Some(Amount(100_000)),
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

/// 응답의 세션 쿠키(`Cookie` 헤더 글자).
fn cookie_of(reply: &reqwest::Response) -> String {
    reply
        .headers()
        .get("set-cookie")
        .and_then(|v| v.to_str().ok())
        .map(|raw| raw.split(';').next().unwrap().trim().to_string())
        .expect("세션 쿠키가 있어야 한다")
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
