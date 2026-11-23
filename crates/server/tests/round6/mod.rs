//! 6차 작업 1~8: AI 연결을 "API 키 또는 Claude Code CLI(구독)" 가운데 사용자가 고른 하나로 잇는다.
//!
//! 근거: `docs/dev/modules/gate.md` "6차 작업", `docs/dev/decisions.md` 2026-11-15, `dev.md` D4,
//! `docs/research/cli-backend.md` 5.7. CLI 확인은 `pi-ai`의 `ClaudeCliProvider::check`(버전 → 로그인)다.
//!
//! 정한 모양(뼈대 `price_inha`의 `ai_link` 모듈 문서와 같다):
//! - `POST /api/setup/ai`(세션 · 같은 출처 · 그 컴퓨터): `{"method":"api_key"}` 또는
//!   `{"method":"cli","command":"<절대 경로>","config_dir":"<절대 경로>"|""|null}`.
//!   갖춰졌으면 200 `{"registered":true}`, API 키를 골랐는데 키가 없으면 200 `{"registered":false}`,
//!   CLI 확인이 로그인된 구독이 아니면 `{"error": 까닭}`(422 · 확인하지 못함은 502).
//! - 설정 상태의 `ai` 칸: `{"method","command","config_dir","cli","cli_version","subscription","legacy_auth_file"}`.
//! - 고른 것은 보관 폴더의 `ai.json`(비밀이 아니다). `.env` · `auth.json`에 넣지 않는다.
//!
//! CLI는 실제 `claude`를 부르지 않는다. 임시 폴더에 쓴 가짜 셸 스크립트를 실행 파일 경로로 준다.
//! 사용자의 `~/.claude`나 실제 파일을 열지 않는다. macOS에서는 새로 쓴 스크립트의 첫 실행이 느릴 수 있어
//! 시간 예산을 넉넉히 잡는다.

use std::collections::BTreeMap;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::PathBuf;
use std::time::Instant;

use super::setup::{assert_registered, latest, post_json, setup_status};
use super::*;
use pi_ai::{AiError, CliStatus, CliVersion, CompletionRequest, Role};
use price_inha::{
    AI_CHOICE_FILE, AiChoice, AiMethod, CLI_COMMON_LOCATIONS, CLI_CONCURRENCY, CLI_ENV_ALLOWLIST,
    CLI_LOGGED_IN, CLI_NOT_INSTALLED, CLI_NOT_LOGGED_IN, CLI_NOT_SUBSCRIPTION, CLI_TERMS_NOTICE,
    CLI_TOO_OLD, CLI_UNCHECKED, CLI_USER_LOCATION, CLI_WORK_DIR, DEFAULT_REGISTER_ATTEMPTS,
    ITEM_AI, KEY_AI_API_KEY, KIND_SETUP, LEGACY_AUTH_FILE, LEGACY_AUTH_NOTICE, SETUP_AI_PATH,
    SETUP_KEYS_PATH, SETUP_REQUIRED, SETUP_STARTED, SetupRefusal, cli_base_env, cli_config,
    cli_refusal, cli_state, find_claude, legacy_auth_file, prepare_cli_work_dir,
};
use tempfile::TempDir;

// ───────────── 도우미 ─────────────

/// 가짜 CLI가 알리는 버전(쓸 수 있는 가장 낮은 버전 2.1.259보다 높다).
const CLI_VERSION_TEXT: &str = "2.1.316 (Claude Code)";
/// 옛 구독 토큰 파일에 넣는 시험용 내용.
const LEGACY_CONTENT: &str = r#"{"access_token":"test-legacy-token-not-real"}"#;
const LEGACY_SECRET: &str = "test-legacy-token-not-real";
/// 서버 환경에 있지만 CLI에 넘기면 안 되는 시험용 값.
const SERVER_ENV_SECRET: &str = "test-server-env-secret-not-real";
const ANTHROPIC_ENV_SECRET: &str = "test-anthropic-env-not-real";

/// 가짜 CLI의 동작(셸 글).
struct Behave {
    version: String,
    auth: String,
    model: String,
}

impl Behave {
    /// 설치돼 있고 구독(`claude.ai`, `max`)으로 로그인돼 있다. 모델 호출은 실패로 끝난다.
    fn logged_in() -> Self {
        Self {
            version: say(CLI_VERSION_TEXT, 0),
            auth: say(&auth_json(true, "claude.ai"), 0),
            model: "exit 1".to_string(),
        }
    }

    fn with_version(mut self, text: &str) -> Self {
        self.version = say(text, 0);
        self
    }

    fn with_auth(mut self, text: &str, code: i32) -> Self {
        self.auth = say(text, code);
        self
    }

    fn with_model(mut self, shell: &str) -> Self {
        self.model = shell.to_string();
        self
    }
}

/// 한 줄을 찍고 끝 코드로 끝내는 셸 글.
fn say(text: &str, code: i32) -> String {
    format!("printf '%s\\n' '{text}'\nexit {code}")
}

/// `auth status --json`의 출력(공식 CLI의 모양, `cli-backend.md` 5.7).
fn auth_json(logged_in: bool, method: &str) -> String {
    json!({
        "loggedIn": logged_in,
        "authMethod": method,
        "apiProvider": "firstParty",
        "subscriptionType": "max",
    })
    .to_string()
}

/// 임시 폴더 하나에 든 가짜 CLI와 그 기록.
struct FakeCli {
    _dir: TempDir,
    /// 실제 경로로 바꾼 임시 폴더.
    root: PathBuf,
    /// 가짜가 기록을 남기는 폴더.
    log: PathBuf,
    /// 가짜 실행 파일.
    bin: PathBuf,
    /// 전용 로그인 폴더로 줄 빈 폴더.
    login: PathBuf,
}

impl FakeCli {
    fn new(behave: &Behave) -> Self {
        let dir = tempfile::tempdir().unwrap();
        // macOS의 임시 폴더는 다른 경로를 거치므로 실제 경로로 바꿔 둔다.
        let root = std::fs::canonicalize(dir.path()).unwrap();
        let log = root.join("log");
        let login = root.join("login");
        let bin_dir = root.join("bin");
        for d in [&log, &login, &bin_dir] {
            std::fs::create_dir(d).unwrap();
        }
        let bin = bin_dir.join("claude");
        std::fs::write(&bin, script(&log, behave)).unwrap();
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
        Self {
            _dir: dir,
            root,
            log,
            bin,
            login,
        }
    }

    fn bin_str(&self) -> &str {
        self.bin.to_str().unwrap()
    }

    fn read(&self, name: &str) -> String {
        std::fs::read_to_string(self.log.join(name)).unwrap_or_default()
    }

    /// 가짜가 불린 인자 줄들(`--version` · `auth status --json` · 모델 호출).
    fn calls(&self) -> Vec<String> {
        self.read("calls").lines().map(str::to_string).collect()
    }

    fn version_calls(&self) -> usize {
        self.calls().iter().filter(|c| *c == "--version").count()
    }

    /// 모델을 부른 호출들(버전 · 로그인 확인이 아닌 것).
    fn model_calls(&self) -> Vec<String> {
        self.calls()
            .into_iter()
            .filter(|c| c != "--version" && !c.starts_with("auth"))
            .collect()
    }

    /// 로그인 확인 때 받은 환경.
    fn auth_env(&self) -> BTreeMap<String, String> {
        env_map(&self.read("auth_env"))
    }
}

/// 가짜 실행 파일의 셸 글. 부모 환경이 없어도 돌도록 명령은 절대 경로로 부른다.
fn script(log: &Path, b: &Behave) -> String {
    let log = log.display();
    format!(
        r#"#!/bin/sh
L='{log}'
printf '%s\n' "$*" >> "$L/calls"
case "$1" in
--version)
{version}
;;
auth)
/usr/bin/env > "$L/auth_env"
/bin/pwd -P > "$L/auth_cwd"
{auth}
;;
esac
/usr/bin/env > "$L/env"
/bin/cat > /dev/null
{model}
"#,
        version = b.version,
        auth = b.auth,
        model = b.model,
    )
}

/// `env` 출력을 이름 → 값으로 읽는다.
fn env_map(text: &str) -> BTreeMap<String, String> {
    text.lines()
        .filter_map(|line| line.split_once('='))
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

/// CLI를 고르는 본문.
fn cli_body(command: &str, config_dir: Option<&str>) -> Value {
    json!({ "method": "cli", "command": command, "config_dir": config_dir })
}

fn api_key_choice() -> Value {
    json!({ "method": "api_key" })
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

/// 세션을 받아 같은 출처에서 글자 본문을 그대로 보낸다.
async fn post_text(addr: SocketAddr, path: &str, body: String) -> Reply {
    let cookie = login(addr).await;
    let request = reqwest::Client::new()
        .post(format!("{}{path}", origin_of(addr)))
        .header("Content-Type", "application/json")
        .header("Cookie", cookie)
        .header("Origin", origin_of(addr))
        .body(body);
    let reply = tokio::time::timeout(STEP, request.send())
        .await
        .expect("응답이 시간 안에 와야 한다")
        .expect("요청이 닿아야 한다");
    let status = reply.status().as_u16();
    let text = reply.text().await.expect("본문을 읽어야 한다");
    Reply { status, text }
}

/// AI 연결을 고른다.
async fn choose(addr: SocketAddr, body: &Value) -> Reply {
    post_text(addr, SETUP_AI_PATH, body.to_string()).await
}

/// 거부 응답은 상태와 까닭뿐이다.
fn assert_refused(reply: &Reply, status: u16, reason: &str) {
    assert_eq!(reply.status, status, "{reason}: {}", reply.text);
    assert_eq!(
        reply.json(),
        json!({ "error": reason }),
        "실패는 까닭만 알린다"
    );
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

/// 설정 상태의 `ai` 칸.
async fn ai_status(addr: SocketAddr) -> Value {
    let status = setup_status(addr).await;
    let ai = status["ai"].clone();
    assert!(ai.is_object(), "설정 상태에 ai 칸이 있어야 한다: {status}");
    ai
}

/// 등록 시도 제한을 넉넉히 잡은 설정(여러 경우를 한 서버에서 차례로 확인할 때).
fn roomy(home: &Path) -> Settings {
    settings(home, &["--register-attempts", "100"])
}

/// 환경 변수를 받은 설정. 프로세스 환경은 읽지 않는다.
fn settings_with_env(home: &Path, extra: &[&str], env: &[(&str, &str)]) -> Settings {
    let home = home.to_str().unwrap();
    let mut list = vec![
        "--home",
        home,
        "--port",
        "0",
        "--interval-ms",
        INTERVAL_MS,
        "--symbols",
        "005930,000660",
    ];
    list.extend_from_slice(extra);
    let env: BTreeMap<String, String> = env
        .iter()
        .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
        .collect();
    let lookup = move |name: &str| env.get(name).cloned();
    Settings::parse(&args(&list), &lookup, None).expect("설정을 읽어야 한다")
}

/// 보관 폴더의 비밀 보관소에 아무것도 등록되지 않았다.
fn no_secrets(home: &Path) -> bool {
    let secrets = SecretStore::open(home).unwrap();
    secrets.status().unwrap().iter().all(|s| !s.registered)
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

/// 가짜 Anthropic이 받은 대화 요청(`POST /v1/messages`)의 수.
async fn messages_requests(ai: &FakeAi) -> usize {
    ai.server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .filter(|r| r.method.as_str() == "POST" && r.url.path() == "/v1/messages")
        .count()
}

// ───────────── 1. AI 줄: API 키 또는 CLI, 구독 토큰 길 없음 ─────────────

// 검증: C3, V46
#[tokio::test]
async fn ai_연결의_기본은_api_키이고_고르면_고른_것을_비밀이_아닌_파일에_둔다() {
    let toss = FakeToss::start().await;
    let ai = FakeAi::start().await;
    let dir = tempfile::tempdir().unwrap();
    let app = start_app(
        settings(dir.path(), &[]),
        wiring_both(&toss, &ai, TestClock::at(kst(10, 0, 0))),
    )
    .await;
    let addr = app.local_addr();

    // 고른 것이 없으면 API 키다. 구독 토큰을 가리키는 칸은 없다.
    let ai_now = ai_status(addr).await;
    assert_eq!(ai_now["method"], json!("api_key"), "{ai_now}");
    assert_eq!(ai_now["cli"], Value::Null, "CLI를 고르지 않았다");
    assert_eq!(ai_now["legacy_auth_file"], json!(false));
    let text = setup_status(addr).await.to_string().to_ascii_lowercase();
    assert!(
        !text.contains("oauth") && !text.contains("access_token"),
        "설정 상태에 구독 토큰 자리가 없다: {text}"
    );

    // API 키를 골랐지만 키가 아직 없다.
    let reply = choose(addr, &api_key_choice()).await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    assert_eq!(reply.json(), json!({ "registered": false }));
    let saved: Value =
        serde_json::from_slice(&std::fs::read(dir.path().join(AI_CHOICE_FILE)).unwrap()).unwrap();
    assert_eq!(
        saved,
        json!({ "method": "api_key" }),
        "고른 것은 비밀이 아닌 파일에 둔다"
    );
    assert!(!registered(&setup_status(addr).await, ITEM_AI));

    // 키를 등록하면 갖춰진다.
    let reply = post_json(
        addr,
        SETUP_KEYS_PATH,
        &json!({ "item": KEY_AI_API_KEY, "fields": { "api_key": AI_KEY } }),
    )
    .await;
    assert_registered(&reply);
    let reply = choose(addr, &api_key_choice()).await;
    assert_eq!(reply.json(), json!({ "registered": true }));
    assert!(registered(&setup_status(addr).await, ITEM_AI));
    assert_eq!(
        toss.total_requests().await,
        0,
        "AI 연결을 고르는 것이 증권사를 두드리지 않는다"
    );
    stop(app).await;
}

// ───────────── 2. CLI를 고르면: 경로 · 로그인 폴더 · 저장 ─────────────

// 검증: C3, C16, V46
#[tokio::test]
async fn cli를_고르면_방식과_경로와_로그인_폴더를_종목처럼_저장하고_다시_켜도_남는다() {
    let toss = FakeToss::start().await;
    let ai = FakeAi::start().await;
    let fake = FakeCli::new(&Behave::logged_in());
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path();
    let app = start_app(
        roomy(home),
        wiring_both(&toss, &ai, TestClock::at(kst(10, 0, 0))),
    )
    .await;
    let addr = app.local_addr();
    let login_dir = fake.login.to_str().unwrap();

    let reply = choose(addr, &cli_body(fake.bin_str(), Some(login_dir))).await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    assert_eq!(reply.json(), json!({ "registered": true }));

    let saved: Value =
        serde_json::from_slice(&std::fs::read(home.join(AI_CHOICE_FILE)).unwrap()).unwrap();
    assert_eq!(
        saved,
        json!({ "method": "cli", "command": fake.bin_str(), "config_dir": login_dir }),
        "고른 방식 · 경로 · 로그인 폴더를 비밀이 아닌 파일에 둔다"
    );
    let env_file = home.join(pi_secrets::ENV_FILE);
    if env_file.exists() {
        let env_text = std::fs::read_to_string(&env_file).unwrap();
        assert!(!env_text.contains(fake.bin_str()), "키 파일에 넣지 않는다");
    }
    assert!(
        !home.join(LEGACY_AUTH_FILE).exists(),
        "auth.json을 만들지 않는다"
    );
    assert!(no_secrets(home), "CLI를 고른 것은 키가 아니다");

    let ai_now = ai_status(addr).await;
    assert_eq!(ai_now["method"], json!("cli"), "{ai_now}");
    assert_eq!(ai_now["command"], json!(fake.bin_str()));
    assert_eq!(ai_now["config_dir"], json!(login_dir));
    assert_eq!(ai_now["cli"], json!(CLI_LOGGED_IN));
    assert_eq!(ai_now["cli_version"], json!("2.1.316"));
    assert_eq!(ai_now["subscription"], json!("max"));
    assert!(registered(&setup_status(addr).await, ITEM_AI));
    assert_eq!(
        fake.auth_env().get("CLAUDE_CONFIG_DIR").map(String::as_str),
        Some(login_dir),
        "전용 로그인 폴더를 확인에도 쓴다"
    );
    stop(app).await;

    // 다시 켜도 고른 것이 남고, 띄울 때 다시 확인한다(모델은 부르지 않는다).
    let before = fake.version_calls();
    let app = start_app(
        roomy(home),
        wiring_both(&toss, &ai, TestClock::at(kst(10, 0, 0))),
    )
    .await;
    let addr = app.local_addr();
    let ai_now = ai_status(addr).await;
    assert_eq!(ai_now["method"], json!("cli"), "{ai_now}");
    assert_eq!(ai_now["command"], json!(fake.bin_str()));
    assert_eq!(ai_now["config_dir"], json!(login_dir));
    assert_eq!(ai_now["cli"], json!(CLI_LOGGED_IN));
    assert!(fake.version_calls() > before, "띄울 때 고른 CLI를 확인한다");
    assert!(fake.model_calls().is_empty(), "확인은 모델을 부르지 않는다");

    // 로그인 폴더를 비우면 평소 로그인이다.
    let reply = choose(addr, &cli_body(fake.bin_str(), Some(""))).await;
    assert_eq!(
        reply.json(),
        json!({ "registered": true }),
        "{}",
        reply.text
    );
    let saved: Value =
        serde_json::from_slice(&std::fs::read(home.join(AI_CHOICE_FILE)).unwrap()).unwrap();
    assert_eq!(saved["config_dir"], Value::Null, "{saved}");
    assert_eq!(ai_status(addr).await["config_dir"], Value::Null);
    assert!(
        !fake.auth_env().contains_key("CLAUDE_CONFIG_DIR"),
        "평소 로그인이면 로그인 폴더를 넘기지 않는다"
    );
    stop(app).await;
}

// 검증: C3
#[test]
fn cli_실행_파일은_path_다음_사용자_홈_다음_흔한_위치_순으로_찾는다() {
    assert_eq!(
        CLI_COMMON_LOCATIONS,
        ["/opt/homebrew/bin/claude", "/usr/local/bin/claude"]
    );
    assert_eq!(CLI_USER_LOCATION, ".local/bin/claude");
    let defaults: Vec<PathBuf> = CLI_COMMON_LOCATIONS.iter().map(PathBuf::from).collect();
    assert_eq!(Wiring::default().cli_locations, defaults);

    let dir = tempfile::tempdir().unwrap();
    let root = std::fs::canonicalize(dir.path()).unwrap();
    let make = |path: &Path, mode: u32| {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, "#!/bin/sh\nexit 0\n").unwrap();
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).unwrap();
    };
    // a: 실행할 수 없는 파일, c: 같은 이름의 폴더, b: 실행 파일.
    make(&root.join("a/claude"), 0o644);
    std::fs::create_dir_all(root.join("c/claude")).unwrap();
    make(&root.join("b/claude"), 0o755);
    let user_home = root.join("home");
    make(&user_home.join(".local/bin/claude"), 0o755);
    let missing = root.join("nowhere/claude");
    let fixed = root.join("fixed/claude");
    make(&fixed, 0o755);
    let path_var = |dirs: &[&str]| {
        dirs.iter()
            .map(|d| root.join(d).to_str().unwrap().to_string())
            .collect::<Vec<_>>()
            .join(":")
    };
    let fixed_list = [missing.clone(), fixed.clone()];

    assert_eq!(
        find_claude(
            Some(&path_var(&["a", "c", "b"])),
            Some(&user_home),
            &fixed_list
        ),
        Some(root.join("b/claude")),
        "PATH에서 실행할 수 있는 파일이 먼저다"
    );
    assert_eq!(
        find_claude(Some(&path_var(&["a", "c"])), Some(&user_home), &fixed_list),
        Some(user_home.join(".local/bin/claude")),
        "PATH에 없으면 사용자 홈의 ~/.local/bin/claude"
    );
    assert_eq!(
        find_claude(Some(&path_var(&["a"])), None, &fixed_list),
        Some(fixed.clone()),
        "그다음은 흔한 위치를 순서대로"
    );
    assert_eq!(find_claude(None, None, &fixed_list), Some(fixed));
    assert_eq!(
        find_claude(Some(""), Some(&root.join("empty-home")), &[missing]),
        None,
        "어디에도 없으면 없다"
    );
}

// 검증: C3
#[tokio::test]
async fn 설정_상태는_서버가_찾은_cli_경로를_채워_준다() {
    let fake = FakeCli::new(&Behave::logged_in());
    let bin_dir = fake.root.join("bin");
    let dir = tempfile::tempdir().unwrap();
    let settings = settings_with_env(dir.path(), &[], &[("PATH", bin_dir.to_str().unwrap())]);
    let app = start_app(settings, wiring(None, TestClock::at(kst(10, 0, 0)))).await;
    let ai_now = ai_status(app.local_addr()).await;
    assert_eq!(
        ai_now["method"],
        json!("api_key"),
        "찾았다고 고른 것은 아니다"
    );
    assert_eq!(
        ai_now["command"],
        json!(fake.bin_str()),
        "PATH에서 찾은 경로로 칸을 채운다: {ai_now}"
    );
    assert_eq!(ai_now["cli"], Value::Null);
    stop(app).await;

    // 어디에도 없으면 비어 있다(테스트의 찾는 자리는 비어 있다).
    let dir = tempfile::tempdir().unwrap();
    let app = start_app(
        settings_with_env(dir.path(), &[], &[]),
        wiring(None, TestClock::at(kst(10, 0, 0))),
    )
    .await;
    assert_eq!(ai_status(app.local_addr()).await["command"], Value::Null);
    stop(app).await;
}

// ───────────── 3. CLI의 "등록됨" ─────────────

// 검증: C3, E4
#[test]
fn cli_확인_결과는_상태_글자와_까닭으로_옮긴다() {
    let version = CliVersion {
        major: 2,
        minor: 1,
        patch: 316,
    };
    let cases = [
        (
            CliStatus::LoggedIn {
                version,
                auth_method: "claude.ai".to_string(),
                subscription_type: Some("max".to_string()),
            },
            CLI_LOGGED_IN,
            None,
        ),
        (
            CliStatus::NotInstalled,
            CLI_NOT_INSTALLED,
            Some(SetupRefusal::CliNotInstalled),
        ),
        (
            CliStatus::TooOld {
                version: CliVersion {
                    major: 2,
                    minor: 1,
                    patch: 100,
                },
            },
            CLI_TOO_OLD,
            Some(SetupRefusal::CliTooOld),
        ),
        (
            CliStatus::NotLoggedIn,
            CLI_NOT_LOGGED_IN,
            Some(SetupRefusal::CliNotLoggedIn),
        ),
        (
            CliStatus::NotSubscription {
                auth_method: "api_key".to_string(),
            },
            CLI_NOT_SUBSCRIPTION,
            Some(SetupRefusal::CliNotSubscription),
        ),
        (
            CliStatus::TimedOut,
            CLI_UNCHECKED,
            Some(SetupRefusal::CliUnchecked),
        ),
        (
            CliStatus::Unreadable,
            CLI_UNCHECKED,
            Some(SetupRefusal::CliUnchecked),
        ),
    ];
    for (status, state, refusal) in cases {
        assert_eq!(cli_state(&status), state, "{status:?}");
        assert_eq!(cli_refusal(&status), refusal, "{status:?}");
    }
    let reasons = [
        (SetupRefusal::CliNotInstalled, "cli_not_installed", 422),
        (SetupRefusal::CliTooOld, "cli_too_old", 422),
        (SetupRefusal::CliNotLoggedIn, "cli_not_logged_in", 422),
        (
            SetupRefusal::CliNotSubscription,
            "cli_not_subscription",
            422,
        ),
        (SetupRefusal::CliUnchecked, "cli_unchecked", 502),
    ];
    for (refusal, reason, status) in reasons {
        assert_eq!((refusal.reason(), refusal.status()), (reason, status));
    }
}

// 검증: C3, E4, V46
#[tokio::test]
async fn cli는_로그인된_구독일_때만_ai_연결이_갖춰지고_아니면_까닭을_알린다() {
    let toss = FakeToss::start().await;
    let ai = FakeAi::start().await;
    let dir = tempfile::tempdir().unwrap();
    let app = start_app(
        roomy(dir.path()),
        wiring_both(&toss, &ai, TestClock::at(kst(10, 0, 0))),
    )
    .await;
    let addr = app.local_addr();

    // 로그인된 구독이면 갖춰진다.
    let fake = FakeCli::new(&Behave::logged_in());
    let reply = choose(addr, &cli_body(fake.bin_str(), None)).await;
    assert_eq!(
        reply.json(),
        json!({ "registered": true }),
        "{}",
        reply.text
    );
    assert!(registered(&setup_status(addr).await, ITEM_AI));
    assert_eq!(
        fake.calls(),
        ["--version", "auth status --json"],
        "버전 → 로그인 순으로 확인하고 모델을 부르지 않는다"
    );

    // 나머지는 등록되지 않음과 까닭이다(앞서 갖춰졌어도 지금 확인이 아니면 갖춰지지 않은 것이다).
    let not_logged_in = auth_json(false, "claude.ai");
    let api_login = auth_json(true, "api_key");
    let cases: [(Behave, u16, &str, &str); 6] = [
        (
            Behave::logged_in().with_version("2.1.100 (Claude Code)"),
            422,
            "cli_too_old",
            CLI_TOO_OLD,
        ),
        (
            Behave::logged_in().with_auth(&not_logged_in, 1),
            422,
            "cli_not_logged_in",
            CLI_NOT_LOGGED_IN,
        ),
        (
            Behave::logged_in().with_auth(&not_logged_in, 0),
            422,
            "cli_not_logged_in",
            CLI_NOT_LOGGED_IN,
        ),
        (
            Behave::logged_in().with_auth(&api_login, 0),
            422,
            "cli_not_subscription",
            CLI_NOT_SUBSCRIPTION,
        ),
        (
            Behave::logged_in().with_auth("not json", 0),
            502,
            "cli_unchecked",
            CLI_UNCHECKED,
        ),
        (
            Behave::logged_in().with_version("garbage"),
            502,
            "cli_unchecked",
            CLI_UNCHECKED,
        ),
    ];
    for (behave, status, reason, state) in cases {
        let fake = FakeCli::new(&behave);
        let reply = choose(addr, &cli_body(fake.bin_str(), None)).await;
        assert_refused(&reply, status, reason);
        let ai_now = ai_status(addr).await;
        assert_eq!(ai_now["method"], json!("cli"), "{reason}: {ai_now}");
        assert_eq!(ai_now["cli"], json!(state), "{reason}: {ai_now}");
        assert_eq!(ai_now["cli_version"], Value::Null, "{reason}");
        assert!(
            !registered(&setup_status(addr).await, ITEM_AI),
            "{reason}: 등록되지 않은 것이다"
        );
        assert!(
            fake.model_calls().is_empty(),
            "{reason}: 모델을 부르지 않는다"
        );
    }

    // 실행 파일이 그 경로에 없다.
    let fake = FakeCli::new(&Behave::logged_in());
    let gone = fake.root.join("bin/not-here/claude");
    let reply = choose(addr, &cli_body(gone.to_str().unwrap(), None)).await;
    assert_refused(&reply, 422, "cli_not_installed");
    assert_eq!(ai_status(addr).await["cli"], json!(CLI_NOT_INSTALLED));
    assert!(!registered(&setup_status(addr).await, ITEM_AI));
    assert_eq!(
        (toss.total_requests().await, messages_requests(&ai).await),
        (0, 0),
        "CLI 확인은 바깥 서버를 부르지 않는다"
    );
    stop(app).await;
}

// 검증: C3, E4
#[tokio::test]
async fn cli_확인도_등록_시도_횟수_제한을_받는다() {
    let dir = tempfile::tempdir().unwrap();
    let app = start_app(
        settings(dir.path(), &["--register-attempts", "2"]),
        wiring(None, TestClock::at(kst(10, 0, 0))),
    )
    .await;
    let addr = app.local_addr();
    let fake = FakeCli::new(&Behave::logged_in().with_auth(&auth_json(false, "claude.ai"), 1));
    for attempt in 1..=2 {
        let reply = choose(addr, &cli_body(fake.bin_str(), None)).await;
        assert_refused(&reply, 422, "cli_not_logged_in");
        assert_eq!(fake.version_calls(), attempt, "횟수 안에서는 확인한다");
    }
    let reply = choose(addr, &cli_body(fake.bin_str(), None)).await;
    assert_refused(&reply, 429, "too_many_attempts");
    assert_eq!(fake.version_calls(), 2, "횟수를 넘으면 CLI를 띄우지 않는다");
    stop(app).await;
}

// 검증: C3, E4
#[tokio::test]
async fn 잘못된_ai_연결_입력은_띄우기_전에_거부한다() {
    let dir = tempfile::tempdir().unwrap();
    let app = start_app(
        settings(dir.path(), &[]),
        wiring(None, TestClock::at(kst(10, 0, 0))),
    )
    .await;
    let addr = app.local_addr();
    let fake = FakeCli::new(&Behave::logged_in());
    let bin = fake.bin_str();
    let bad_bodies = [
        String::new(),
        "not json".to_string(),
        "[]".to_string(),
        json!({}).to_string(),
        json!({ "method": "oauth" }).to_string(),
        json!({ "method": "subscription_token", "access_token": "test-oauth-access-not-real" })
            .to_string(),
        json!({ "method": 1 }).to_string(),
        json!({ "method": "cli" }).to_string(),
        json!({ "method": "cli", "command": "" }).to_string(),
        json!({ "method": "cli", "command": "claude" }).to_string(),
        json!({ "method": "cli", "command": "bin/claude" }).to_string(),
        json!({ "method": "cli", "command": 7 }).to_string(),
        json!({ "method": "cli", "command": format!("{bin}\n") }).to_string(),
        json!({ "method": "cli", "command": bin, "config_dir": "login" }).to_string(),
        json!({ "method": "cli", "command": bin, "config_dir": 3 }).to_string(),
        json!({ "method": "cli", "command": bin, "args": ["--dangerously-skip-permissions"] })
            .to_string(),
        json!({ "method": "api_key", "api_key": AI_KEY }).to_string(),
    ];
    for body in &bad_bodies {
        let reply = post_text(addr, SETUP_AI_PATH, body.clone()).await;
        assert_refused(&reply, 400, "invalid_input");
        assert!(
            !reply.text.contains(AI_KEY),
            "받은 값을 되돌려 보이지 않는다"
        );
    }
    assert!(
        fake.calls().is_empty(),
        "잘못된 입력으로는 CLI를 띄우지 않는다"
    );
    assert!(
        !dir.path().join(AI_CHOICE_FILE).exists(),
        "잘못된 입력은 저장하지 않는다"
    );
    assert_eq!(ai_status(addr).await["method"], json!("api_key"));

    // 잘못된 입력은 시도 횟수를 쓰지 않는다: 바른 고르기는 확인된다.
    assert!(bad_bodies.len() > DEFAULT_REGISTER_ATTEMPTS as usize);
    let reply = choose(addr, &cli_body(bin, None)).await;
    assert_eq!(
        reply.json(),
        json!({ "registered": true }),
        "{}",
        reply.text
    );
    stop(app).await;
}

// 검증: V31, C16
#[tokio::test]
async fn cli를_고르면_api_키가_있어도_넘어가지_않는다() {
    let toss = FakeToss::start().await;
    let ai = FakeAi::start().await;
    let dir = tempfile::tempdir().unwrap();
    register_broker(dir.path(), &toss).await;
    register_ai(dir.path(), &ai).await;
    let app = start_app(
        roomy(dir.path()),
        wiring_both(&toss, &ai, TestClock::at(kst(10, 0, 0))),
    )
    .await;
    let addr = app.local_addr();
    let mut ws = connect(addr, 0).await;
    wait_price(&mut ws, "005930", "72000").await;

    // CLI를 골랐는데 로그인돼 있지 않다: API 키가 있어도 AI 연결은 갖춰지지 않은 것이다.
    let fake = FakeCli::new(&Behave::logged_in().with_auth(&auth_json(false, "claude.ai"), 1));
    let reply = choose(addr, &cli_body(fake.bin_str(), None)).await;
    assert_refused(&reply, 422, "cli_not_logged_in");
    let status = setup_status(addr).await;
    assert!(!registered(&status, ITEM_AI), "{status}");
    assert_eq!(status["ready"], json!(false));
    let notice = find_frame(&mut ws, STEP, |frame| {
        transient(frame, KIND_SETUP).filter(|n| n["state"] == json!(SETUP_REQUIRED))
    })
    .await
    .expect("고른 연결이 빠지면 멈추고 설정이 필요하다고 알린다");
    assert_eq!(notice["missing"], json!([ITEM_AI]));

    // 공급자도 고른 것(CLI)이다. API 키로 넘어가지 않는다.
    let before = messages_requests(&ai).await;
    let result = app.ai_provider().complete(tiny_request()).await;
    assert!(result.is_err(), "가짜 CLI의 모델 호출은 실패로 끝난다");
    assert_eq!(fake.model_calls().len(), 1, "고른 CLI를 부른다");
    assert_eq!(
        messages_requests(&ai).await,
        before,
        "API 키로 넘어가지 않는다"
    );

    // API 키를 다시 고르면 이어 간다.
    let reply = choose(addr, &api_key_choice()).await;
    assert_eq!(
        reply.json(),
        json!({ "registered": true }),
        "{}",
        reply.text
    );
    find_frame(&mut ws, STEP, |frame| {
        transient(frame, KIND_SETUP).filter(|n| n["state"] == json!(SETUP_STARTED))
    })
    .await
    .expect("API 키를 다시 고르면 이어 간다");
    stop(app).await;
}

// 검증: V31, V32, C3
#[tokio::test]
async fn cli가_마지막_필수_항목이면_확인되자마자_시작한다() {
    let toss = FakeToss::start().await;
    let ai = FakeAi::start().await;
    let dir = tempfile::tempdir().unwrap();
    register_broker(dir.path(), &toss).await;
    let app = start_app(
        settings(dir.path(), &[]),
        wiring_both(&toss, &ai, TestClock::at(kst(10, 0, 0))),
    )
    .await;
    let addr = app.local_addr();
    assert_eq!(latest(addr, KIND_SETUP).await["missing"], json!([ITEM_AI]));
    let mut ws = connect(addr, 0).await;

    let fake = FakeCli::new(&Behave::logged_in());
    let reply = choose(addr, &cli_body(fake.bin_str(), None)).await;
    assert_eq!(
        reply.json(),
        json!({ "registered": true }),
        "{}",
        reply.text
    );
    find_frame(&mut ws, STEP, |frame| {
        transient(frame, KIND_SETUP).filter(|n| n["state"] == json!(SETUP_STARTED))
    })
    .await
    .expect("CLI가 확인되면 다시 켜지 않아도 시작한다");
    wait_price(&mut ws, "005930", "72000").await;
    assert_eq!(
        messages_requests(&ai).await,
        0,
        "시작하는 것이 AI 모델을 부르지 않는다"
    );
    assert!(fake.model_calls().is_empty());
    stop(app).await;
}

// ───────────── 4. CLI에 넘기는 환경과 작업 폴더 ─────────────

// 검증: E38, N8
#[test]
fn cli에_넘길_환경은_허용_목록의_이름만_고른다() {
    assert_eq!(
        CLI_ENV_ALLOWLIST,
        [
            "HOME", "PATH", "LANG", "LC_ALL", "TMPDIR", "USER", "LOGNAME"
        ]
    );
    let env: BTreeMap<&str, &str> = [
        ("USER", "tester"),
        ("ANTHROPIC_API_KEY", ANTHROPIC_ENV_SECRET),
        ("HOME", "/home/tester"),
        ("PRICE_INHA_TEST_SECRET", SERVER_ENV_SECRET),
        ("LANG", "ko_KR.UTF-8"),
        ("PATH", "/usr/bin:/bin"),
        ("CLAUDE_CODE_OAUTH_TOKEN", SERVER_ENV_SECRET),
        ("TMPDIR", "/tmp/tester"),
        ("LOGNAME", "tester"),
    ]
    .into_iter()
    .collect();
    let lookup = |name: &str| env.get(name).map(|v| (*v).to_string());
    let expected: Vec<(String, String)> = [
        ("HOME", "/home/tester"),
        ("PATH", "/usr/bin:/bin"),
        ("LANG", "ko_KR.UTF-8"),
        ("TMPDIR", "/tmp/tester"),
        ("USER", "tester"),
        ("LOGNAME", "tester"),
    ]
    .iter()
    .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
    .collect();
    assert_eq!(
        cli_base_env(&lookup),
        expected,
        "허용 목록의 순서, 없는 것은 뺀다"
    );

    let dir = tempfile::tempdir().unwrap();
    let pairs: Vec<(&str, &str)> = env.iter().map(|(k, v)| (*k, *v)).collect();
    let settings = settings_with_env(dir.path(), &[], &pairs);
    assert_eq!(settings.cli_env, expected, "설정도 같은 것만 쥔다");
}

// 검증: N8
#[test]
fn cli_작업_폴더는_보관_폴더_아래_0700의_빈_전용_폴더다() {
    let dir = tempfile::tempdir().unwrap();
    let work = prepare_cli_work_dir(dir.path()).expect("작업 폴더를 만든다");
    assert_eq!(work, dir.path().join(CLI_WORK_DIR));
    let meta = std::fs::metadata(&work).unwrap();
    assert!(meta.is_dir());
    assert_eq!(meta.mode() & 0o777, 0o700, "본인만 쓰는 폴더다");
    assert_eq!(std::fs::read_dir(&work).unwrap().count(), 0, "비어 있다");

    // 이미 있는데 권한이 넓으면 좁힌다.
    std::fs::set_permissions(&work, std::fs::Permissions::from_mode(0o755)).unwrap();
    let again = prepare_cli_work_dir(dir.path()).unwrap();
    assert_eq!(again, work);
    assert_eq!(std::fs::metadata(&work).unwrap().mode() & 0o777, 0o700);
}

// 검증: E38, N8, V46
#[tokio::test]
async fn cli에는_허용된_환경만_넘기고_작업_폴더는_보관_폴더_아래_빈_전용_폴더다() {
    let toss = FakeToss::start().await;
    let dir = tempfile::tempdir().unwrap();
    let home = std::fs::canonicalize(dir.path()).unwrap();
    register_broker(&home, &toss).await;
    let user = tempfile::tempdir().unwrap();
    let user_home = user.path().to_str().unwrap().to_string();
    let env = [
        ("HOME", user_home.as_str()),
        ("PATH", "/usr/bin:/bin"),
        ("LANG", "ko_KR.UTF-8"),
        ("USER", "tester"),
        ("LOGNAME", "tester"),
        ("ANTHROPIC_API_KEY", ANTHROPIC_ENV_SECRET),
        ("PRICE_INHA_TEST_SECRET", SERVER_ENV_SECRET),
    ];
    let app = start_app(
        settings_with_env(&home, &[], &env),
        wiring(Some(&toss), TestClock::at(kst(10, 0, 0))),
    )
    .await;
    let addr = app.local_addr();
    let fake = FakeCli::new(&Behave::logged_in());
    let reply = choose(addr, &cli_body(fake.bin_str(), None)).await;
    assert_eq!(
        reply.json(),
        json!({ "registered": true }),
        "{}",
        reply.text
    );

    let seen = fake.auth_env();
    assert_eq!(seen.get("HOME"), Some(&user_home));
    assert_eq!(seen.get("LANG").map(String::as_str), Some("ko_KR.UTF-8"));
    assert_eq!(seen.get("USER").map(String::as_str), Some("tester"));
    for name in [
        "ANTHROPIC_API_KEY",
        "PRICE_INHA_TEST_SECRET",
        "CARGO_MANIFEST_DIR",
    ] {
        assert!(
            !seen.contains_key(name),
            "허용 목록 밖의 환경 {name}을 넘기면 안 된다"
        );
    }
    let raw = fake.read("auth_env");
    for value in [
        CLIENT_ID,
        CLIENT_SECRET,
        ANTHROPIC_ENV_SECRET,
        SERVER_ENV_SECRET,
    ] {
        assert!(
            !raw.contains(value),
            "CLI 환경에 증권사 키나 서버의 비밀이 있다"
        );
    }

    let cwd = fake.read("auth_cwd");
    let work = home.join(CLI_WORK_DIR);
    assert_eq!(
        Path::new(cwd.trim()),
        work.as_path(),
        "작업 폴더는 보관 폴더 아래 전용 폴더다"
    );
    let meta = std::fs::metadata(&work).unwrap();
    assert_eq!(meta.mode() & 0o777, 0o700);
    assert_eq!(std::fs::read_dir(&work).unwrap().count(), 0, "비어 있다");
    stop(app).await;
}

// ───────────── 6. 고른 연결로 공급자 하나 ─────────────

// 검증: C3
#[test]
fn cli_공급자_설정은_동시_실행_2와_받은_시간_제한을_쓴다() {
    assert_eq!(CLI_CONCURRENCY, 2);
    let choice = AiChoice {
        method: AiMethod::Cli,
        command: Some(PathBuf::from("/opt/test/bin/claude")),
        config_dir: Some(PathBuf::from("/opt/test/login")),
    };
    let env = vec![("HOME".to_string(), "/home/tester".to_string())];
    let work = PathBuf::from("/opt/test/home/cli-work");
    let timeout = Duration::from_millis(7_500);
    let config = cli_config(&choice, work.clone(), env.clone(), timeout);
    assert_eq!(config.command, choice.command);
    assert_eq!(config.config_dir, choice.config_dir);
    assert_eq!(config.work_dir, work);
    assert_eq!(config.base_env, env);
    assert_eq!(config.timeout, timeout, "AI 요청 시간 제한과 같다");
    assert_eq!(config.max_concurrency, 2, "동시 실행 수는 2");

    let plain = AiChoice {
        config_dir: None,
        ..choice
    };
    assert_eq!(
        cli_config(&plain, work, env, timeout).config_dir,
        None,
        "로그인 폴더가 없으면 평소 로그인"
    );
}

// 검증: V31
#[tokio::test]
async fn 서버는_고른_연결로_공급자_하나를_만들어_쥔다() {
    let toss = FakeToss::start().await;
    let ai = FakeAi::start().await;
    let dir = tempfile::tempdir().unwrap();
    register_broker(dir.path(), &toss).await;
    register_ai(dir.path(), &ai).await;
    let app = start_app(
        roomy(dir.path()),
        wiring_both(&toss, &ai, TestClock::at(kst(10, 0, 0))),
    )
    .await;
    let addr = app.local_addr();
    let fake = FakeCli::new(&Behave::logged_in());

    // API 키(기본): Anthropic으로 간다.
    let result = app.ai_provider().complete(tiny_request()).await;
    assert!(result.is_err(), "가짜 Anthropic에는 대화 경로가 없다");
    assert_eq!(messages_requests(&ai).await, 1, "API 키 공급자를 부른다");
    assert!(fake.calls().is_empty());

    // CLI를 고르면 공급자를 새로 만든다.
    let reply = choose(addr, &cli_body(fake.bin_str(), None)).await;
    assert_eq!(
        reply.json(),
        json!({ "registered": true }),
        "{}",
        reply.text
    );
    let result = app.ai_provider().complete(tiny_request()).await;
    assert!(result.is_err(), "가짜 CLI의 모델 호출은 실패로 끝난다");
    assert_eq!(fake.model_calls().len(), 1, "CLI 공급자를 부른다");
    assert_eq!(messages_requests(&ai).await, 1, "API로 넘어가지 않는다");
    stop(app).await;
}

// 검증: V31
#[tokio::test]
async fn cli_공급자의_시간_제한은_ai_요청_시간_제한과_같다() {
    let dir = tempfile::tempdir().unwrap();
    let mut wiring = wiring(None, TestClock::at(kst(10, 0, 0)));
    let limit = Duration::from_millis(1_500);
    wiring.upstreams.anthropic.timeout = limit;
    let app = start_app(roomy(dir.path()), wiring).await;
    let fake = FakeCli::new(&Behave::logged_in().with_model("/bin/sleep 30"));
    let reply = choose(app.local_addr(), &cli_body(fake.bin_str(), None)).await;
    assert_eq!(
        reply.json(),
        json!({ "registered": true }),
        "{}",
        reply.text
    );

    let started = Instant::now();
    let result = app.ai_provider().complete(tiny_request()).await;
    let took = started.elapsed();
    assert_eq!(result.unwrap_err(), AiError::Timeout);
    assert!(
        took >= limit,
        "AI 요청 시간 제한보다 먼저 끝나지 않는다: {took:?}"
    );
    assert!(
        took < Duration::from_secs(8),
        "AI 요청 시간 제한을 따른다: {took:?}"
    );
    stop(app).await;
}

// ───────────── 5 · 8. 화면 ─────────────

fn page() -> String {
    // 11차(Vite 이전): 스크립트가 묶음으로 나가므로 빌드한 화면과 그 묶음에서 찾는다.
    crate::screen::built_page("index.html")
}

// 검증: D11, V46
#[test]
fn 화면의_ai_줄은_두_고르기와_cli_칸과_확인_단추와_알림이고_토큰_칸은_없다() {
    let page = page();
    assert!(
        page.contains(&format!("\"{SETUP_AI_PATH}\"")),
        "화면이 {SETUP_AI_PATH}를 불러야 한다"
    );
    assert!(
        !page.contains(&format!("{SETUP_AI_PATH}?")),
        "고르기에 쿼리를 붙이지 않는다"
    );
    let radios = page.matches("type=\"radio\"").count();
    assert!(radios >= 2, "API 키와 CLI 두 고르기: {radios}개");
    for word in ["\"api_key\"", "\"cli\"", "\"command\"", "\"config_dir\""] {
        assert!(page.contains(word), "화면이 {word}를 알아야 한다");
    }
    assert!(page.contains("확인</button>"), "CLI를 확인하는 단추가 있다");
    assert!(
        page.contains(CLI_TERMS_NOTICE),
        "CLI를 고르면 약관 알림을 보여 준다"
    );
    assert!(
        page.contains("claude auth login"),
        "로그인 안 됨이면 터미널에서 로그인하라고 알린다"
    );
    for word in [
        CLI_LOGGED_IN,
        CLI_NOT_INSTALLED,
        CLI_TOO_OLD,
        CLI_NOT_LOGGED_IN,
        CLI_NOT_SUBSCRIPTION,
        CLI_UNCHECKED,
        "cli_not_installed",
        "cli_too_old",
        "cli_not_logged_in",
        "cli_not_subscription",
        "cli_unchecked",
    ] {
        assert!(
            page.contains(&format!("\"{word}\"")),
            "화면이 상태 {word}를 글자로 보여 줄 줄 안다"
        );
    }
    assert!(page.contains("legacy_auth_file"));
    assert!(
        page.contains(LEGACY_AUTH_NOTICE),
        "옛 auth.json 알림 문장이 있다"
    );
    for banned in [
        "ai_oauth",
        "access_token",
        "refresh_token",
        "OAuth",
        "구독 토큰 등록",
    ] {
        assert!(
            !page.contains(banned),
            "구독 토큰 칸 · 단추가 남아 있다: {banned}"
        );
    }
}

// ───────────── 8. 옛 auth.json 알림 ─────────────

// 검증: N8, V46, E38
#[tokio::test]
async fn 옛_auth_json은_있는지만_보고_읽지도_고치지도_지우지도_않는다() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path();
    let legacy = home.join(LEGACY_AUTH_FILE);
    assert!(!legacy_auth_file(home), "없으면 알리지 않는다");

    std::fs::write(&legacy, LEGACY_CONTENT).unwrap();
    // 읽을 수 없는 권한이어도 있는 것만 보고 알린다(내용을 읽지 않는다).
    std::fs::set_permissions(&legacy, std::fs::Permissions::from_mode(0o000)).unwrap();
    let before = std::fs::metadata(&legacy).unwrap();
    assert!(
        legacy_auth_file(home),
        "읽을 수 없는 파일이어도 있으면 알린다"
    );

    let app = start_app(
        settings(home, &[]),
        wiring(None, TestClock::at(kst(10, 0, 0))),
    )
    .await;
    let addr = app.local_addr();
    let status = setup_status(addr).await;
    assert_eq!(status["ai"]["legacy_auth_file"], json!(true), "{status}");
    assert!(
        !status.to_string().contains(LEGACY_SECRET),
        "알림에 파일 내용이 없다"
    );
    assert!(!registered(&status, ITEM_AI), "옛 토큰은 AI 연결이 아니다");
    stop(app).await;

    let after = std::fs::metadata(&legacy).expect("지우지 않는다");
    assert_eq!(after.mode() & 0o777, 0, "권한을 고치지 않는다");
    assert_eq!(after.len(), before.len(), "내용을 고치지 않는다");
    assert_eq!(after.mtime(), before.mtime(), "건드리지 않는다");
    assert_eq!(after.mtime_nsec(), before.mtime_nsec());
    std::fs::set_permissions(&legacy, std::fs::Permissions::from_mode(0o600)).unwrap();
    assert_eq!(std::fs::read_to_string(&legacy).unwrap(), LEGACY_CONTENT);

    // 지우면 알림도 없다.
    std::fs::remove_file(&legacy).unwrap();
    let app = start_app(
        settings(home, &[]),
        wiring(None, TestClock::at(kst(10, 0, 0))),
    )
    .await;
    assert_eq!(
        ai_status(app.local_addr()).await["legacy_auth_file"],
        json!(false)
    );
    stop(app).await;
}
