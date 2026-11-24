//! 33차 시험의 공용 도우미: 실행 파일 조립으로 띄우기(가짜 AI 뜸 · 하루 AI 상한 · 다른 기기로 보이기) · 시계 ·
//! 바깥 주소 자리의 로컬 가짜 서버 · 세션 · 요청 · 이벤트 읽기.
//! round27 · round34의 도우미를 본떴다(그 폴더들은 고치지 않는다). 바깥 주소 자리는 모두 로컬 가짜 서버다.
//!
//! 시간(곁에서 10초 · 도착 없음 30초 · 보내기 간격 2초)은 넣어 주는 시계([`TestClock`])를 옮겨 잰다 — 실제로
//! 10초를 자지 않는다. 서버는 [`price_inha::OFFICE_SUMMON_TICK`]마다 그 시계로 때가 됐는지 본다.

use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use chrono::{DateTime, FixedOffset, NaiveDate, TimeZone, Utc};
use pi_ai::AnthropicConfig;
use pi_data::{DataConfig, NAVER_NEWS_KEYS, NaverNewsConfig};
use pi_market::{Clock, Symbol, TossConfig};
use pi_secrets::{
    Credentials, KeyKind, KeySlot, ProviderKeys, SecretStore, Verifier, VerifyFailure,
};
use pi_store::Store;
use price_inha::{
    AI_DAILY_LIMIT_KEY, App, Crew, DayPlan, DemoAi, DemoTurn, KIND_OFFICE_SUMMON, RunAssembly,
    SETUP_AI_PATH, Settings, TEMPLATE_ADJUST_PATH, TEMPLATE_PATH, Upstreams, run_assembly,
    save_watchlist,
};
use serde_json::{Value, json};
use tempfile::TempDir;
use wiremock::matchers::any;
use wiremock::{Mock, MockServer, ResponseTemplate};

/// 한 걸음을 기다리는 가장 긴 시간.
pub const STEP: Duration = Duration::from_secs(15);
/// 일어나면 안 되는 일을 지켜보는 시간(서버 시계 굴리기 간격보다 넉넉히 길다).
pub const QUIET: Duration = Duration::from_millis(900);
/// 지켜볼 종목(국내, 보관 폴더에 저장한다).
pub const WATCHED: &str = "005930";
/// 보관 폴더에 등록하는 DART 키(진짜 키처럼 보이지 않는다). 가리기를 보는 데 쓴다.
pub const DART_KEY: &str = "test-dart-key-round35-zz";
/// 다른 기기로 보일 때의 상대 주소(사설 대역, 실제로 닿지 않는다).
const REMOTE_PEER: [u8; 4] = [100, 64, 0, 35];

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

/// 2026-11-21 토요일(시연 달력은 매일을 거래일로 본다).
pub fn saturday() -> NaiveDate {
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

/// 시험의 기본 시각: 토요일 10:00(한국).
pub fn morning() -> DateTime<Utc> {
    kst(saturday(), 10, 0)
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
        .with("client_id", "test-client-id-round35")
        .with("client_secret", "test-client-secret-round35");
    secrets
        .register(KeyKind::Broker, broker, &AcceptAll)
        .await
        .unwrap();
    let ai = Credentials::new().with("api_key", "test-ai-key-round35-zz");
    secrets
        .register(KeyKind::AiApiKey, ai, &AcceptAll)
        .await
        .unwrap();
    let slots = [
        (KeySlot::Dart, DART_KEY),
        (KeySlot::Ecos, "test-ecos-key-round35-zz"),
        (KeySlot::Fred, "test-fred-key-round35-zz"),
    ];
    for (slot, value) in slots {
        let values = Credentials::new().with(slot.fields()[0], value);
        secrets
            .register_slot(slot, values, &AcceptAll)
            .await
            .unwrap();
    }
    let naver = Credentials::new()
        .with("client_id", "test-naver-id-round35")
        .with("client_secret", "test-naver-secret-round35");
    secrets
        .register_provider(NAVER_NEWS_KEYS.provider_keys(), naver, &AcceptAll)
        .await
        .unwrap();
    save_watchlist(home, &[Symbol::new(WATCHED)]).unwrap();
}

/// 실행 파일 조립에서 시계 · 바깥 주소 · CLI 찾는 자리 · 가짜 AI 뜸 · 상대 주소만 바꾼다.
fn bend(
    mut assembly: RunAssembly,
    clock: &Arc<TestClock>,
    outside: &Outside,
    pause: Duration,
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
    assembly.demo_ai_pause = pause;
    assembly
}

async fn launch(
    home: &Path,
    clock: &Arc<TestClock>,
    outside: &Outside,
    pause: Duration,
    remote: &Arc<AtomicBool>,
) -> App {
    let settings = settings(home);
    let assembly = bend(run_assembly(&settings, None), clock, outside, pause, remote);
    tokio::time::timeout(STEP, App::launch_run(settings, assembly))
        .await
        .expect("서버가 시간 안에 떠야 한다")
        .expect("서버가 떠야 한다")
}

/// 띄울 모양: 실행 파일의 `--demo` 조립 · 가짜 AI 뜸(기본 0) · 하루 AI 상한(기본 없음).
pub struct Plan {
    pause: Duration,
    ai_limit: Option<u32>,
}

impl Plan {
    /// 실행 파일의 `--demo` 조립. 가짜 AI 뜸은 0, 하루 AI 상한은 없다.
    pub fn demo() -> Plan {
        Plan {
            pause: Duration::ZERO,
            ai_limit: None,
        }
    }

    /// 가짜 AI 답마다 그 뜸을 둔다(논의가 그만큼 오래 열려 있다).
    pub fn pause(self, pause: Duration) -> Plan {
        Plan { pause, ..self }
    }

    /// 하루 AI 호출 상한을 설정 저장 길로 정하고 다시 켠다(상한은 켤 때 팀에 닿는다).
    pub fn ai_limit(self, limit: u32) -> Plan {
        Plan {
            ai_limit: Some(limit),
            ..self
        }
    }

    /// 이 모양으로 그 시각에 띄운다.
    pub async fn start(self, at: DateTime<Utc>) -> Bench {
        let dir = tempfile::tempdir().unwrap();
        prepare(dir.path()).await;
        let clock = TestClock::at(at, self.pause);
        let outside = Outside::start().await;
        let remote = Arc::new(AtomicBool::new(false));
        let app = launch(dir.path(), &clock, &outside, self.pause, &remote).await;
        let mut bench = Bench {
            dir,
            outside,
            clock,
            remote,
            pause: self.pause,
            app,
        };
        if let Some(limit) = self.ai_limit {
            let cookie = session(bench.addr()).await;
            let reply = post(
                bench.addr(),
                SETUP_AI_PATH,
                &cookie,
                &json!({ AI_DAILY_LIMIT_KEY: limit }),
            )
            .await;
            assert_eq!(reply.status, 200, "상한을 정해야 한다: {}", reply.text);
            bench = bench.reopen().await;
        }
        bench
    }
}

/// 떠 있는 서버 하나와 그 보관 폴더 · 시계 · 가짜 서버들.
pub struct Bench {
    dir: TempDir,
    outside: Outside,
    clock: Arc<TestClock>,
    remote: Arc<AtomicBool>,
    pause: Duration,
    pub app: App,
}

impl Bench {
    /// 기본 모양(뜸 없음 · 상한 없음)으로 토요일 10:00에 띄운다.
    pub async fn start() -> Bench {
        Plan::demo().start(morning()).await
    }

    /// 끈다.
    pub async fn close(self) {
        tokio::time::timeout(STEP, self.app.shutdown())
            .await
            .expect("시간 안에 꺼져야 한다")
            .expect("끄기가 성공해야 한다");
    }

    /// 끄고 같은 보관 폴더 · 시계 · 가짜 서버로 다시 켠다.
    pub async fn reopen(self) -> Bench {
        let Bench {
            dir,
            outside,
            clock,
            remote,
            pause,
            app,
        } = self;
        tokio::time::timeout(STEP, app.shutdown())
            .await
            .expect("시간 안에 꺼져야 한다")
            .expect("끄기가 성공해야 한다");
        let app = launch(dir.path(), &clock, &outside, pause, &remote).await;
        Bench {
            dir,
            outside,
            clock,
            remote,
            pause,
            app,
        }
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

    /// 시연용 가짜 AI.
    pub fn demo_ai(&self) -> Arc<DemoAi> {
        self.crew().demo_ai().expect("시연 모드다")
    }

    /// 가짜 AI의 답을 정한다.
    pub fn script(&self, turn: DemoTurn, reply: &str) {
        self.demo_ai().script(turn, reply);
    }

    /// 가짜 AI가 받은 글들(받은 순서).
    pub fn prompts(&self) -> Vec<String> {
        self.demo_ai().prompts()
    }

    /// 가짜 AI가 받은 글 가운데 `needle`이 든 것의 수.
    pub fn prompts_with(&self, needle: &str) -> usize {
        self.prompts().iter().filter(|p| p.contains(needle)).count()
    }

    pub fn now(&self) -> DateTime<Utc> {
        self.clock.read()
    }

    pub fn set(&self, at: DateTime<Utc>) {
        self.clock.set(at);
    }

    /// 시계를 `seconds`초 옮긴다(실제로 자지 않는다).
    pub fn pass(&self, seconds: i64) {
        self.set(self.now() + chrono::Duration::seconds(seconds));
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

    /// 증권사 주소 자리의 가짜 서버가 받은 요청 가운데 주문 경로로 간 것의 수.
    pub async fn outside_order_requests(&self) -> usize {
        self.outside
            .broker
            .received_requests()
            .await
            .unwrap_or_default()
            .iter()
            .filter(|r| r.url.path().contains("order"))
            .count()
    }

    /// 매매하기 좋게 만든다: 스윙 템플릿을 고르고 종목 비중 10%로 맞춘 뒤 오늘 매매하기로 한다(운용 중).
    pub async fn ready(&self) {
        let cookie = session(self.addr()).await;
        let reply = post(
            self.addr(),
            TEMPLATE_PATH,
            &cookie,
            &json!({"style": "swing"}),
        )
        .await;
        assert_eq!(reply.status, 200, "템플릿을 골라야 한다: {}", reply.text);
        let id = reply.body["id"].as_i64().expect("템플릿 번호");
        let body = json!({"id": id, "changes": {"symbol_weight_pct": "10"}});
        let reply = post(self.addr(), TEMPLATE_ADJUST_PATH, &cookie, &body).await;
        assert_eq!(reply.status, 200, "상한 안의 값은 저장된다: {}", reply.text);
        self.app
            .trading()
            .decide_day(DayPlan::Trade)
            .expect("매매하기로 정할 수 있다");
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

/// 그 담당의 부르기 단계 이벤트([`KIND_OFFICE_SUMMON`], 번호 순서).
pub fn summons(store: &Store, agent: &str) -> Vec<Value> {
    events(store, KIND_OFFICE_SUMMON)
        .into_iter()
        .filter(|e| e["agent"] == agent)
        .collect()
}

/// 그 담당의 부르기 단계 이벤트 가운데 그 단계인 것이 있는가.
pub fn has_phase(store: &Store, agent: &str, phase: &str) -> bool {
    summons(store, agent).iter().any(|e| e["phase"] == phase)
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

pub fn origin_of(addr: SocketAddr) -> String {
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

/// `POST` JSON 글자 `body`. `cookie` · `origin`이 `None`이면 그 머리를 붙이지 않는다.
pub async fn post_raw(
    addr: SocketAddr,
    path: &str,
    cookie: Option<&str>,
    origin: Option<&str>,
    body: &str,
) -> Reply {
    let mut request = reqwest::Client::new()
        .post(format!("{}{path}", origin_of(addr)))
        .header("Content-Type", "application/json")
        .body(body.to_owned());
    if let Some(cookie) = cookie {
        request = request.header("Cookie", cookie);
    }
    if let Some(origin) = origin {
        request = request.header("Origin", origin);
    }
    send(request).await
}

/// 같은 출처 · 그 세션으로 JSON을 `POST`한다.
pub async fn post(addr: SocketAddr, path: &str, cookie: &str, body: &Value) -> Reply {
    let origin = origin_of(addr);
    post_raw(addr, path, Some(cookie), Some(&origin), &body.to_string()).await
}

/// 팀 방 멘션 글의 몸통(`agent` 없음).
pub fn team_text(text: &str, source: &str) -> Value {
    json!({"room": "team", "text": text, "sourceMessageId": source})
}

/// 공지 몸통.
pub fn notice_body(text: &str, source: &str) -> Value {
    json!({"text": text, "sourceMessageId": source})
}
