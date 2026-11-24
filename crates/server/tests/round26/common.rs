//! round26 테스트가 함께 쓰는 도우미: 시계 · 날짜 · "실제" 주소 자리의 로컬 가짜 서버 · 실행 파일 조립으로
//! 띄우기 · 이벤트 읽기 · HTTP. round25의 공용 장치를 본떠 두고(그 폴더는 고치지 않는다) 24차에 맞게 줄였다.
//!
//! 서버는 임시 보관 폴더 · `127.0.0.1:0`으로 띄운다. 조립은 실행 파일과 같은 [`run_assembly`]이고, 시험은
//! 시계([`TestClock`]) · 바깥 주소(모두 로컬 가짜 서버) · CLI 찾는 자리(비움) · 가짜 공시 간격만 바꾼다.
//! 가짜 서버는 무엇을 물어도 503(점검 중)으로 답하고 받은 요청을 센다.

use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use chrono::{DateTime, FixedOffset, NaiveDate, TimeZone, Utc};
use pi_ai::AnthropicConfig;
use pi_data::{DataConfig, NAVER_NEWS_KEYS, NaverNewsConfig};
use pi_market::{Clock, Symbol, TossConfig};
use pi_secrets::{
    Credentials, KeyKind, KeySlot, ProviderKeys, SecretStore, Verifier, VerifyFailure,
};
use pi_store::{RoomEvent, Store};
use price_inha::{
    App, Crew, DayPlan, RunAssembly, Settings, TEMPLATE_ADJUST_PATH, TEMPLATE_PATH, Upstreams,
    run_assembly, save_watchlist,
};
use serde_json::{Value, json};
use tempfile::TempDir;
use wiremock::matchers::any;
use wiremock::{Mock, MockServer, ResponseTemplate};

/// 한 걸음을 기다리는 가장 긴 시간.
pub const STEP: Duration = Duration::from_secs(15);
/// 일어나면 안 되는 일을 지켜보는 시간.
pub const QUIET: Duration = Duration::from_millis(600);
/// 지켜볼 종목(보관 폴더에 저장한다).
pub const WATCHED: &str = "005930";
/// 깨우기 엔진의 장 시작 작업 id(첫 tick이 성공하면 저장소에 상태가 남는다).
pub const OPEN_JOB: &str = "team-open";

/// 유닉스 밀리초 칸을 시각으로 읽는다.
fn now_millis(cell: &AtomicI64) -> DateTime<Utc> {
    Utc.timestamp_millis_opt(cell.load(Ordering::SeqCst))
        .unwrap()
}

// ───────────── 시계 · 날짜 ─────────────

/// 테스트가 정하는 시계. 자는 것은 짧게만 잔다(오래 자지 않는다).
pub struct TestClock(AtomicI64);

impl TestClock {
    fn at(time: DateTime<Utc>) -> Arc<Self> {
        Arc::new(Self(AtomicI64::new(time.timestamp_millis())))
    }

    fn set(&self, time: DateTime<Utc>) {
        self.0.store(time.timestamp_millis(), Ordering::SeqCst);
    }

    fn read(&self) -> DateTime<Utc> {
        now_millis(&self.0)
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

/// 2026-11-17 화요일.
pub fn tuesday() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 11, 17).unwrap()
}

/// 2026-11-21 토요일.
pub fn saturday() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 11, 21).unwrap()
}

/// 2026-11-22 일요일.
pub fn sunday() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 11, 22).unwrap()
}

/// 한국 시각(UTC+9).
pub fn seoul() -> FixedOffset {
    FixedOffset::east_opt(9 * 3600).unwrap()
}

/// 11월 뉴욕 시각(동부 표준시, UTC-5. 2026년 일광 절약 시간은 11월 1일에 끝났다).
pub fn new_york_november() -> FixedOffset {
    FixedOffset::west_opt(5 * 3600).unwrap()
}

/// 그 시간대 · 그 날의 시각.
pub fn at_zone(zone: FixedOffset, date: NaiveDate, hour: u32, minute: u32) -> DateTime<Utc> {
    zone.from_local_datetime(&date.and_hms_opt(hour, minute, 0).unwrap())
        .unwrap()
        .with_timezone(&Utc)
}

/// 그 날의 한국 시각.
pub fn kst(date: NaiveDate, hour: u32, minute: u32) -> DateTime<Utc> {
    at_zone(seoul(), date, hour, minute)
}

/// `seconds`초 뒤.
pub fn plus(at: DateTime<Utc>, seconds: i64) -> DateTime<Utc> {
    at + chrono::Duration::seconds(seconds)
}

// ───────────── "실제" 주소 자리의 가짜 서버 ─────────────

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
    pub broker: MockServer,
    pub ai: MockServer,
    pub data: MockServer,
    pub news: MockServer,
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

    /// 가짜 서버 하나가 받은 요청 수.
    pub async fn received(server: &MockServer) -> usize {
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
            ("DART", Outside::received(&self.data).await),
            ("네이버", Outside::received(&self.news).await),
        ]
    }
}

// ───────────── 보관 폴더 · 설정 · 조립 ─────────────

fn no_env(_name: &str) -> Option<String> {
    None
}

/// 설정: 임시 보관 폴더 · 빈 포트 · 짧은 시세 간격과 다시 시도 간격. 종목은 저장된 목록에서 읽는다.
pub fn settings(home: &Path, demo: bool) -> Settings {
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

/// 띄우기 전 보관 폴더에 모든 키(증권사 · AI · DART · ECOS · FRED · 네이버)를 등록하고 지켜볼 종목을 저장한다.
/// 값은 진짜 키처럼 보이지 않는다.
async fn prepare(home: &Path) {
    let secrets = SecretStore::open(home).unwrap();
    let broker = Credentials::new()
        .with("client_id", "test-client-id-round26")
        .with("client_secret", "test-client-secret-round26");
    secrets
        .register(KeyKind::Broker, broker, &AcceptAll)
        .await
        .unwrap();
    let ai = Credentials::new().with("api_key", "test-ai-key-round26-zz");
    secrets
        .register(KeyKind::AiApiKey, ai, &AcceptAll)
        .await
        .unwrap();
    let slots = [
        (KeySlot::Dart, "test-dart-key-round26-zz"),
        (KeySlot::Ecos, "test-ecos-key-round26-zz"),
        (KeySlot::Fred, "test-fred-key-round26-zz"),
    ];
    for (slot, value) in slots {
        let values = Credentials::new().with(slot.fields()[0], value);
        secrets
            .register_slot(slot, values, &AcceptAll)
            .await
            .unwrap();
    }
    let naver = Credentials::new()
        .with("client_id", "test-naver-id-round26")
        .with("client_secret", "test-naver-secret-round26");
    secrets
        .register_provider(NAVER_NEWS_KEYS.provider_keys(), naver, &AcceptAll)
        .await
        .unwrap();
    save_watchlist(home, &[Symbol::new(WATCHED)]).unwrap();
}

/// 실행 파일 조립에서 시계 · 바깥 주소 · CLI 찾는 자리 · 가짜 공시 간격만 바꾼다.
/// `every`가 있으면 조립에 이미 있는 가짜 공시 간격만 그 값으로 줄인다(없던 것을 켜지 않는다).
fn bend(
    mut assembly: RunAssembly,
    clock: &Arc<TestClock>,
    outside: &Outside,
    every: Option<Duration>,
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
    if let Some(short) = every {
        assembly.demo.disclosure_every = assembly.demo.disclosure_every.map(|_| short);
    }
    assembly
}

/// 띄울 모양: 시연인가 · 가짜 공시 간격(줄일 값).
pub struct Plan {
    demo: bool,
    every: Option<Duration>,
}

impl Plan {
    /// 실행 파일의 `--demo` 조립. 가짜 공시 간격은 조립 그대로(기본 3분).
    pub fn demo() -> Plan {
        Plan {
            demo: true,
            every: None,
        }
    }

    /// 실행 파일의 실제(시연 아님) 조립.
    pub fn live() -> Plan {
        Plan {
            demo: false,
            every: None,
        }
    }

    /// 가짜 공시 간격을 줄인다.
    pub fn every(self, short: Duration) -> Plan {
        Plan {
            every: Some(short),
            ..self
        }
    }

    /// 이 모양으로 그 시각에 띄운다.
    pub async fn start(self, at: DateTime<Utc>) -> Bench {
        let dir = tempfile::tempdir().unwrap();
        prepare(dir.path()).await;
        let clock = TestClock::at(at);
        let outside = Outside::start().await;
        let settings = settings(dir.path(), self.demo);
        let assembly = bend(run_assembly(&settings, None), &clock, &outside, self.every);
        let app = tokio::time::timeout(STEP, App::launch_run(settings, assembly))
            .await
            .expect("서버가 시간 안에 떠야 한다")
            .expect("서버가 떠야 한다");
        Bench {
            _dir: dir,
            clock,
            app,
            outside,
        }
    }
}

/// 떠 있는 서버 하나와 그 보관 폴더 · 시계 · 가짜 서버들.
pub struct Bench {
    _dir: TempDir,
    clock: Arc<TestClock>,
    pub app: App,
    pub outside: Outside,
}

impl Bench {
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

    pub fn crew(&self) -> Arc<Crew> {
        self.app.crew().expect("실행 파일 조립은 팀을 잇는다")
    }

    pub fn addr(&self) -> SocketAddr {
        self.app.local_addr()
    }

    pub fn set(&self, at: DateTime<Utc>) {
        self.clock.set(at);
    }

    pub fn now(&self) -> DateTime<Utc> {
        self.clock.read()
    }

    /// 시연용 가짜 AI가 받은 글들.
    pub fn prompts(&self) -> Vec<String> {
        let ai = self.crew().demo_ai().expect("시연 모드다");
        ai.prompts()
    }

    /// 매매하기 좋게 만든다: 스윙 템플릿을 고르고 종목 비중 10%로 맞춘 뒤 오늘 매매하기로 한다
    /// (논의는 고른 템플릿이 있어야 끝까지 돈다).
    pub async fn ready(&self) {
        let reply = post(self.addr(), TEMPLATE_PATH, &json!({"style": "swing"})).await;
        assert_eq!(reply.status, 200, "템플릿을 골라야 한다: {}", reply.body);
        let id = reply.body["id"].as_i64().expect("템플릿 번호");
        let body = json!({"id": id, "changes": {"symbol_weight_pct": "10"}});
        let reply = post(self.addr(), TEMPLATE_ADJUST_PATH, &body).await;
        assert_eq!(reply.status, 200, "상한 안의 값은 저장된다: {}", reply.body);
        // 이미 하루 판단으로 매매하기로 했으면 그대로다(오류만 아니면 된다).
        self.app
            .trading()
            .decide_day(DayPlan::Trade)
            .expect("매매하기로 정할 수 있다");
    }

    /// 깨우기 엔진 작업 상태가 저장된 작업 id들(첫 tick이 성공해야 남는다).
    pub fn trigger_jobs(&self) -> Vec<String> {
        self.store()
            .trigger_jobs()
            .unwrap()
            .into_iter()
            .map(|job| job.job)
            .collect()
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

/// 저장된 이벤트 가운데 그 종류이고 그 날짜(`date` 칸)인 것의 수.
pub fn on_date(store: &Store, kind: &str, date: NaiveDate) -> usize {
    let date = json!(date.format("%Y-%m-%d").to_string());
    events(store, kind)
        .into_iter()
        .filter(|payload| payload["date"] == date)
        .count()
}

/// 그 방의 방 로그(번호 순서).
pub fn room(store: &Store, name: &str) -> Vec<RoomEvent> {
    store.room_events(name, 0, 10_000).unwrap()
}

// ───────────── HTTP ─────────────

fn origin_of(addr: SocketAddr) -> String {
    format!("http://127.0.0.1:{}", addr.port())
}

/// HTTP 응답 하나: 상태 · JSON 본문(JSON이 아니면 `Null`).
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

/// 세션을 받아 같은 출처에서 보낸다.
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
    let bytes = reply.bytes().await.expect("본문을 읽어야 한다");
    Reply {
        status,
        body: serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    }
}
