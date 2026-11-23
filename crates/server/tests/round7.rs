//! 7차 작업 1~14의 테스트(`docs/dev/modules/gate.md` "7차 작업").
//!
//! 잠긴 `server.rs`와 그 아래 폴더를 고치지 않으려고 따로 떼어 낸 최상위 테스트 파일이다.
//! 도우미는 이 파일 안에 둔다. 서버는 임시 보관 폴더로 `127.0.0.1:0`에 띄우고, 증권사 · AI는
//! 로컬 가짜 서버(wiremock)로, Claude Code CLI는 임시 폴더에 쓴 가짜 셸 스크립트로 대신한다.
//! 실제 `claude`, 토스증권, Anthropic을 부르지 않고 사용자의 `~/.claude`를 열지 않는다.
//!
//! - 1번(`valid_event_kind`의 앞뒤 공백 동작)은 `pi-gate`의 문서 주석 doc test로 고정한다.
//! - 2번(저장한 줄을 돌려주는 저장소 API)은 `pi-store`에 그런 API가 없어 테스트하지 않는다.
//! - 9번(`spawn_blocking`)은 바깥에서 볼 수 있는 동작이 없어 테스트하지 않는다.
//! - 14번은 문서 주석뿐이다.

// 11차(Vite 이전): 옛 시험이 빌드한 화면(dist)에서 같은 뜻을 찾는 도우미.
#[path = "support/screen.rs"]
mod screen;

use std::net::SocketAddr;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use chrono::{DateTime, TimeZone, Utc};
use futures::StreamExt;
use pi_ai::{AnthropicConfig, CompletionRequest, Role};
use pi_market::TossConfig;
use pi_secrets::{Credentials, KeyKind, SecretStore};
use price_inha::{
    AI_CHOICE_FILE, AiChoice, AiMethod, App, CLI_CHECKING, CLI_LOGGED_IN, CLI_WORK_DIR,
    CLI_WORK_NOT_EMPTY, DB_FILE, ITEM_AI, ITEM_SYMBOLS, KIND_QUOTES, KIND_QUOTES_STALE,
    KeyVerifier, SETUP_AI_PATH, SETUP_PATH, SETUP_SYMBOLS_PATH, Settings, Upstreams,
    WATCHLIST_FILE, Wiring, find_claude, load_watchlist, prepare_cli_work_dir, save_ai_choice,
};
use serde_json::{Value, json};
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

// ───────────── 도우미 ─────────────

/// 한 걸음을 기다리는 가장 긴 시간.
const STEP: Duration = Duration::from_secs(10);
/// "오지 않는다"를 확인할 때 기다리는 시간.
const QUIET: Duration = Duration::from_millis(1200);
/// 테스트에서 쓰는 시세 간격.
const INTERVAL_MS: &str = "20";
/// 시험용 자격. 진짜 키처럼 보이지 않는 글자다.
const CLIENT_ID: &str = "test-client-id-0007";
const CLIENT_SECRET: &str = "test-client-secret-not-real-7";
const AI_KEY: &str = "test-ai-key-not-real-0007";
const TOSS_TOKEN: &str = "test-toss-token-not-real-7";
/// 토스증권 경로(공식 명세, `docs/dev/toss-api-notes.md`).
const TOKEN_PATH: &str = "/oauth2/token";
const PRICES_PATH: &str = "/api/v1/prices";
const CALENDAR_KR_PATH: &str = "/api/v1/market-calendar/KR";
/// Anthropic 모델 목록 경로(키 확인).
const MODELS_PATH: &str = "/v1/models";
/// 가짜 CLI가 알리는 버전(쓸 수 있는 가장 낮은 버전보다 높다).
const CLI_VERSION_TEXT: &str = "2.1.316 (Claude Code)";

type Ws = WebSocketStream<MaybeTlsStream<TcpStream>>;

/// 테스트가 시각을 정하는 시계.
struct TestClock(AtomicI64);

impl TestClock {
    fn at(time: DateTime<Utc>) -> Arc<Self> {
        Arc::new(Self(AtomicI64::new(time.timestamp_millis())))
    }

    fn set(&self, time: DateTime<Utc>) {
        self.0.store(time.timestamp_millis(), Ordering::SeqCst);
    }
}

#[async_trait]
impl pi_market::Clock for TestClock {
    fn now(&self) -> DateTime<Utc> {
        Utc.timestamp_millis_opt(self.0.load(Ordering::SeqCst))
            .unwrap()
    }

    async fn sleep(&self, _duration: Duration) {
        tokio::task::yield_now().await;
    }
}

/// 2026-03-25(수) 한국 시각.
fn kst(hour: u32, minute: u32) -> DateTime<Utc> {
    let offset = chrono::FixedOffset::east_opt(9 * 3600).unwrap();
    offset
        .with_ymd_and_hms(2026, 3, 25, hour, minute, 0)
        .unwrap()
        .with_timezone(&Utc)
}

fn no_env(_name: &str) -> Option<String> {
    None
}

fn args(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| (*s).to_string()).collect()
}

/// 임시 보관 폴더 · 빈 포트 · 짧은 시세 간격의 설정. 지켜볼 종목은 주지 않는다(`extra`에 넣으면 준 것이다).
fn settings_without_symbols(home: &Path, extra: &[&str]) -> Settings {
    let home = home.to_str().unwrap();
    let mut list = vec!["--home", home, "--port", "0", "--interval-ms", INTERVAL_MS];
    list.extend_from_slice(extra);
    Settings::parse(&args(&list), &no_env, None).expect("설정을 읽어야 한다")
}

/// [`settings_without_symbols`]에 국내 종목 하나를 인자로 준다.
fn settings(home: &Path, extra: &[&str]) -> Settings {
    let mut list = vec!["--symbols", "005930"];
    list.extend_from_slice(extra);
    settings_without_symbols(home, &list)
}

/// 아무도 듣지 않는 로컬 주소(실제 바깥 주소로 나가지 않게).
fn dead_base_url() -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    format!("http://127.0.0.1:{port}")
}

/// 바깥 서버 설정. 주지 않은 쪽은 아무도 듣지 않는 로컬 주소다.
fn upstreams(toss: Option<&str>, anthropic: Option<&str>) -> Upstreams {
    let dead = dead_base_url();
    Upstreams {
        toss: TossConfig {
            base_url: toss.unwrap_or(&dead).to_string(),
            timeout: Duration::from_secs(3),
            token_failure_cooldown: Duration::ZERO,
            ..TossConfig::default()
        },
        anthropic: AnthropicConfig {
            base_url: anthropic.unwrap_or(&dead).to_string(),
            timeout: Duration::from_secs(3),
            max_retries: 0,
            retry_base_delay: Duration::from_millis(1),
        },
    }
}

/// 바깥은 모두 닿지 않는 주소이고, CLI를 스스로 찾는 자리는 비운 연결.
fn wiring(clock: Arc<TestClock>) -> Wiring {
    Wiring {
        upstreams: upstreams(None, None),
        clock,
        peer_override: None,
        cli_locations: Vec::new(),
    }
}

async fn start_app(settings: Settings, wiring: Wiring) -> App {
    tokio::time::timeout(STEP, App::start(settings, wiring))
        .await
        .expect("서버가 시간 안에 떠야 한다")
        .expect("서버가 떠야 한다")
}

async fn stop(app: App) {
    tokio::time::timeout(STEP, app.shutdown())
        .await
        .expect("시간 안에 꺼져야 한다")
        .expect("끄기가 성공해야 한다");
}

fn origin_of(addr: SocketAddr) -> String {
    format!("http://127.0.0.1:{}", addr.port())
}

/// HTTP 응답 하나(상태와 본문).
struct Reply {
    status: u16,
    text: String,
}

impl Reply {
    fn json(&self) -> Value {
        serde_json::from_str(&self.text)
            .unwrap_or_else(|_| panic!("응답 본문은 JSON이어야 한다(HTTP {})", self.status))
    }
}

async fn send(request: reqwest::RequestBuilder) -> Reply {
    let reply = tokio::time::timeout(STEP, request.send())
        .await
        .expect("응답이 시간 안에 와야 한다")
        .expect("요청이 닿아야 한다");
    let status = reply.status().as_u16();
    let text = reply.text().await.expect("본문을 읽어야 한다");
    Reply { status, text }
}

/// `POST`를 날것으로 보낸다. 쿠키와 출처는 준 것만 나간다.
async fn post_raw(
    addr: SocketAddr,
    path: &str,
    cookie: Option<&str>,
    origin: Option<&str>,
    body: String,
) -> Reply {
    let mut request = reqwest::Client::new()
        .post(format!("{}{path}", origin_of(addr)))
        .header("Content-Type", "application/json")
        .body(body);
    if let Some(cookie) = cookie {
        request = request.header("Cookie", cookie);
    }
    if let Some(origin) = origin {
        request = request.header("Origin", origin);
    }
    send(request).await
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

/// 세션을 받아 같은 출처에서 JSON을 보낸다.
async fn post_json(addr: SocketAddr, path: &str, body: &Value) -> Reply {
    let cookie = login(addr).await;
    let origin = origin_of(addr);
    post_raw(addr, path, Some(&cookie), Some(&origin), body.to_string()).await
}

/// 세션을 받아 설정 상태를 읽는다.
async fn setup_status(addr: SocketAddr) -> Value {
    let cookie = login(addr).await;
    let request = reqwest::Client::new()
        .get(format!("{}{SETUP_PATH}", origin_of(addr)))
        .header("Cookie", cookie);
    let reply = send(request).await;
    assert_eq!(reply.status, 200, "세션이 있으면 설정 상태를 읽는다");
    reply.json()
}

/// 설정 상태에서 그 항목이 등록됐는가.
fn registered(status: &Value, name: &str) -> bool {
    status["items"]
        .as_array()
        .expect("items는 목록이다")
        .iter()
        .find(|item| item["name"] == json!(name))
        .and_then(|item| item["registered"].as_bool())
        .unwrap_or_else(|| panic!("설정 상태에 {name} 항목이 있어야 한다: {status}"))
}

/// 설정 상태의 `ai.cli`가 `wanted`가 될 때까지 읽는다. 마지막으로 읽은 설정 상태를 돌려준다.
async fn wait_ai_cli(addr: SocketAddr, wanted: impl Fn(&Value) -> bool) -> Value {
    let deadline = Instant::now() + STEP;
    loop {
        let status = setup_status(addr).await;
        if wanted(&status["ai"]["cli"]) || Instant::now() >= deadline {
            return status;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// 세션을 받고 실시간 연결을 연다.
async fn connect(addr: SocketAddr, last_seq: u64) -> Ws {
    let cookie = login(addr).await;
    let url = format!(
        "ws://127.0.0.1:{}{}?{}={last_seq}",
        addr.port(),
        pi_gate::WS_PATH,
        pi_gate::LAST_SEQ_PARAM
    );
    let mut request = url.into_client_request().unwrap();
    let headers = request.headers_mut();
    headers.insert("Cookie", cookie.parse().unwrap());
    headers.insert("Origin", origin_of(addr).parse().unwrap());
    let (ws, _) = tokio::time::timeout(STEP, tokio_tungstenite::connect_async(request))
        .await
        .expect("웹소켓 응답이 시간 안에 와야 한다")
        .expect("세션이 있으면 웹소켓이 열려야 한다");
    ws
}

/// 다음 글자 프레임. 닫혔으면 `None`, 시간 안에 아무 일도 없으면 `Err`.
async fn next_text(ws: &mut Ws, deadline: tokio::time::Instant) -> Result<Option<String>, ()> {
    loop {
        match tokio::time::timeout_at(deadline, ws.next()).await {
            Err(_) => return Err(()),
            Ok(None | Some(Err(_)) | Some(Ok(Message::Close(_)))) => return Ok(None),
            Ok(Some(Ok(Message::Text(text)))) => return Ok(Some(text.as_str().to_string())),
            Ok(Some(Ok(_))) => {}
        }
    }
}

/// `wanted`가 값을 돌려주는 프레임이 올 때까지 읽는다. 시간 안에 없으면 `None`.
async fn find_frame<T>(
    ws: &mut Ws,
    wait: Duration,
    mut wanted: impl FnMut(&Value) -> Option<T>,
) -> Option<T> {
    let deadline = tokio::time::Instant::now() + wait;
    while let Ok(Some(text)) = next_text(ws, deadline).await {
        let frame: Value = serde_json::from_str(&text).expect("프레임은 JSON이어야 한다");
        if let Some(found) = wanted(&frame) {
            return Some(found);
        }
    }
    None
}

/// 그 종류의 지나가는 이벤트(번호 없음)의 내용.
fn transient(frame: &Value, kind: &str) -> Option<Value> {
    (frame["kind"] == json!(kind) && frame.get("seq").is_none()).then(|| frame["payload"].clone())
}

/// 시세 묶음이 올 때까지 기다린다.
async fn wait_quotes(ws: &mut Ws) -> Value {
    find_frame(ws, STEP, |frame| {
        transient(frame, KIND_QUOTES)
            .filter(|q| q["quotes"].as_array().is_some_and(|l| !l.is_empty()))
    })
    .await
    .expect("시세 묶음이 실시간 연결로 와야 한다")
}

/// `wait` 동안 늦은 종목이 하나 이상인 "시세 늦음"이 오면 그 내용.
async fn late_within(ws: &mut Ws, wait: Duration) -> Option<Value> {
    find_frame(ws, wait, |frame| {
        transient(frame, KIND_QUOTES_STALE)
            .filter(|late| late["symbols"].as_array().is_some_and(|l| !l.is_empty()))
    })
    .await
}

// ── 가짜 CLI ──

/// 가짜 CLI의 동작(셸 글).
struct Behave {
    version: String,
    auth: String,
    model: String,
}

/// 한 줄을 찍고 끝 코드로 끝내는 셸 글.
fn say(text: &str, code: i32) -> String {
    format!("printf '%s\\n' '{text}'\nexit {code}")
}

impl Behave {
    /// 설치돼 있고 구독(`claude.ai`, `max`)으로 로그인돼 있다. 모델 호출은 실패로 끝난다.
    fn logged_in() -> Self {
        let auth = json!({
            "loggedIn": true,
            "authMethod": "claude.ai",
            "apiProvider": "firstParty",
            "subscriptionType": "max",
        });
        Self {
            version: say(CLI_VERSION_TEXT, 0),
            auth: say(&auth.to_string(), 0),
            model: "exit 1".to_string(),
        }
    }

    /// 버전 확인이 `seconds`초 걸린다.
    fn slow_version(mut self, seconds: u32) -> Self {
        self.version = format!("/bin/sleep {seconds}\n{}", say(CLI_VERSION_TEXT, 0));
        self
    }

    /// 모델 호출이 제 번호를 남기고 오래 잔다(스스로 끝나지 않는 자식).
    fn hanging_model(mut self) -> Self {
        self.model = "printf '%s' \"$$\" > \"$L/model_pid\"\nexec /bin/sleep 30".to_string();
        self
    }
}

/// 임시 폴더 하나에 든 가짜 CLI와 그 기록.
struct FakeCli {
    _dir: tempfile::TempDir,
    log: PathBuf,
    bin: PathBuf,
}

impl FakeCli {
    fn new(behave: &Behave) -> Self {
        let dir = tempfile::tempdir().unwrap();
        // macOS의 임시 폴더는 다른 경로를 거치므로 실제 경로로 바꿔 둔다.
        let root = std::fs::canonicalize(dir.path()).unwrap();
        let log = root.join("log");
        let bin_dir = root.join("bin");
        std::fs::create_dir(&log).unwrap();
        std::fs::create_dir(&bin_dir).unwrap();
        let bin = bin_dir.join("claude");
        let script = format!(
            "#!/bin/sh\nL='{log}'\nprintf '%s\\n' \"$*\" >> \"$L/calls\"\ncase \"$1\" in\n--version)\n{version}\n;;\nauth)\n{auth}\n;;\nesac\n{model}\n",
            log = log.display(),
            version = behave.version,
            auth = behave.auth,
            model = behave.model,
        );
        std::fs::write(&bin, script).unwrap();
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
        Self {
            _dir: dir,
            log,
            bin,
        }
    }

    /// 가짜가 불린 인자 줄들(`--version` · `auth status --json` · 모델 호출).
    fn calls(&self) -> Vec<String> {
        std::fs::read_to_string(self.log.join("calls"))
            .unwrap_or_default()
            .lines()
            .map(str::to_string)
            .collect()
    }

    /// 모델 호출이 남긴 자식 번호.
    fn model_pid(&self) -> Option<String> {
        std::fs::read_to_string(self.log.join("model_pid"))
            .ok()
            .filter(|pid| !pid.trim().is_empty())
    }

    /// 이 가짜 CLI를 고른 것을 보관 폴더에 둔다(띄우기 전에).
    fn choose_in(&self, home: &Path) {
        let choice = AiChoice {
            method: AiMethod::Cli,
            command: Some(self.bin.clone()),
            config_dir: None,
        };
        save_ai_choice(home, &choice).expect("고른 것을 저장해야 한다");
    }
}

/// 프로세스가 아직 있는가(`kill -0`).
fn alive(pid: &str) -> bool {
    std::process::Command::new("/bin/kill")
        .args(["-0", pid.trim()])
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

/// 테스트가 실패해도 남은 자식을 치운다.
struct KillOnDrop(Option<String>);

impl Drop for KillOnDrop {
    fn drop(&mut self) {
        if let Some(pid) = &self.0 {
            let _ = std::process::Command::new("/bin/kill")
                .args(["-9", pid.trim()])
                .stderr(std::process::Stdio::null())
                .status();
        }
    }
}

/// 아주 짧은 AI 요청 하나.
fn tiny_request() -> CompletionRequest {
    CompletionRequest {
        model: "test-model".to_string(),
        system: None,
        messages: vec![pi_ai::Message {
            role: Role::User,
            content: "안녕".to_string(),
        }],
        max_tokens: 16,
    }
}

// ── 가짜 토스증권 · 가짜 Anthropic ──

/// 토큰 · 현재가를 주고 국내 달력은 주지 않는(404) 가짜 토스증권. `failing`이면 현재가가 503이다.
struct NoCalendarToss {
    server: MockServer,
    failing: Arc<AtomicBool>,
}

impl NoCalendarToss {
    async fn start() -> Self {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path(TOKEN_PATH))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "access_token": TOSS_TOKEN, "token_type": "Bearer", "expires_in": 86_400
            })))
            .mount(&server)
            .await;
        let failing = Arc::new(AtomicBool::new(false));
        let fail = Arc::clone(&failing);
        Mock::given(method("GET"))
            .and(path(PRICES_PATH))
            .respond_with(move |_: &Request| {
                if fail.load(Ordering::SeqCst) {
                    return ResponseTemplate::new(503).set_body_json(json!({
                        "error": { "requestId": "r-7", "code": "maintenance", "message": "점검" }
                    }));
                }
                ResponseTemplate::new(200).set_body_json(json!({ "result": [{
                    "symbol": "005930",
                    "timestamp": "2026-03-25T10:00:00.000+09:00",
                    "lastPrice": "72000",
                    "currency": "KRW",
                }]}))
            })
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path(CALENDAR_KR_PATH))
            .respond_with(ResponseTemplate::new(404).set_body_json(json!({
                "error": { "requestId": "r-8", "code": "not-found", "message": "없음" }
            })))
            .mount(&server)
            .await;
        Self { server, failing }
    }
}

/// 어떤 키든 받는 가짜 Anthropic(모델 목록만).
async fn accepting_ai() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(MODELS_PATH))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": [], "has_more": false, "first_id": null, "last_id": null
        })))
        .mount(&server)
        .await;
    server
}

/// 증권사 키와 AI API 키를 가짜 서버로 확인해 보관 폴더에 등록한다.
async fn register_keys(home: &Path, ups: &Upstreams) {
    let secrets = SecretStore::open(home).unwrap();
    let verifier = KeyVerifier::new(ups.clone());
    let broker = Credentials::new()
        .with("client_id", CLIENT_ID)
        .with("client_secret", CLIENT_SECRET);
    secrets
        .register(KeyKind::Broker, broker, &verifier)
        .await
        .expect("가짜 서버가 토큰을 주면 등록된다");
    let ai = Credentials::new().with("api_key", AI_KEY);
    secrets
        .register(KeyKind::AiApiKey, ai, &verifier)
        .await
        .expect("가짜 서버가 키를 받으면 등록된다");
}

// ───────────── 3. `--demo`에 종목이 없으면 시연 종목 하나 ─────────────

// 검증: D11, C2, E3, V31, V6
#[tokio::test]
async fn 시연에_종목이_없으면_시연_종목_하나의_시세가_오고_저장된_종목은_비어_있는_그대로다() {
    let dir = tempfile::tempdir().unwrap();
    for round in ["처음", "다시 켜서"] {
        let app = start_app(
            settings_without_symbols(dir.path(), &["--demo"]),
            wiring(TestClock::at(kst(10, 0))),
        )
        .await;
        let mut ws = connect(app.local_addr(), 0).await;
        let quotes = wait_quotes(&mut ws).await;
        assert_eq!(quotes["demo"], json!(true), "{round}: 시연용 시세다");
        let list = quotes["quotes"].as_array().unwrap();
        assert_eq!(list.len(), 1, "{round}: 시연 종목 하나만 쓴다: {quotes}");
        let symbol = list[0]["symbol"].as_str().expect("종목 글자가 있어야 한다");
        assert!(
            pi_market::Symbol::parse(symbol).is_ok(),
            "{round}: 시연 종목도 종목 글자 규칙을 지킨다"
        );
        let status = setup_status(app.local_addr()).await;
        assert!(
            !registered(&status, ITEM_SYMBOLS),
            "{round}: 시연 종목은 등록된 종목이 아니다: {status}"
        );
        stop(app).await;
        assert!(
            !dir.path().join(WATCHLIST_FILE).exists(),
            "{round}: 시연 종목을 저장하지 않는다"
        );
        assert!(load_watchlist(dir.path()).unwrap().is_empty());
    }
}

// ───────────── 4. 종목 등록은 `Symbol::parse`로 글자를 확인한다 ─────────────

// 검증: C3, C16, E4
#[tokio::test]
async fn 종목_글자가_틀리면_등록하지_않고_이유를_보이되_받은_글자를_되돌려_보이지_않는다() {
    let dir = tempfile::tempdir().unwrap();
    let app = start_app(
        settings_without_symbols(dir.path(), &["--symbols-attempts", "100"]),
        wiring(TestClock::at(kst(10, 0))),
    )
    .await;
    let addr = app.local_addr();
    // `pi_market::Symbol::parse`가 받지 않는 글자(밑줄)가 든 종목들.
    for bad in [["005_930"].as_slice(), ["005930", "BRK_B"].as_slice()] {
        assert!(bad.iter().any(|s| pi_market::Symbol::parse(s).is_err()));
        let reply = post_json(addr, SETUP_SYMBOLS_PATH, &json!({ "symbols": bad })).await;
        assert!(
            (400..500).contains(&reply.status),
            "{bad:?}: 틀린 종목 글자는 등록하지 않는다: {} {}",
            reply.status,
            reply.text
        );
        let reason = reply.json()["error"].clone();
        assert!(
            reason.as_str().is_some_and(|r| !r.is_empty()),
            "{bad:?}: 거부한 이유를 보인다: {}",
            reply.text
        );
        for symbol in bad.iter().filter(|s| s.contains('_')) {
            assert!(
                !reply.text.contains(symbol),
                "응답에 받은 글자를 그대로 되돌려 보이지 않는다: {}",
                reply.text
            );
        }
        assert!(!registered(&setup_status(addr).await, ITEM_SYMBOLS));
        assert!(!dir.path().join(WATCHLIST_FILE).exists());
    }
    // 맞는 글자(영문 · 숫자 · 점 하나)는 등록된다.
    let reply = post_json(
        addr,
        SETUP_SYMBOLS_PATH,
        &json!({ "symbols": ["005930", "BRK.B"] }),
    )
    .await;
    assert_eq!(
        reply.status, 200,
        "맞는 종목 글자는 등록된다: {}",
        reply.text
    );
    assert!(registered(&setup_status(addr).await, ITEM_SYMBOLS));
    stop(app).await;
    let saved: Vec<String> = load_watchlist(dir.path())
        .unwrap()
        .iter()
        .map(|s| s.as_str().to_string())
        .collect();
    assert_eq!(saved, ["005930", "BRK.B"]);
}

// ───────────── 5. 꺼질 때 CLI 자식을 끝낸다 ─────────────

// 검증: V46, E38
#[tokio::test]
async fn 서버가_정상으로_꺼지면_cli_공급자를_끝내_남은_자식이_없다() {
    let dir = tempfile::tempdir().unwrap();
    let fake = FakeCli::new(&Behave::logged_in().hanging_model());
    fake.choose_in(dir.path());
    let app = start_app(settings(dir.path(), &[]), wiring(TestClock::at(kst(10, 0)))).await;
    let status = wait_ai_cli(app.local_addr(), |cli| *cli == json!(CLI_LOGGED_IN)).await;
    assert_eq!(status["ai"]["cli"], json!(CLI_LOGGED_IN), "{status}");

    let provider = app.ai_provider();
    let call = tokio::spawn(async move { provider.complete(tiny_request()).await });
    let deadline = Instant::now() + STEP;
    let pid = loop {
        if let Some(pid) = fake.model_pid() {
            break pid;
        }
        assert!(Instant::now() < deadline, "모델 호출이 자식을 띄워야 한다");
        tokio::time::sleep(Duration::from_millis(50)).await;
    };
    let _cleanup = KillOnDrop(Some(pid.clone()));
    assert!(alive(&pid), "끄기 전에는 자식이 살아 있다");

    stop(app).await;
    assert!(
        !alive(&pid),
        "서버가 꺼지면(끄기가 끝나면) CLI 자식이 끝나 거둬져 있어야 한다"
    );
    let result = tokio::time::timeout(STEP, call)
        .await
        .expect("끝낸 뒤 호출이 돌아와야 한다")
        .expect("호출 작업이 끝나야 한다");
    assert!(result.is_err(), "끝낸 자식의 호출은 실패로 돌아온다");
}

// ───────────── 6. 옛 봉투 판단을 저장소와 맞춘다 ─────────────

/// 저장소 버전 3 픽스처(`pi-store`의 테스트 자료). 이벤트 1~11번이 있다.
const FIXTURE_V3: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../store/tests/fixtures/v3.db");
/// 시험 줄을 쌓기 시작하는 번호(픽스처의 마지막 번호 다음).
const FIRST_ROW: u64 = 12;
/// 시험 줄 뒤에 쌓는 바른 옛 봉투 줄. 저장소는 이 줄까지를 옛 형식 경계로 정한다.
const TAIL_ENVELOPE: &str = r#"{"at":1800000003000,"payload":{"ok":"끝"}}"#;

/// 버전 3 픽스처를 복사해 `rows`를 [`FIRST_ROW`]번부터(시각 칸은 비움) 쌓고, 마지막에
/// [`TAIL_ENVELOPE`]를 쌓은 뒤 저장소로 연다. 버전 4로 올라가며 옛 형식 경계는 그 마지막 줄이 되므로
/// 시험 줄은 모두 경계 안이다. 접속 관문을 그 저장소로 띄운다.
async fn gate_on_fixture_with(
    rows: &[&str],
) -> (tempfile::TempDir, pi_store::Store, pi_gate::GateServer) {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("v3.db");
    std::fs::copy(FIXTURE_V3, &file).unwrap();
    let last = FIRST_ROW + u64::try_from(rows.len()).unwrap();
    {
        let conn = rusqlite::Connection::open(&file).unwrap();
        for (seq, raw) in (FIRST_ROW..).zip(rows.iter().chain([&TAIL_ENVELOPE])) {
            conn.execute(
                "INSERT INTO events (seq, kind, payload, at) VALUES (?1, 'chat', ?2, NULL)",
                rusqlite::params![i64::try_from(seq).unwrap(), raw],
            )
            .unwrap();
        }
    }
    let store = pi_store::Store::open(&file).unwrap();
    assert_eq!(
        store.legacy_envelope_until().unwrap(),
        Some(last),
        "경계는 마지막 옛 봉투 줄이다(시험 줄은 모두 경계 안이다)"
    );
    let gate = pi_gate::Gate::builder(store.clone(), pi_gate::GateConfig::local(0))
        .build()
        .serve()
        .await
        .unwrap();
    (dir, store, gate)
}

/// 다시 보내기로 받은 [`FIRST_ROW`]번부터의 저장된 프레임 `count`개(번호 순서).
async fn replayed_rows(addr: SocketAddr, count: usize) -> Vec<Value> {
    let mut ws = connect(addr, FIRST_ROW - 1).await;
    let mut frames = Vec::new();
    while frames.len() < count {
        let frame = find_frame(&mut ws, STEP, |frame| {
            frame.get("seq").is_some().then(|| frame.clone())
        })
        .await
        .expect("경계 안의 저장된 줄이 다시 보내져야 한다");
        frames.push(frame);
    }
    frames
}

/// 시험 줄은 옛 봉투로 풀리지 않고 저장된 내용 그대로(JSON으로 읽은 값) 오고,
/// 마지막의 바른 옛 봉투 줄은 예전처럼 풀린다.
async fn assert_rows_sent_as_is(addr: SocketAddr, rows: &[&str]) {
    let frames = replayed_rows(addr, rows.len() + 1).await;
    for ((seq, frame), raw) in (FIRST_ROW..).zip(&frames).zip(rows) {
        let stored: Value = serde_json::from_str(raw).unwrap();
        assert_eq!(frame["seq"], json!(seq), "번호 순서대로 온다");
        assert_eq!(
            frame["payload"], stored,
            "{seq}번({raw})은 경계 안이어도 옛 봉투로 풀지 않고 내용 그대로 보낸다: {frame}"
        );
    }
    let tail = &frames[rows.len()];
    assert_eq!(
        tail["payload"],
        json!({ "ok": "끝" }),
        "바른 옛 봉투는 푼다: {tail}"
    );
    assert_eq!(tail["at"], json!(1_800_000_003_000_i64), "{tail}");
}

// 검증: C2, E3, V6
#[tokio::test]
async fn 경계_안이어도_at이_i64_범위_밖이거나_음의_0이거나_실수_모양이면_옛_봉투로_풀지_않는다() {
    let rows = [
        r#"{"at":9223372036854775808,"payload":{"n":1}}"#,
        r#"{"at":-9223372036854775809,"payload":{"n":2}}"#,
        r#"{"at":-0,"payload":{"n":3}}"#,
        r#"{"at":1800000001000.0,"payload":{"n":4}}"#,
        r#"{"at":1.8000000011e12,"payload":{"n":5}}"#,
    ];
    let (_dir, store, gate) = gate_on_fixture_with(&rows).await;
    assert_rows_sent_as_is(gate.local_addr(), &rows).await;
    gate.shutdown().await.unwrap();
    drop(store);
}

// 검증: C2, E3, N8
#[tokio::test]
async fn 경계_안이어도_맨_위_칸_이름이_겹치면_옛_봉투로_풀지_않는다() {
    // 겹친 이름을 하나로 읽으면 `{"at": 정수, "payload": …}` 두 칸이 되는 줄들.
    let rows = [
        r#"{"at":1800000002100,"at":1800000002100,"payload":{"n":1}}"#,
        r#"{"at":1800000002110,"payload":{"n":2},"payload":{"n":2}}"#,
        r#"{"payload":3,"at":1800000002120,"payload":3}"#,
        r#"{"at":1800000002130,"payload":null,"at":1800000002130,"payload":null}"#,
    ];
    let (_dir, store, gate) = gate_on_fixture_with(&rows).await;
    assert_rows_sent_as_is(gate.local_addr(), &rows).await;
    gate.shutdown().await.unwrap();
    drop(store);
}

// ───────────── 7. 늦은 시세 판단을 `pi-market`에 맡긴다 ─────────────

// 검증: N6, V31
#[tokio::test]
async fn 미국_서머타임_날_00시_30분에_데모_달력과_방금_받은_시세는_늦음이_아니다() {
    // 2026-07-15(수) 00:30 미 동부 서머타임(UTC-4) = 04:30 UTC. UTC-5로 세면 아직 14일이다.
    let now = Utc.with_ymd_and_hms(2026, 7, 15, 4, 30, 0).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let app = start_app(
        settings_without_symbols(
            dir.path(),
            &["--demo", "--market", "us", "--symbols", "AAPL"],
        ),
        wiring(TestClock::at(now)),
    )
    .await;
    let mut ws = connect(app.local_addr(), 0).await;
    let quotes = wait_quotes(&mut ws).await;
    assert_eq!(
        quotes["quotes"][0]["received_at"],
        json!(now.timestamp_millis())
    );
    let late = late_within(&mut ws, QUIET).await;
    assert_eq!(
        late, None,
        "시계가 멈춰 있어 방금 받은 시세다. 장 닫힘이고 달력이 지금을 덮으니 늦음이 아니다"
    );
    stop(app).await;
}

// 검증: N6, V32
#[tokio::test]
async fn 달력이_없어도_받은_시각이_미래인_시세는_늦음이다() {
    let toss = NoCalendarToss::start().await;
    let ai = accepting_ai().await;
    let ups = upstreams(Some(&toss.server.uri()), Some(&ai.uri()));
    let dir = tempfile::tempdir().unwrap();
    register_keys(dir.path(), &ups).await;
    let clock = TestClock::at(kst(10, 0));
    let wiring = Wiring {
        upstreams: ups,
        clock: Arc::clone(&clock) as Arc<dyn pi_market::Clock>,
        peer_override: None,
        cli_locations: Vec::new(),
    };
    let extra = ["--retry-initial-ms", "20", "--retry-max-ms", "40"];
    let app = start_app(settings(dir.path(), &extra), wiring).await;
    let mut ws = connect(app.local_addr(), 0).await;
    let quotes = wait_quotes(&mut ws).await;
    assert_eq!(
        quotes["quotes"][0]["received_at"],
        json!(kst(10, 0).timestamp_millis())
    );
    assert_eq!(late_within(&mut ws, Duration::from_millis(300)).await, None);

    // 더 받지 못하게 하고, 시계를 한 시간 되돌린다: 가진 시세는 받은 시각이 지금보다 뒤다.
    toss.failing.store(true, Ordering::SeqCst);
    tokio::time::sleep(Duration::from_millis(300)).await;
    clock.set(kst(9, 0));
    let late = late_within(&mut ws, STEP)
        .await
        .expect("달력이 없어도 받은 시각이 미래인 시세는 늦은 것이다");
    assert!(
        late["symbols"]
            .as_array()
            .unwrap()
            .contains(&json!("005930")),
        "{late}"
    );
    stop(app).await;
}

// ───────────── 8. AI 고르기 경로도 접근 제한 ─────────────

// 검증: C15, D2, M6, C3
#[tokio::test]
async fn ai_고르기는_세션_없음_다른_출처_바깥_주소에서_거부된다() {
    let dir = tempfile::tempdir().unwrap();
    let app = start_app(settings(dir.path(), &[]), wiring(TestClock::at(kst(10, 0)))).await;
    let addr = app.local_addr();
    let origin = origin_of(addr);
    let body = json!({ "method": "api_key" }).to_string();
    let reply = post_raw(addr, SETUP_AI_PATH, None, Some(&origin), body.clone()).await;
    assert_eq!(reply.status, 401, "세션 없이 고를 수 없다: {}", reply.text);
    let cookie = login(addr).await;
    let reply = post_raw(addr, SETUP_AI_PATH, Some(&cookie), None, body.clone()).await;
    assert!(
        (400..500).contains(&reply.status),
        "Origin이 없으면 거부한다: {}",
        reply.status
    );
    let evil = Some("http://evil.example");
    let reply = post_raw(addr, SETUP_AI_PATH, Some(&cookie), evil, body.clone()).await;
    assert!(
        (400..500).contains(&reply.status),
        "다른 출처면 거부한다: {}",
        reply.status
    );
    stop(app).await;
    assert!(
        !dir.path().join(AI_CHOICE_FILE).exists(),
        "거부된 고르기는 저장하지 않는다"
    );

    // 상대 주소를 루프백이 아닌 것으로 본다(문서용 주소 대역).
    let outside: SocketAddr = "203.0.113.7:40000".parse().unwrap();
    let mut wired = wiring(TestClock::at(kst(10, 0)));
    wired.peer_override = Some(Arc::new(move |_actual: SocketAddr| outside));
    let app = start_app(settings(dir.path(), &[]), wired).await;
    let addr = app.local_addr();
    let origin = origin_of(addr);
    let reply = post_raw(addr, SETUP_AI_PATH, None, Some(&origin), body).await;
    assert!(
        reply.status == 401 || reply.status == 403,
        "밖에서 온 고르기는 거부한다: {}",
        reply.status
    );
    stop(app).await;
    assert!(!dir.path().join(AI_CHOICE_FILE).exists());
    // 접속 기록(M6)에는 그 컴퓨터에서 받은 세션만 있고, 밖의 상대는 들어오지 못했다.
    let store = pi_store::Store::open(dir.path().join(DB_FILE)).unwrap();
    let log = store.access_log().unwrap();
    assert!(!log.is_empty(), "그 컴퓨터에서 받은 세션은 기록된다");
    assert!(
        log.iter().all(|r| !r.origin.contains("203.0.113.7")),
        "{log:?}"
    );
}

// ───────────── 10. 작업 폴더는 비어 있어야 한다 ─────────────

// 검증: N8, V46
#[test]
fn 이미_있는_작업_폴더가_비어_있지_않으면_지우지_않고_거부한다() {
    let dir = tempfile::tempdir().unwrap();
    let work = dir.path().join(CLI_WORK_DIR);
    std::fs::create_dir(&work).unwrap();
    // 비어 있는 이미 있는 폴더는 받는다.
    assert_eq!(prepare_cli_work_dir(dir.path()).unwrap(), work);
    std::fs::write(work.join("keep.txt"), "남겨 둘 것").unwrap();
    assert!(
        prepare_cli_work_dir(dir.path()).is_err(),
        "안에 무엇이 있으면 작업 폴더로 쓰지 않는다"
    );
    assert_eq!(
        std::fs::read_to_string(work.join("keep.txt")).unwrap(),
        "남겨 둘 것",
        "안의 것을 지우지 않는다"
    );
}

// 검증: N8, V46, C16
#[tokio::test]
async fn 작업_폴더에_무엇이_있으면_cli를_쓰지_않고_이유를_상태로_보인다() {
    let dir = tempfile::tempdir().unwrap();
    let fake = FakeCli::new(&Behave::logged_in());
    fake.choose_in(dir.path());
    let work = dir.path().join(CLI_WORK_DIR);
    std::fs::create_dir(&work).unwrap();
    std::fs::set_permissions(&work, std::fs::Permissions::from_mode(0o700)).unwrap();
    std::fs::write(work.join("keep.txt"), "남겨 둘 것").unwrap();

    let app = start_app(settings(dir.path(), &[]), wiring(TestClock::at(kst(10, 0)))).await;
    let status = wait_ai_cli(app.local_addr(), |cli| *cli != json!(CLI_CHECKING)).await;
    assert_eq!(
        status["ai"]["cli"],
        json!(CLI_WORK_NOT_EMPTY),
        "작업 폴더가 비어 있지 않다는 이유를 상태로 보인다: {status}"
    );
    assert!(
        !registered(&status, ITEM_AI),
        "AI 연결이 갖춰지지 않은 것이다"
    );
    let result = app.ai_provider().complete(tiny_request()).await;
    assert!(result.is_err(), "CLI를 쓰지 않는다");
    stop(app).await;
    assert!(
        fake.calls().is_empty(),
        "CLI를 띄우지 않는다: {:?}",
        fake.calls()
    );
    assert_eq!(
        std::fs::read_to_string(work.join("keep.txt")).unwrap(),
        "남겨 둘 것",
        "안의 것을 지우지 않는다"
    );
}

// ───────────── 11. CLI 확인이 서버 시작을 막지 않는다 ─────────────

// 검증: V32, V46
#[tokio::test]
async fn cli_확인이_느려도_서버는_먼저_연결을_받고_상태는_확인_중이다() {
    let dir = tempfile::tempdir().unwrap();
    let fake = FakeCli::new(&Behave::logged_in().slow_version(3));
    fake.choose_in(dir.path());
    let started = Instant::now();
    let app = start_app(settings(dir.path(), &[]), wiring(TestClock::at(kst(10, 0)))).await;
    let took = started.elapsed();
    assert!(
        took < Duration::from_millis(1500),
        "CLI 확인(3초)을 기다리지 않고 띄운다: {took:?}"
    );
    let status = setup_status(app.local_addr()).await;
    assert_eq!(
        status["ai"]["cli"],
        json!(CLI_CHECKING),
        "확인이 끝나기 전 상태는 확인 중이다: {status}"
    );
    assert!(
        !registered(&status, ITEM_AI),
        "확인 중에는 갖춰진 것이 아니다"
    );
    let status = wait_ai_cli(app.local_addr(), |cli| *cli == json!(CLI_LOGGED_IN)).await;
    assert_eq!(
        status["ai"]["cli"],
        json!(CLI_LOGGED_IN),
        "확인은 뒤에서 끝난다: {status}"
    );
    assert!(registered(&status, ITEM_AI));
    stop(app).await;
}

// ───────────── 12. 망가진 `ai.json`이면 어느 파일인지 알린다 ─────────────

// 검증: N8, E4
#[tokio::test]
async fn 망가진_ai_json이면_시작_오류에_파일_이름이_있고_값은_없다() {
    let marker = "test-broken-choice-not-real";
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join(AI_CHOICE_FILE),
        format!(r#"{{"method":"cli","command":"{marker}"}}"#),
    )
    .unwrap();
    let started = tokio::time::timeout(
        STEP,
        App::start(settings(dir.path(), &[]), wiring(TestClock::at(kst(10, 0)))),
    )
    .await
    .expect("시간 안에 끝나야 한다");
    let Err(error) = started else {
        panic!("읽을 수 없는 ai.json이면 띄우지 않는다");
    };
    let text = error.to_string();
    assert!(
        text.contains(AI_CHOICE_FILE),
        "어느 파일인지 알린다: {text}"
    );
    assert!(!text.contains(marker), "파일의 값은 넣지 않는다: {text}");
    assert!(
        !format!("{error:?}").contains(marker),
        "디버그 글에도 값이 없다"
    );
}

// ───────────── 13. PATH의 상대 경로 · 화면의 경로 칸 · API 키 고르기의 횟수 ─────────────

/// 지금 작업 폴더에서 `abs`로 가는 상대 경로(`../../…/abs`).
fn relative_from_cwd(abs: &Path) -> String {
    let cwd = std::env::current_dir().unwrap();
    let ups = cwd.components().count() - 1;
    let tail = abs.to_str().unwrap().trim_start_matches('/');
    format!("{}{tail}", "../".repeat(ups))
}

/// 실행할 수 있는 가짜 `claude`를 그 폴더에 둔다.
fn put_claude(dir: &Path) -> PathBuf {
    let bin = dir.join("claude");
    std::fs::write(&bin, "#!/bin/sh\nexit 0\n").unwrap();
    std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
    bin
}

// 검증: E38, C3
#[test]
fn path의_상대_경로_항목은_cli_후보가_아니다() {
    let temp = tempfile::tempdir().unwrap();
    let root = std::fs::canonicalize(temp.path()).unwrap();
    let near = root.join("near");
    let far = root.join("far");
    std::fs::create_dir(&near).unwrap();
    std::fs::create_dir(&far).unwrap();
    put_claude(&near);
    let far_bin = put_claude(&far);
    let relative = relative_from_cwd(&near);
    assert!(
        Path::new(&relative).join("claude").is_file(),
        "상대 경로로도 그 파일에 닿는다(시험 준비)"
    );
    assert_eq!(
        find_claude(Some(&relative), None, &[]),
        None,
        "상대 경로 항목은 보지 않는다"
    );
    let var = format!("{relative}:.:{}", far.display());
    assert_eq!(
        find_claude(Some(&var), None, &[]),
        Some(far_bin),
        "상대 경로 항목을 건너뛰고 절대 경로 항목에서 찾는다"
    );
}

// 검증: D11, V46
#[test]
fn 화면은_사용자가_고치던_cli_경로_칸을_상태로_덮지_않는다() {
    // 11차(Vite 이전): 스크립트가 묶음으로 나가므로 빌드한 화면과 그 묶음에서 찾는다.
    let page = screen::built_page("index.html");
    assert!(page.contains("f-cli-command") && page.contains("f-cli-config-dir"));
    // 사용자가 칸을 고친 것을 알아채야(입력 이벤트) 상태를 다시 그릴 때 그 칸을 덮지 않을 수 있다.
    // 지금 화면은 칸에 초점이 있을 때만 덮지 않아, 고친 뒤 다른 곳을 누르면 다음 상태가 덮는다.
    assert!(
        page.contains("addEventListener(\"input\"") || page.contains(".oninput"),
        "화면은 경로 칸을 사용자가 고쳤는지 입력 이벤트로 기억해야 한다"
    );
}

// 검증: C3, E4, C16
#[tokio::test]
async fn api_키_고르기도_등록_시도_횟수_제한에_든다() {
    let dir = tempfile::tempdir().unwrap();
    let app = start_app(
        settings(dir.path(), &["--register-attempts", "2"]),
        wiring(TestClock::at(kst(10, 0))),
    )
    .await;
    let addr = app.local_addr();
    let body = json!({ "method": "api_key" });
    for attempt in 1..=2 {
        let reply = post_json(addr, SETUP_AI_PATH, &body).await;
        assert_eq!(
            reply.status, 200,
            "{attempt}번째는 횟수 안이다: {}",
            reply.text
        );
    }
    let reply = post_json(addr, SETUP_AI_PATH, &body).await;
    assert_eq!(reply.status, 429, "횟수를 넘으면 거부한다: {}", reply.text);
    assert_eq!(reply.json(), json!({ "error": "too_many_attempts" }));
    stop(app).await;
}
