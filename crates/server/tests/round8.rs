//! 8차 작업 1~5의 테스트(`docs/dev/modules/gate.md` "8차 작업").
//!
//! 잠긴 `server.rs` · `round7.rs`와 그 아래 폴더를 고치지 않으려고 따로 떼어 낸 최상위 테스트 파일이다.
//! 도우미는 이 파일 안에 둔다. 서버는 임시 보관 폴더로 `127.0.0.1:0`에 띄우고, 바깥 서버는 아무도 듣지
//! 않는 로컬 주소로, Claude Code CLI는 임시 폴더에 쓴 가짜 셸 스크립트로 대신한다.
//! 실제 `claude`, 토스증권, Anthropic을 부르지 않고 사용자의 `~/.claude`를 열지 않는다.
//!
//! - 4번(다시 보내기가 `legacy_envelope_seqs_upto`로 읽은 쪽의 마지막 번호까지만 훑기)은 바깥에서 보이는
//!   동작이 같아 테스트하지 않는다(다시 보내기의 동작은 잠긴 테스트들이 지킨다).
//! - 5번(`CLI_CHECK_GRACE`의 까닭)은 문서 주석뿐이다.

use std::net::SocketAddr;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use chrono::{DateTime, TimeZone, Utc};
use futures::StreamExt;
use pi_ai::{AnthropicConfig, CompletionRequest, Role};
use pi_market::TossConfig;
use price_inha::{
    AI_CHOICE_FILE, AiChoice, AiMethod, App, CLI_CHECKING, CLI_LOGGED_IN, CLI_WORK_DIR,
    CLI_WORK_NOT_EMPTY, ITEM_AI, KIND_QUOTES, KIND_QUOTES_STALE, SETUP_AI_PATH, SETUP_PATH,
    Settings, Upstreams, WATCHLIST_FILE, Wiring, load_ai_choice, save_ai_choice,
};
use serde_json::{Value, json};
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

// ───────────── 도우미 ─────────────

/// 한 걸음을 기다리는 가장 긴 시간.
const STEP: Duration = Duration::from_secs(10);
/// "오지 않는다"를 확인할 때 기다리는 시간.
const QUIET: Duration = Duration::from_millis(1200);
/// 테스트에서 쓰는 시세 간격.
const INTERVAL_MS: &str = "20";
/// 가짜 CLI가 알리는 버전(쓸 수 있는 가장 낮은 버전보다 높다).
const CLI_VERSION_TEXT: &str = "2.1.316 (Claude Code)";

type Ws = WebSocketStream<MaybeTlsStream<TcpStream>>;

/// 테스트가 시각을 정하는 시계(멈춰 있다).
struct TestClock(AtomicI64);

impl TestClock {
    fn at(time: DateTime<Utc>) -> Arc<Self> {
        Arc::new(Self(AtomicI64::new(time.timestamp_millis())))
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

/// 바깥은 모두 닿지 않는 주소이고, CLI를 스스로 찾는 자리는 비운 연결.
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

async fn send(request: reqwest::RequestBuilder) -> Reply {
    let reply = tokio::time::timeout(STEP, request.send())
        .await
        .expect("응답이 시간 안에 와야 한다")
        .expect("요청이 닿아야 한다");
    let status = reply.status().as_u16();
    let text = reply.text().await.expect("본문을 읽어야 한다");
    Reply { status, text }
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
    let request = reqwest::Client::new()
        .post(format!("{}{path}", origin_of(addr)))
        .header("Content-Type", "application/json")
        .header("Cookie", cookie)
        .header("Origin", origin_of(addr))
        .body(body.to_string());
    send(request).await
}

/// 세션을 받아 설정 상태를 읽는다.
async fn setup_status(addr: SocketAddr) -> Value {
    let cookie = login(addr).await;
    let request = reqwest::Client::new()
        .get(format!("{}{SETUP_PATH}", origin_of(addr)))
        .header("Cookie", cookie);
    let reply = send(request).await;
    assert_eq!(reply.status, 200, "세션이 있으면 설정 상태를 읽는다");
    serde_json::from_str(&reply.text).expect("설정 상태는 JSON이어야 한다")
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

/// 설정 상태의 `ai.cli`가 "확인 중"이 아니게 될 때까지 읽는다. 마지막으로 읽은 설정 상태를 돌려준다.
async fn wait_ai_checked(addr: SocketAddr) -> Value {
    let deadline = Instant::now() + STEP;
    loop {
        let status = setup_status(addr).await;
        if status["ai"]["cli"] != json!(CLI_CHECKING) || Instant::now() >= deadline {
            return status;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// 세션을 받고 실시간 연결을 연다.
async fn connect(addr: SocketAddr) -> Ws {
    let cookie = login(addr).await;
    let url = format!(
        "ws://127.0.0.1:{}{}?{}=0",
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

/// `wanted`가 값을 돌려주는 지나가는 이벤트(번호 없음)가 올 때까지 읽는다. 시간 안에 없으면 `None`.
async fn find_transient(
    ws: &mut Ws,
    wait: Duration,
    kind: &str,
    wanted: impl Fn(&Value) -> bool,
) -> Option<Value> {
    let deadline = tokio::time::Instant::now() + wait;
    loop {
        let text = match tokio::time::timeout_at(deadline, ws.next()).await {
            Err(_) | Ok(None | Some(Err(_)) | Some(Ok(Message::Close(_)))) => return None,
            Ok(Some(Ok(Message::Text(text)))) => text.as_str().to_string(),
            Ok(Some(Ok(_))) => continue,
        };
        let frame: Value = serde_json::from_str(&text).expect("프레임은 JSON이어야 한다");
        if frame["kind"] == json!(kind) && frame.get("seq").is_none() && wanted(&frame["payload"]) {
            return Some(frame["payload"].clone());
        }
    }
}

/// 비어 있지 않은 목록 칸이 있는가.
fn non_empty(payload: &Value, field: &str) -> bool {
    payload[field]
        .as_array()
        .is_some_and(|list| !list.is_empty())
}

// ── 가짜 CLI ──

/// 한 줄을 찍고 끝 코드로 끝내는 셸 글.
fn say(text: &str, code: i32) -> String {
    format!("printf '%s\\n' '{text}'\nexit {code}")
}

/// 임시 폴더 하나에 든 가짜 CLI와 그 기록. 설치돼 있고 구독(`claude.ai`, `max`)으로 로그인돼 있다.
struct FakeCli {
    _dir: tempfile::TempDir,
    log: PathBuf,
    bin: PathBuf,
}

impl FakeCli {
    /// `hanging_model`이면 모델 호출이 제 번호를 남기고 오래 잔다(스스로 끝나지 않는 자식).
    /// 아니면 모델 호출은 실패로 끝난다. 버전 확인은 `version_secs`초 걸린다(0이면 바로).
    fn new(hanging_model: bool, version_secs: u32) -> Self {
        let dir = tempfile::tempdir().unwrap();
        // macOS의 임시 폴더는 다른 경로를 거치므로 실제 경로로 바꿔 둔다.
        let root = std::fs::canonicalize(dir.path()).unwrap();
        let log = root.join("log");
        let bin_dir = root.join("bin");
        std::fs::create_dir(&log).unwrap();
        std::fs::create_dir(&bin_dir).unwrap();
        let bin = bin_dir.join("claude");
        let auth = json!({
            "loggedIn": true,
            "authMethod": "claude.ai",
            "apiProvider": "firstParty",
            "subscriptionType": "max",
        });
        let model = if hanging_model {
            "printf '%s' \"$$\" > \"$L/model_pid\"\nexec /bin/sleep 30"
        } else {
            "exit 1"
        };
        let script = format!(
            "#!/bin/sh\nL='{log}'\nprintf '%s\\n' \"$*\" >> \"$L/calls\"\ncase \"$1\" in\n--version)\n/bin/sleep {version_secs}\n{version}\n;;\nauth)\n{auth}\n;;\nesac\n{model}\n",
            log = log.display(),
            version = say(CLI_VERSION_TEXT, 0),
            auth = say(&auth.to_string(), 0),
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

    /// 이 가짜 CLI를 고르는 요청 본문.
    fn choose_body(&self) -> Value {
        json!({ "method": "cli", "command": self.bin.to_str().unwrap() })
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

/// 테스트가 실패해도 남은 자식을 치운다.
struct KillOnDrop(String);

impl Drop for KillOnDrop {
    fn drop(&mut self) {
        let _ = std::process::Command::new("/bin/kill")
            .args(["-9", self.0.trim()])
            .stderr(std::process::Stdio::null())
            .status();
    }
}

/// 보관 폴더 안에 이미 있는 CLI 작업 폴더(0700)를 만든다.
fn make_work_dir(home: &Path) -> PathBuf {
    let work = home.join(CLI_WORK_DIR);
    std::fs::create_dir(&work).unwrap();
    std::fs::set_permissions(&work, std::fs::Permissions::from_mode(0o700)).unwrap();
    work
}

/// 작업 폴더 안의 `pi-ai` 자기 임시 파일 이름들(정렬).
fn own_files(work: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(work)
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .filter(|name| pi_ai::is_own_work_file(name))
        .map(|name| name.to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

// ───────────── 1. 작업 폴더의 자기 임시 파일은 "비어 있음"을 깨지 않는다 ─────────────

/// 비정상으로 꺼진 서버가 남긴 모양의 `pi-ai` 시스템 글 임시 파일 이름들.
const LEFTOVERS: [&str; 2] = [".pi-ai-system-4242-0.txt", ".pi-ai-system-99999-17.txt"];

// 검증: N8, V46, V6, N6, V31
#[tokio::test]
async fn 시작할_때_작업_폴더에_자기_임시_파일만_있으면_지우고_cli를_쓴다() {
    let dir = tempfile::tempdir().unwrap();
    let fake = FakeCli::new(false, 0);
    fake.choose_in(dir.path());
    let work = make_work_dir(dir.path());
    for name in LEFTOVERS {
        assert!(pi_ai::is_own_work_file(name.as_ref()), "시험 준비: {name}");
        std::fs::write(work.join(name), "남은 시스템 글").unwrap();
    }

    let app = start_app(
        settings(dir.path(), &["--demo"]),
        wiring(TestClock::at(kst(10, 0))),
    )
    .await;
    let addr = app.local_addr();
    let status = wait_ai_checked(addr).await;
    assert_eq!(
        status["ai"]["cli"],
        json!(CLI_LOGGED_IN),
        "자기 임시 파일만 있는 작업 폴더는 비어 있는 것이다: {status}"
    );
    assert!(registered(&status, ITEM_AI), "{status}");
    assert!(
        fake.calls().iter().any(|call| call.contains("--version")),
        "CLI를 확인한다: {:?}",
        fake.calls()
    );
    for name in LEFTOVERS {
        assert!(
            !work.join(name).exists(),
            "시작할 때(호출이 돌지 않을 때) 남은 임시 파일은 지운다: {name}"
        );
    }
    assert!(work.is_dir(), "작업 폴더 자체는 남는다");

    // 서버는 그대로 돈다: 시세가 실시간 연결로 오고, 멈춘 시계의 방금 받은 시세는 늦음이 아니다.
    let mut ws = connect(addr).await;
    let quotes = find_transient(&mut ws, STEP, KIND_QUOTES, |q| non_empty(q, "quotes")).await;
    assert!(quotes.is_some(), "시세 묶음이 실시간 연결로 와야 한다");
    let late = find_transient(&mut ws, QUIET, KIND_QUOTES_STALE, |l| {
        non_empty(l, "symbols")
    })
    .await;
    assert_eq!(late, None);
    stop(app).await;

    // 다시 켜도 같다(지운 뒤의 작업 폴더는 비어 있다).
    let app = start_app(settings(dir.path(), &[]), wiring(TestClock::at(kst(10, 0)))).await;
    let status = wait_ai_checked(app.local_addr()).await;
    assert_eq!(status["ai"]["cli"], json!(CLI_LOGGED_IN), "{status}");
    stop(app).await;
}

// 검증: N8, V46, C16, E38
#[tokio::test]
async fn 자기_임시_파일_말고_다른_항목이_하나라도_있으면_아무것도_지우지_않고_막는다() {
    // 자기 임시 파일과 닮았지만 아닌 이름들도 "다른 항목"이다.
    let others = [
        "keep.txt",
        ".pi-ai-system-12-3.txt.bak",
        ".pi-ai-system-x-3.txt",
        "pi-ai-system-12-3.txt",
    ];
    for other in others {
        assert!(
            !pi_ai::is_own_work_file(other.as_ref()),
            "시험 준비: {other}"
        );
        let dir = tempfile::tempdir().unwrap();
        let fake = FakeCli::new(false, 0);
        fake.choose_in(dir.path());
        let work = make_work_dir(dir.path());
        std::fs::write(work.join(LEFTOVERS[0]), "남은 시스템 글").unwrap();
        std::fs::write(work.join(other), "남겨 둘 것").unwrap();

        let app = start_app(settings(dir.path(), &[]), wiring(TestClock::at(kst(10, 0)))).await;
        let status = wait_ai_checked(app.local_addr()).await;
        assert_eq!(
            status["ai"]["cli"],
            json!(CLI_WORK_NOT_EMPTY),
            "{other}: 다른 항목이 있으면 7차처럼 막는다: {status}"
        );
        assert!(!registered(&status, ITEM_AI), "{other}: {status}");
        stop(app).await;
        assert!(
            fake.calls().is_empty(),
            "{other}: CLI를 띄우지 않는다: {:?}",
            fake.calls()
        );
        assert_eq!(
            std::fs::read_to_string(work.join(other)).unwrap(),
            "남겨 둘 것",
            "{other}: 다른 항목을 지우지 않는다"
        );
        assert_eq!(
            std::fs::read_to_string(work.join(LEFTOVERS[0])).unwrap(),
            "남은 시스템 글",
            "{other}: 막을 때는 자기 임시 파일도 지우지 않는다"
        );
        let mut names: Vec<String> = std::fs::read_dir(&work)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        let mut expected = vec![LEFTOVERS[0].to_string(), other.to_string()];
        expected.sort();
        assert_eq!(names, expected, "{other}: 남은 것이 모두 그대로다");
    }
}

// 검증: C3, C16, V46, E4
#[tokio::test]
async fn 호출이_도는_중에_다시_고르면_그_임시_파일을_지우지_않고_비어_있음으로_본다() {
    let dir = tempfile::tempdir().unwrap();
    let fake = FakeCli::new(true, 0);
    fake.choose_in(dir.path());
    let app = start_app(settings(dir.path(), &[]), wiring(TestClock::at(kst(10, 0)))).await;
    let addr = app.local_addr();
    let status = wait_ai_checked(addr).await;
    assert_eq!(status["ai"]["cli"], json!(CLI_LOGGED_IN), "{status}");
    let work = dir.path().join(CLI_WORK_DIR);
    assert!(
        own_files(&work).is_empty(),
        "시작한 뒤 작업 폴더는 비어 있다"
    );

    // 시스템 글이 있는 호출은 작업 폴더에 자기 임시 파일을 두고 돈다.
    let provider = app.ai_provider();
    let request = CompletionRequest {
        model: "test-model".to_string(),
        system: Some("시험용 시스템 글".to_string()),
        messages: vec![pi_ai::Message {
            role: Role::User,
            content: "안녕".to_string(),
        }],
        max_tokens: 16,
    };
    let call = tokio::spawn(async move { provider.complete(request).await });
    let deadline = Instant::now() + STEP;
    let pid = loop {
        if let Some(pid) = fake.model_pid() {
            break pid;
        }
        assert!(Instant::now() < deadline, "모델 호출이 자식을 띄워야 한다");
        tokio::time::sleep(Duration::from_millis(50)).await;
    };
    let _cleanup = KillOnDrop(pid);
    let during = own_files(&work);
    assert_eq!(during.len(), 1, "도는 호출의 임시 파일이 있다: {during:?}");

    // 호출이 도는 중에 같은 CLI를 다시 고른다.
    let reply = post_json(addr, SETUP_AI_PATH, &fake.choose_body()).await;
    assert_eq!(
        reply.status, 200,
        "자기 임시 파일만 있으면 비어 있는 것으로 보고 고르기를 받는다: {}",
        reply.text
    );
    let status = wait_ai_checked(addr).await;
    assert_eq!(
        status["ai"]["cli"],
        json!(CLI_LOGGED_IN),
        "작업 폴더가 비어 있지 않다고 막지 않는다: {status}"
    );
    assert!(registered(&status, ITEM_AI), "{status}");
    assert_eq!(
        own_files(&work),
        during,
        "호출이 도는 중에 다시 골랐으면 그 임시 파일을 지우지 않는다"
    );

    stop(app).await;
    let result = tokio::time::timeout(STEP, call)
        .await
        .expect("끈 뒤 호출이 돌아와야 한다")
        .expect("호출 작업이 끝나야 한다");
    assert!(result.is_err(), "끝낸 자식의 호출은 실패로 돌아온다");
}

// ───────────── 2. 저장된 종목 파일에 지금 규칙에 맞지 않는 종목 ─────────────

// 검증: N8, E4, V31, V32, E3
#[tokio::test]
async fn 저장된_종목이_지금_규칙에_맞지_않으면_시작_오류에_파일_이름이_있고_값은_없고_파일은_그대로다()
 {
    let marker = "ZQ_MARK_8";
    assert!(
        pi_market::Symbol::parse(marker).is_err(),
        "시험 준비: 지금 규칙에 맞지 않는 종목이다"
    );
    for list in [[marker].as_slice(), ["005930", marker].as_slice()] {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join(WATCHLIST_FILE);
        let saved = json!({ "symbols": list }).to_string().into_bytes();
        std::fs::write(&file, &saved).unwrap();

        let started = tokio::time::timeout(
            STEP,
            App::start(
                settings_without_symbols(dir.path(), &[]),
                wiring(TestClock::at(kst(10, 0))),
            ),
        )
        .await
        .expect("시간 안에 끝나야 한다");
        let error = match started {
            Ok(app) => {
                stop(app).await;
                panic!("{list:?}: 규칙에 맞지 않는 종목이 저장돼 있으면 띄우지 않는다");
            }
            Err(error) => error,
        };
        let text = error.to_string();
        assert!(
            text.contains(WATCHLIST_FILE),
            "{list:?}: 어느 파일인지 알린다: {text}"
        );
        assert!(
            !text.contains(marker),
            "{list:?}: 종목 값은 넣지 않는다: {text}"
        );
        assert!(
            !format!("{error:?}").contains(marker),
            "{list:?}: 디버그 글에도 값이 없다"
        );
        assert_eq!(
            std::fs::read(&file).unwrap(),
            saved,
            "{list:?}: 파일을 고치거나 지우지 않는다(바이트 그대로)"
        );
    }
}

// ───────────── 3. 꺼지는 중에 AI 고르기가 끼어도 공급자가 남지 않는다 ─────────────

/// 세션 쿠키와 같은 출처로 AI 고르기를 보낸다. 연결이 끊겨 응답이 없으면 `None`.
async fn choose(addr: SocketAddr, cookie: String, body: Value) -> Option<u16> {
    let request = reqwest::Client::new()
        .post(format!("{}{SETUP_AI_PATH}", origin_of(addr)))
        .header("Content-Type", "application/json")
        .header("Cookie", cookie)
        .header("Origin", origin_of(addr))
        .body(body.to_string());
    match tokio::time::timeout(STEP, request.send()).await {
        Ok(Ok(reply)) => Some(reply.status().as_u16()),
        _ => None,
    }
}

// 검증: D2, C15, D11, C2, M6, E38, V46, V6
#[tokio::test]
async fn 꺼지기_시작한_뒤의_ai_고르기는_거부되고_cli_공급자가_남지_않는다() {
    let dir = tempfile::tempdir().unwrap();
    // 첫 고르기의 CLI는 버전 확인이 2초 걸린다(그동안 고르기 잠금을 쥔다).
    let first = FakeCli::new(false, 2);
    let second = FakeCli::new(false, 0);
    let app = start_app(settings(dir.path(), &[]), wiring(TestClock::at(kst(10, 0)))).await;
    let addr = app.local_addr();
    let cookie = login(addr).await;

    // 첫 고르기가 잠금을 쥐고 CLI를 확인하는 중이 될 때까지 기다린다.
    let first_reply = tokio::spawn(choose(addr, cookie.clone(), first.choose_body()));
    let deadline = Instant::now() + STEP;
    while !first.calls().iter().any(|call| call.contains("--version")) {
        assert!(Instant::now() < deadline, "첫 고르기가 CLI를 확인해야 한다");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    // 둘째 고르기는 본문까지 받아져 잠금을 기다린다(꺼지기 시작하기 전에 들어온 요청이다).
    let second_reply = tokio::spawn(choose(addr, cookie, second.choose_body()));
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(
        second.calls().is_empty(),
        "시험 준비: 둘째 고르기는 아직 잠금을 기다린다"
    );

    // 끄기를 시작한다. 둘째 고르기가 잠금을 얻는 것은 끄기가 시작된 뒤다.
    stop(app).await;
    let second_status = second_reply.await.expect("고르기 작업이 끝나야 한다");
    assert!(
        !second_status.is_some_and(|code| (200..300).contains(&code)),
        "꺼지기 시작한 뒤에 잠금을 얻은 고르기는 거부한다: {second_status:?}"
    );
    let _ = first_reply.await;

    tokio::time::sleep(QUIET).await;
    assert!(
        second.calls().is_empty(),
        "꺼지기 시작한 뒤의 고르기로 CLI 공급자가 생겨 CLI를 띄우지 않는다: {:?}",
        second.calls()
    );
    let saved = load_ai_choice(dir.path()).expect("고른 것 파일은 읽을 수 있어야 한다");
    assert_eq!(
        saved.and_then(|choice| choice.command),
        Some(first.bin.clone()),
        "거부된 고르기는 {AI_CHOICE_FILE}에 남지 않는다(끄기 전에 받은 고르기 그대로)"
    );
}
