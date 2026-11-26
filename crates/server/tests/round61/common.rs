//! 61차 시험의 공용 도우미: 옛 기록 심기 · 실행 파일의 `--demo` 조립으로 띄우기 · 시계 · 바깥 주소 자리의 로컬 가짜
//! 서버 · 세션 · 요청. round52의 도우미를 본떴다(그 폴더는 고치지 않는다).

use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, FixedOffset, NaiveDate, TimeZone, Utc};
use pi_ai::AnthropicConfig;
use pi_data::{DataConfig, NaverNewsConfig};
use pi_market::{Clock, Symbol, TossConfig};
use pi_secrets::{
    Credentials, KeyKind, KeySlot, ProviderKeys, SecretStore, Verifier, VerifyFailure,
};
use pi_store::{NewRoomEvent, RoomActorKind, Store, UnixMillis};
use price_inha::{App, DB_FILE, RunAssembly, Settings, Upstreams, run_assembly, save_watchlist};
use serde_json::{Value, json};
use tempfile::TempDir;
use wiremock::matchers::any;
use wiremock::{Mock, MockServer, ResponseTemplate};

/// 한 걸음을 기다리는 가장 긴 시간.
pub const STEP: Duration = Duration::from_secs(20);
/// 하루(밀리초).
pub const DAY: i64 = 86_400_000;
/// 심는 담당들.
pub const AGENTS: [&str; 3] = ["scout", "analyst", "lead"];

// ───────────── 시계 · 날짜 ─────────────

/// 시험이 정하는 시계. 자는 것은 짧게만 잔다.
pub struct TestClock(AtomicI64);

impl TestClock {
    fn at(time: DateTime<Utc>) -> Arc<Self> {
        Arc::new(Self(AtomicI64::new(time.timestamp_millis())))
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

/// 그 날의 한국 시각(UTC+9).
pub fn kst(date: NaiveDate, hour: u32, minute: u32) -> DateTime<Utc> {
    FixedOffset::east_opt(9 * 3600)
        .unwrap()
        .from_local_datetime(&date.and_hms_opt(hour, minute, 0).unwrap())
        .unwrap()
        .with_timezone(&Utc)
}

/// 시험의 "오늘": 2026-11-20 금요일.
pub fn today() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 11, 20).unwrap()
}

/// 오늘 장 마감 뒤(17:00 한국).
pub fn after_close() -> DateTime<Utc> {
    kst(today(), 17, 0)
}

/// 오늘 장중(10:00 한국).
pub fn in_session() -> DateTime<Utc> {
    kst(today(), 10, 0)
}

/// `now`에서 `days`일 앞의 시각(밀리초).
pub fn days_before(now: DateTime<Utc>, days: i64) -> UnixMillis {
    UnixMillis(now.timestamp_millis() - days * DAY)
}

// ───────────── 옛 기록 심기 ─────────────

/// 심은 기록의 수.
pub struct Seeded {
    /// 보관 기간이 지난 담당 글 수(방 `team`).
    pub old_member: usize,
    /// 보관 기간이 지난 진행 · 걷기 이벤트 수.
    pub old_screen: usize,
    /// 보관 기간이 지난 AI 사용 세부 수.
    pub old_ai: usize,
}

/// `now` 기준 120~100일 앞의 옛 기록과 최근 기록을 심는다. 감사 기록(`rules_config` · `journal.review`)도 옛 날에 둔다.
pub fn seed(store: &Store, now: DateTime<Utc>) -> Seeded {
    let mut old_member = 0;
    for i in 0..30_i64 {
        let agent = AGENTS[(i % 3) as usize];
        let at = UnixMillis(days_before(now, 120).0 + i * (DAY / 2));
        room(
            store,
            &format!("old-{i}"),
            "message.member",
            agent,
            at,
            i % 5,
        );
        old_member += 1;
    }
    for i in 0..3_i64 {
        let at = UnixMillis(days_before(now, 118).0 + i * 1000);
        room(store, &format!("user-{i}"), "message.user", "me", at, 0);
    }
    for i in 0..4_i64 {
        let at = UnixMillis(days_before(now, 3).0 + i * 1000);
        room(store, &format!("new-{i}"), "message.member", "scout", at, 0);
    }
    let mut old_screen = 0;
    let mut old_ai = 0;
    for i in 0..20_i64 {
        let at = UnixMillis(days_before(now, 115).0 + i * (DAY / 4));
        let agent = AGENTS[(i % 3) as usize];
        store
            .append_at(
                "office.progress",
                &json!({"agent": agent, "trade": 7, "step": i}),
                at,
            )
            .unwrap();
        store
            .append_at(
                "office.agent",
                &json!({"agent": agent, "place": "desk"}),
                at,
            )
            .unwrap();
        old_screen += 2;
        if i % 4 == 0 {
            store
                .append_at("team.ai_used", &json!({"agent": agent, "calls": 1}), at)
                .unwrap();
            old_ai += 1;
        }
    }
    for i in 0..3_i64 {
        let at = UnixMillis(days_before(now, 112).0 + i * 1000);
        store
            .append_at("rules_config", &json!({"changed": i}), at)
            .unwrap();
        store
            .append_at("journal.review", &json!({"trade": i}), at)
            .unwrap();
    }
    for i in 0..3_i64 {
        let at = UnixMillis(days_before(now, 2).0 + i * 1000);
        store
            .append_at("office.progress", &json!({"agent": "scout", "step": i}), at)
            .unwrap();
    }
    Seeded {
        old_member,
        old_screen,
        old_ai,
    }
}

fn room(store: &Store, id: &str, kind: &str, actor: &str, at: UnixMillis, trade: i64) {
    let actor_kind = if kind == "message.user" {
        RoomActorKind::User
    } else {
        RoomActorKind::Member
    };
    store
        .append_room_event(NewRoomEvent {
            room: "team".into(),
            event_id: id.into(),
            kind: kind.into(),
            actor_kind,
            actor_id: actor.into(),
            payload: json!({"text": format!("글 {id}"), "trade": trade}),
            at,
        })
        .unwrap();
}

/// SQL 한 값(정수).
pub fn count(path: &Path, sql: &str) -> i64 {
    let conn = rusqlite::Connection::open(path).unwrap();
    conn.query_row(sql, [], |row| row.get(0)).unwrap()
}

// ───────────── "실제" 주소 자리의 가짜 서버 ─────────────

async fn refusing_server() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(any())
        .respond_with(ResponseTemplate::new(503))
        .mount(&server)
        .await;
    server
}

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

/// 시연 모드 설정. `extra`는 더 줄 인자.
pub fn settings(home: &Path, extra: &[&str]) -> Settings {
    let home = home.to_str().unwrap();
    let mut args: Vec<String> = [
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
    ]
    .iter()
    .map(|s| (*s).to_string())
    .collect();
    args.extend(extra.iter().map(|s| (*s).to_string()));
    Settings::parse(&args, &no_env, None).expect("설정을 읽어야 한다")
}

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

async fn prepare(home: &Path) {
    let secrets = SecretStore::open(home).unwrap();
    let broker = Credentials::new()
        .with("client_id", "test-client-id-round61")
        .with("client_secret", "test-client-secret-round61");
    secrets
        .register(KeyKind::Broker, broker, &AcceptAll)
        .await
        .unwrap();
    let ai = Credentials::new().with("api_key", "test-ai-key-round61-zz");
    secrets
        .register(KeyKind::AiApiKey, ai, &AcceptAll)
        .await
        .unwrap();
    save_watchlist(home, &[Symbol::new("005930")]).unwrap();
}

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

/// 떠 있는 서버 하나와 그 보관 폴더 · 가짜 서버들.
pub struct Bench {
    dir: TempDir,
    _outside: Outside,
    app: App,
}

impl Bench {
    /// `now`에 띄운다. 띄우기 전에 `before`로 저장소 파일에 기록을 심을 수 있다.
    pub async fn start(now: DateTime<Utc>, before: impl FnOnce(&Store)) -> Bench {
        let dir = tempfile::tempdir().unwrap();
        prepare(dir.path()).await;
        {
            let store = Store::open(dir.path().join(DB_FILE)).unwrap();
            before(&store);
        }
        let clock = TestClock::at(now);
        let outside = Outside::start().await;
        let settings = settings(dir.path(), &[]);
        let assembly = bend(run_assembly(&settings, None), &clock, &outside);
        let app = tokio::time::timeout(STEP, App::launch_run(settings, assembly))
            .await
            .expect("서버가 시간 안에 떠야 한다")
            .expect("서버가 떠야 한다");
        Bench {
            dir,
            _outside: outside,
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

    pub fn home(&self) -> &Path {
        self.dir.path()
    }

    pub fn store(&self) -> Store {
        self.app.store()
    }

    /// 새 세션으로 읽는다.
    pub async fn get(&self, path: &str) -> Reply {
        let cookie = session(self.app.local_addr()).await;
        get(self.app.local_addr(), path, Some(&cookie)).await
    }

    /// 세션 없이 읽는다.
    pub async fn get_without_session(&self, path: &str) -> Reply {
        get(self.app.local_addr(), path, None).await
    }
}

// ───────────── 요청 ─────────────

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

async fn get(addr: SocketAddr, path: &str, cookie: Option<&str>) -> Reply {
    let mut request = reqwest::Client::new().get(format!("{}{path}", origin_of(addr)));
    if let Some(cookie) = cookie {
        request = request.header("Cookie", cookie);
    }
    send(request).await
}
