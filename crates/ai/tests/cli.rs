//! pi-ai 2차: 구독은 공식 Claude Code CLI로(CLI 공급자). docs/dev/modules/ai.md "2차 작업"을 확인한다.
//!
//! 실제 `claude`를 부르지 않는다. 테스트가 임시 폴더에 가짜 실행 파일(셸 스크립트)을 만들어
//! 경로로 준다. 가짜는 받은 인자(한 줄에 하나) · 표준 입력 · 환경 · 작업 폴더를 파일에 적고,
//! 정해 둔 출력 · 끝 코드 · 지연을 낸다. 오래 자는 가짜는 쓰지 않는다(제한을 짧게 준다).
//!
//! 출력 모양(`stream-json`의 `result` · `system/api_retry` 줄, `auth status --json`)은
//! `docs/research/cli-backend.md` 2.4 · 2.6 · 4.3 · 4.4 · 5.4 · 5.7을 따른다.

use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, Instant};

use pi_ai::{
    AiError, AiProvider, CLI_AUTH_TIMEOUT, CLI_DEFAULT_MAX_CONCURRENCY, CLI_MAX_STDIN_BYTES,
    CLI_MIN_VERSION, CLI_RESTRICTED_SETTINGS, CLI_VERSION_TIMEOUT, ClaudeCliConfig,
    ClaudeCliProvider, CliStatus, CliVersion, CompletionRequest, CompletionResponse, Message, Role,
    StopReason, Usage,
};
use serde_json::{Value, json};
use tempfile::TempDir;

const TEST_MODEL: &str = "siheom-model-b";
/// 환경에 넣는 시험용 값. 오류 · `Debug`에 나오면 안 된다.
const ENV_MARK: &str = "SIHEOM-HWANGYEONG-0001";
/// 대화 글의 시험용 표식.
const TALK_MARK: &str = "SIHEOM-DAEHWA-0001";
/// 시스템 글의 시험용 표식.
const SYSTEM_MARK: &str = "SIHEOM-SISEUTEM-0001";
/// CLI 출력 원문의 시험용 표식.
const OUTPUT_MARK: &str = "SIHEOM-CHULRYEOK-0001";

/// 셸이 스스로 만드는 환경 이름. 부모에게서 물려받은 것으로 보지 않는다.
const SHELL_MADE_ENV: [&str; 5] = ["PWD", "OLDPWD", "SHLVL", "_", "__CF_USER_TEXT_ENCODING"];

/// 어떤 경우에도 인자에 없어야 하는 것.
const FORBIDDEN_ARGS: [&str; 10] = [
    "--bare",
    "bypassPermissions",
    "--dangerously-skip-permissions",
    "--allowedTools",
    "--mcp-config",
    "--add-dir",
    "--plugin-dir",
    "--session-id",
    "--resume",
    "--continue",
];

// ---------- 가짜 CLI ----------

/// 가짜가 할 일.
struct Behave {
    /// `-p` 호출의 표준 출력.
    stdout: String,
    /// `-p` 호출의 표준 오류.
    stderr: String,
    /// `-p` 호출의 끝 코드.
    exit: i32,
    /// 기록 뒤, 출력 앞에 돌릴 셸 줄.
    before: String,
    /// 출력 뒤, 끝내기 앞에 돌릴 셸 줄.
    after: String,
    /// `--version` 갈래의 셸 줄(스스로 `exit` 한다).
    version: String,
    /// `auth status --json` 갈래의 셸 줄(스스로 `exit` 한다).
    auth: String,
}

impl Behave {
    fn ok() -> Self {
        Self::with_stdout(stream(&[ok_result("안녕하세요.", json!("end_turn"))]))
    }

    fn with_stdout(stdout: String) -> Self {
        Self {
            stdout,
            stderr: String::new(),
            exit: 0,
            before: String::new(),
            after: String::new(),
            version: version_says("2.1.316 (Claude Code)"),
            auth: auth_says(&logged_in_json("claude.ai", "max"), 0),
        }
    }

    fn failing(stdout: String, exit: i32) -> Self {
        Self {
            exit,
            ..Self::with_stdout(stdout)
        }
    }
}

fn version_says(text: &str) -> String {
    format!("printf '%s\\n' '{text}'\nexit 0")
}

fn auth_says(text: &str, code: i32) -> String {
    format!("printf '%s\\n' '{text}'\nexit {code}")
}

fn logged_in_json(method: &str, subscription: &str) -> String {
    json!({
        "loggedIn": true,
        "authMethod": method,
        "apiProvider": "firstParty",
        "subscriptionType": subscription,
    })
    .to_string()
}

/// 임시 폴더 하나에 든 가짜 CLI와 그 기록.
struct Fake {
    _root: TempDir,
    root: PathBuf,
    /// 가짜가 기록을 남기는 폴더.
    log: PathBuf,
    /// 가짜 실행 파일.
    bin: PathBuf,
    /// 작업 폴더(비어 있는 전용 폴더).
    work: PathBuf,
    /// 전용 로그인 폴더.
    login: PathBuf,
}

impl Fake {
    fn new(behave: Behave) -> Self {
        let dir = tempfile::tempdir().unwrap();
        // macOS의 임시 폴더는 다른 경로를 거치므로 실제 경로로 바꿔 둔다.
        let root = fs::canonicalize(dir.path()).unwrap();
        let log = root.join("log");
        let work = root.join("work");
        let login = root.join("login");
        for d in [&log, &work, &login] {
            fs::create_dir(d).unwrap();
        }
        fs::write(log.join("out"), &behave.stdout).unwrap();
        fs::write(log.join("err"), &behave.stderr).unwrap();
        let bin = root.join("claude");
        fs::write(&bin, script(&log, &behave)).unwrap();
        fs::set_permissions(&bin, fs::Permissions::from_mode(0o755)).unwrap();
        Self {
            _root: dir,
            root,
            log,
            bin,
            work,
            login,
        }
    }

    fn read(&self, name: &str) -> String {
        fs::read_to_string(self.log.join(name)).unwrap()
    }

    fn has(&self, name: &str) -> bool {
        self.log.join(name).exists()
    }

    /// 받은 인자. 빈 글자 인자도 한 칸이다.
    fn args(&self) -> Vec<String> {
        let text = self.read("args");
        let text = text.strip_suffix('\n').unwrap_or(&text);
        text.split('\n').map(str::to_string).collect()
    }

    fn env(&self) -> BTreeMap<String, String> {
        env_map(&self.read("env"))
    }

    /// 가짜가 불린 횟수(`--version` · `auth` 포함).
    fn calls(&self) -> Vec<String> {
        if !self.has("calls") {
            return Vec::new();
        }
        self.read("calls").lines().map(str::to_string).collect()
    }

    fn pid(&self, name: &str) -> u32 {
        self.read(name).trim().parse().unwrap()
    }

    fn work_entries(&self) -> Vec<PathBuf> {
        fs::read_dir(&self.work)
            .unwrap()
            .map(|e| e.unwrap().path())
            .collect()
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
echo $$ > "$L/pid"
: > "$L/args"
for a in "$@"; do printf '%s\n' "$a" >> "$L/args"; done
/bin/cat > "$L/stdin"
/usr/bin/env > "$L/env"
/bin/pwd -P > "$L/cwd"
prev=""
for a in "$@"; do
  if [ "$prev" = "--system-prompt-file" ]; then
    printf '%s\n' "$a" >> "$L/sysfile_paths"
    /bin/cat "$a" > "$L/sysfile"
    /bin/ls -l "$a" > "$L/sysfile_ls"
  fi
  prev="$a"
done
{before}
/bin/cat "$L/out"
/bin/cat "$L/err" >&2
{after}
exit {exit}
"#,
        version = b.version,
        auth = b.auth,
        before = b.before,
        after = b.after,
        exit = b.exit,
    )
}

/// 손자 프로세스를 띄워 그 번호를 남기는 셸 줄.
const SPAWN_GRANDCHILD: &str =
    "/bin/sleep 5 </dev/null >/dev/null 2>&1 &\necho $! > \"$L/grandchild\"";

// ---------- 출력 줄 ----------

fn stream(lines: &[Value]) -> String {
    let mut out = String::new();
    out.push_str(&json!({"type": "system", "subtype": "init", "session_id": "siheom"}).to_string());
    out.push('\n');
    for line in lines {
        out.push_str(&line.to_string());
        out.push('\n');
    }
    out
}

fn ok_result(text: &str, stop_reason: Value) -> Value {
    json!({
        "type": "result",
        "subtype": "success",
        "is_error": false,
        "result": text,
        "stop_reason": stop_reason,
        "num_turns": 1,
        "permission_denials": [],
        "usage": {"input_tokens": 12, "output_tokens": 5},
        "session_id": "siheom",
    })
}

fn fail_result(text: &str, api_error_status: Option<u16>) -> Value {
    let mut v = json!({
        "type": "result",
        "subtype": "success",
        "is_error": true,
        "result": text,
        "stop_reason": null,
        "num_turns": 1,
        "permission_denials": [],
        "usage": {"input_tokens": 0, "output_tokens": 0},
        "session_id": "siheom",
    });
    if let Some(status) = api_error_status {
        v["api_error_status"] = json!(status);
    }
    v
}

fn api_retry(error: &str) -> Value {
    json!({
        "type": "system",
        "subtype": "api_retry",
        "attempt": 1,
        "max_retries": 10,
        "retry_delay_ms": 500,
        "error": error,
        "session_id": "siheom",
    })
}

// ---------- 설정 · 요청 ----------

fn kv(k: &str, v: &str) -> (String, String) {
    (k.to_string(), v.to_string())
}

fn base_env() -> Vec<(String, String)> {
    vec![
        kv("PATH", "/usr/bin:/bin"),
        kv("LANG", "C.UTF-8"),
        kv("SIHEOM_KEEP", ENV_MARK),
    ]
}

/// 모든 칸을 적은 설정. 추가 인자를 받는 칸이 없음을 이 모양이 못 박는다.
fn config(f: &Fake) -> ClaudeCliConfig {
    ClaudeCliConfig {
        command: Some(f.bin.clone()),
        config_dir: None,
        work_dir: f.work.clone(),
        base_env: base_env(),
        timeout: Duration::from_secs(5),
        max_concurrency: 2,
        max_output_bytes: 1_000_000,
        kill_grace: Duration::from_millis(100),
        version_timeout: Duration::from_secs(2),
        auth_timeout: Duration::from_secs(2),
    }
}

fn user(text: &str) -> Message {
    Message {
        role: Role::User,
        content: text.to_string(),
    }
}

fn assistant(text: &str) -> Message {
    Message {
        role: Role::Assistant,
        content: text.to_string(),
    }
}

fn request(system: Option<&str>, messages: Vec<Message>) -> CompletionRequest {
    CompletionRequest {
        model: TEST_MODEL.to_string(),
        system: system.map(str::to_string),
        messages,
        max_tokens: 777,
    }
}

fn simple_request() -> CompletionRequest {
    request(
        Some(&format!("너는 시험 담당이다 {SYSTEM_MARK}")),
        vec![user(&format!("오늘 시장은? {TALK_MARK}"))],
    )
}

async fn call(cfg: ClaudeCliConfig, req: CompletionRequest) -> Result<CompletionResponse, AiError> {
    ClaudeCliProvider::new(cfg).complete(req).await
}

async fn call_fake(f: &Fake) -> Result<CompletionResponse, AiError> {
    call(config(f), simple_request()).await
}

/// 가짜가 `stdout`을 내고 `exit`로 끝날 때의 오류.
async fn error_for(stdout: String, stderr: &str, exit: i32) -> AiError {
    let mut b = Behave::failing(stdout, exit);
    b.stderr = stderr.to_string();
    let f = Fake::new(b);
    call_fake(&f).await.unwrap_err()
}

// ---------- 확인 도우미 ----------

fn env_map(text: &str) -> BTreeMap<String, String> {
    text.lines()
        .filter_map(|l| l.split_once('='))
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

/// 프로세스가 살아 있는가(좀비는 죽은 것으로 본다).
fn alive(pid: u32) -> bool {
    let out = Command::new("/bin/ps")
        .args(["-o", "stat=", "-p", &pid.to_string()])
        .output()
        .unwrap();
    let stat = String::from_utf8_lossy(&out.stdout);
    let stat = stat.trim();
    out.status.success() && !stat.is_empty() && !stat.starts_with('Z')
}

/// 3초 안에 죽으면 참.
async fn dies_soon(pid: u32) -> bool {
    for _ in 0..60 {
        if !alive(pid) {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    !alive(pid)
}

/// 3초 안에 파일이 생기면 참.
async fn appears_soon(path: &Path) -> bool {
    for _ in 0..60 {
        if path.exists() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    path.exists()
}

fn assert_clean(label: &str, text: &str) {
    for mark in [ENV_MARK, TALK_MARK, SYSTEM_MARK, OUTPUT_MARK] {
        assert!(!text.contains(mark), "{label}에 {mark}가 있다: {text}");
    }
}

fn assert_error_clean(err: &AiError) {
    assert_clean("오류 문장", &err.to_string());
    assert_clean("오류 Debug", &format!("{err:?}"));
}

fn expected_settings() -> Value {
    json!({
        "disableAllHooks": true,
        "enabledPlugins": {},
        "autoMemoryEnabled": false,
        "claudeMdExcludes": ["**/CLAUDE.md", "**/CLAUDE.local.md", "**/.claude/rules/**"],
    })
}

// ---------- 공급자 · 출력 읽기 ----------

// 검증: D4, A11
#[tokio::test]
async fn 구독_공급자는_trait_객체로_쓰고_결과_줄을_읽는다() {
    let stdout = format!(
        "이것은 JSON이 아니다\n{}{}\n",
        stream(&[
            json!({"type": "assistant", "message": {"content": [{"type": "text", "text": "안녕하세요."}]}}),
            api_retry("overloaded"),
        ]),
        ok_result("안녕하세요.", json!("end_turn"))
    );
    let f = Fake::new(Behave::with_stdout(stdout));
    let provider: Arc<dyn AiProvider> = Arc::new(ClaudeCliProvider::new(config(&f)));

    let response = provider.complete(simple_request()).await.unwrap();

    assert_eq!(
        response,
        CompletionResponse {
            text: "안녕하세요.".to_string(),
            stop_reason: StopReason::EndTurn,
            usage: Usage {
                input_tokens: 12,
                output_tokens: 5,
            },
        }
    );
}

// 검증: D4
#[test]
fn 설정의_기본값() {
    let cfg = ClaudeCliConfig::new(
        Some(PathBuf::from("/siheom/bin/claude")),
        PathBuf::from("/siheom/work"),
        base_env(),
        Duration::from_secs(30),
    );
    assert_eq!(cfg.command, Some(PathBuf::from("/siheom/bin/claude")));
    assert_eq!(cfg.work_dir, PathBuf::from("/siheom/work"));
    assert_eq!(cfg.base_env, base_env());
    assert_eq!(cfg.timeout, Duration::from_secs(30));
    assert_eq!(cfg.config_dir, None);
    assert_eq!(cfg.max_concurrency, 2);
    assert_eq!(CLI_DEFAULT_MAX_CONCURRENCY, 2);
    assert!(cfg.max_output_bytes > 0);
    assert_eq!(cfg.version_timeout, Duration::from_secs(5));
    assert_eq!(cfg.auth_timeout, Duration::from_secs(10));
    assert_eq!(CLI_VERSION_TIMEOUT, Duration::from_secs(5));
    assert_eq!(CLI_AUTH_TIMEOUT, Duration::from_secs(10));
    assert_eq!(CLI_MAX_STDIN_BYTES, 10_000_000);
    assert_eq!(
        CLI_MIN_VERSION,
        CliVersion {
            major: 2,
            minor: 1,
            patch: 259,
        }
    );
    let settings: Value = serde_json::from_str(CLI_RESTRICTED_SETTINGS).unwrap();
    assert_eq!(settings, expected_settings());
}

// ---------- 인자 묶음 ----------

// 검증: D4
#[tokio::test]
async fn 인자는_정해진_순서로_빠짐없이_나간다() {
    let f = Fake::new(Behave::ok());
    call_fake(&f).await.unwrap();

    let mut args = f.args();
    assert_eq!(args.len(), 27, "인자: {args:?}");
    assert_eq!(args[6], "--system-prompt-file");
    assert_eq!(args[16], "--settings");
    let settings: Value = serde_json::from_str(&args[17]).unwrap();
    assert_eq!(settings, expected_settings());
    args[7] = "<파일>".to_string();
    args[17] = "<설정>".to_string();
    let expected = [
        "-p",
        "--output-format",
        "stream-json",
        "--verbose",
        "--model",
        TEST_MODEL,
        "--system-prompt-file",
        "<파일>",
        "--safe-mode",
        "--tools",
        "",
        "--disallowedTools",
        "mcp__*",
        "--strict-mcp-config",
        "--setting-sources",
        "",
        "--settings",
        "<설정>",
        "--disable-slash-commands",
        "--no-chrome",
        "--no-session-persistence",
        "--max-turns",
        "1",
        "--permission-mode",
        "dontAsk",
        "--permission-prompts",
        "none",
    ];
    assert_eq!(args, expected);
}

// 검증: D4
#[tokio::test]
async fn 시스템_글이_없으면_system_prompt_file_두_칸만_빠진다() {
    let f = Fake::new(Behave::ok());
    call(config(&f), request(None, vec![user("안녕")]))
        .await
        .unwrap();

    let args = f.args();
    let settings: Value = serde_json::from_str(&args[15]).unwrap();
    assert_eq!(settings, expected_settings());
    let mut args = args;
    args[15] = "<설정>".to_string();
    let expected = [
        "-p",
        "--output-format",
        "stream-json",
        "--verbose",
        "--model",
        TEST_MODEL,
        "--safe-mode",
        "--tools",
        "",
        "--disallowedTools",
        "mcp__*",
        "--strict-mcp-config",
        "--setting-sources",
        "",
        "--settings",
        "<설정>",
        "--disable-slash-commands",
        "--no-chrome",
        "--no-session-persistence",
        "--max-turns",
        "1",
        "--permission-mode",
        "dontAsk",
        "--permission-prompts",
        "none",
    ];
    assert_eq!(args, expected);
    assert!(!f.has("sysfile_paths"));
}

// 검증: D4, E38
#[tokio::test]
async fn 금지된_인자와_대화_글과_시스템_글은_인자에_없다() {
    let requests = [
        simple_request(),
        request(None, vec![user(&format!("질문 {TALK_MARK}"))]),
        request(
            Some(SYSTEM_MARK),
            vec![user(TALK_MARK), assistant(TALK_MARK), user(TALK_MARK)],
        ),
    ];
    for req in requests {
        let f = Fake::new(Behave::ok());
        call(config(&f), req).await.unwrap();
        let args = f.args();
        for arg in &args {
            for bad in FORBIDDEN_ARGS {
                assert!(!arg.contains(bad), "금지된 인자 {bad}: {args:?}");
            }
            assert!(!arg.contains(TALK_MARK), "대화 글이 인자에 있다: {args:?}");
            assert!(
                !arg.contains(SYSTEM_MARK),
                "시스템 글이 인자에 있다: {args:?}"
            );
        }
    }
}

// ---------- 시스템 글 임시 파일 ----------

// 검증: E38
#[tokio::test]
async fn 시스템_글은_작업_폴더_안의_0600_파일로_주고_끝나면_지운다() {
    let f = Fake::new(Behave::ok());
    call_fake(&f).await.unwrap();

    assert_eq!(
        f.read("sysfile"),
        format!("너는 시험 담당이다 {SYSTEM_MARK}")
    );
    assert!(
        f.read("sysfile_ls").starts_with("-rw-------"),
        "권한: {}",
        f.read("sysfile_ls")
    );
    let path = PathBuf::from(f.read("sysfile_paths").trim());
    let parent = fs::canonicalize(path.parent().unwrap()).unwrap();
    assert_eq!(parent, f.work);
    assert!(!path.exists(), "시스템 글 파일이 남았다");
    assert!(f.work_entries().is_empty(), "작업 폴더가 비지 않았다");
}

// 검증: E38
#[tokio::test]
async fn 시스템_글_파일_이름은_호출마다_다르다() {
    let f = Fake::new(Behave::ok());
    let provider = ClaudeCliProvider::new(config(&f));
    provider.complete(simple_request()).await.unwrap();
    provider.complete(simple_request()).await.unwrap();

    let paths: Vec<String> = f
        .read("sysfile_paths")
        .lines()
        .map(str::to_string)
        .collect();
    assert_eq!(paths.len(), 2);
    assert_ne!(paths[0], paths[1]);
    assert!(f.work_entries().is_empty());
}

// 검증: E38
#[tokio::test]
async fn 실패와_시간_초과에도_시스템_글_파일을_지운다() {
    // 실패
    let f = Fake::new(Behave::failing(String::new(), 2));
    call_fake(&f).await.unwrap_err();
    assert!(f.has("sysfile_paths"));
    assert!(
        f.work_entries().is_empty(),
        "실패 뒤 작업 폴더가 비지 않았다"
    );

    // 시간 초과
    let mut b = Behave::ok();
    b.after = "/bin/sleep 5".to_string();
    let f = Fake::new(b);
    let mut cfg = config(&f);
    cfg.timeout = Duration::from_millis(1500);
    assert_eq!(
        call(cfg, simple_request()).await.unwrap_err(),
        AiError::Timeout
    );
    assert!(f.has("sysfile_paths"));
    assert!(
        f.work_entries().is_empty(),
        "시간 초과 뒤 작업 폴더가 비지 않았다"
    );
}

// ---------- 환경 ----------

// 검증: D4, E38
#[tokio::test]
async fn 부모_환경을_물려주지_않고_base_env만_준다() {
    let f = Fake::new(Behave::ok());
    call_fake(&f).await.unwrap();

    let child = f.env();
    let base: BTreeMap<String, String> = base_env().into_iter().collect();
    for (k, v) in &base {
        assert_eq!(child.get(k), Some(v), "base_env의 {k}가 없다");
    }
    let injected = [
        "CLAUDE_CODE_DISABLE_GIT_INSTRUCTIONS",
        "CLAUDE_CODE_MAX_OUTPUT_TOKENS",
    ];
    for k in child.keys() {
        assert!(
            base.contains_key(k)
                || injected.contains(&k.as_str())
                || SHELL_MADE_ENV.contains(&k.as_str()),
            "넣지 않은 환경 {k}가 있다"
        );
    }
    let mut parent_only = 0;
    for (k, _) in std::env::vars() {
        if base.contains_key(&k) || SHELL_MADE_ENV.contains(&k.as_str()) {
            continue;
        }
        parent_only += 1;
        assert!(!child.contains_key(&k), "부모 환경 {k}를 물려줬다");
    }
    assert!(
        parent_only > 0,
        "부모에만 있는 환경이 없어 확인이 되지 않는다"
    );
}

// 검증: D4
#[tokio::test]
async fn 과금을_바꾸는_환경은_base_env에_있어도_뺀다() {
    let f = Fake::new(Behave::ok());
    let mut cfg = config(&f);
    let removed = [
        "ANTHROPIC_API_KEY",
        "ANTHROPIC_AUTH_TOKEN",
        "ANTHROPIC_BASE_URL",
        "CLAUDE_CODE_USE_BEDROCK",
        "CLAUDE_CODE_USE_VERTEX",
        "OTEL_EXPORTER_OTLP_ENDPOINT",
        "CLAUDE_CODE_OAUTH_TOKEN",
        "CLAUDE_CODE_OAUTH_TOKEN_FILE_DESCRIPTOR",
    ];
    for k in removed {
        cfg.base_env.push(kv(k, "SIHEOM-BBAEL-GAP"));
    }
    cfg.base_env.push(kv("MY_ANTHROPIC_NOTE", "남는다"));
    call(cfg, simple_request()).await.unwrap();

    let child = f.env();
    for k in removed {
        assert!(!child.contains_key(k), "{k}가 남았다");
    }
    assert_eq!(
        child.get("MY_ANTHROPIC_NOTE").map(String::as_str),
        Some("남는다")
    );
    assert_eq!(child.get("SIHEOM_KEEP").map(String::as_str), Some(ENV_MARK));
}

// 검증: D4
#[tokio::test]
async fn 넣는_환경은_git_지시_끔과_최대_토큰과_로그인_폴더다() {
    let f = Fake::new(Behave::ok());
    let mut cfg = config(&f);
    cfg.config_dir = Some(f.login.clone());
    cfg.base_env
        .push(kv("CLAUDE_CODE_DISABLE_GIT_INSTRUCTIONS", "0"));
    cfg.base_env.push(kv("CLAUDE_CODE_MAX_OUTPUT_TOKENS", "1"));
    cfg.base_env
        .push(kv("CLAUDE_CONFIG_DIR", "/siheom/elsewhere"));
    call(cfg, simple_request()).await.unwrap();

    let child = f.env();
    assert_eq!(
        child
            .get("CLAUDE_CODE_DISABLE_GIT_INSTRUCTIONS")
            .map(String::as_str),
        Some("1")
    );
    assert_eq!(
        child
            .get("CLAUDE_CODE_MAX_OUTPUT_TOKENS")
            .map(String::as_str),
        Some("777")
    );
    assert_eq!(
        child.get("CLAUDE_CONFIG_DIR").map(PathBuf::from),
        Some(f.login.clone())
    );
}

// 검증: D4
#[tokio::test]
async fn 로그인_폴더가_없으면_claude_config_dir를_넣지_않는다() {
    let f = Fake::new(Behave::ok());
    call_fake(&f).await.unwrap();
    assert!(!f.env().contains_key("CLAUDE_CONFIG_DIR"));
}

// 검증: D4
#[tokio::test]
async fn 작업_폴더는_설정의_전용_폴더다() {
    let f = Fake::new(Behave::ok());
    call_fake(&f).await.unwrap();
    assert_eq!(PathBuf::from(f.read("cwd").trim()), f.work);
}

// ---------- 표준 입력 ----------

// 검증: D4
#[tokio::test]
async fn 사용자_글_하나는_그대로_표준_입력으로_쓰고_닫는다() {
    let f = Fake::new(Behave::ok());
    let text = "첫 줄\n둘째 줄 그대로";
    call(config(&f), request(None, vec![user(text)]))
        .await
        .unwrap();
    // 가짜는 표준 입력을 끝까지 읽은 뒤에야 답한다. 닫지 않았다면 여기까지 오지 못한다.
    assert_eq!(f.read("stdin"), text);
}

// 검증: D4
#[tokio::test]
async fn 여러_줄_대화는_머리_줄과_빈_줄로_이어_쓴다() {
    let f = Fake::new(Behave::ok());
    let messages = vec![user("가"), assistant("나\n나2"), user("다")];
    call(config(&f), request(None, messages)).await.unwrap();

    let expected =
        "[사용자]\n가\n\n[담당]\n나\n나2\n\n[사용자]\n다\n\n마지막 사용자 요청에 이어서 답한다.";
    let stdin = f.read("stdin");
    let got = stdin.strip_suffix('\n').unwrap_or(&stdin);
    assert_eq!(got, expected);
}

// 검증: V31
#[tokio::test]
async fn 표준_입력이_10mb를_넘으면_띄우지_않고_잘못된_요청이다() {
    let f = Fake::new(Behave::ok());
    let big = "x".repeat(CLI_MAX_STDIN_BYTES + 1);
    let err = call(config(&f), request(None, vec![user(&big)]))
        .await
        .unwrap_err();
    assert!(
        matches!(err, AiError::InvalidRequest { .. }),
        "오류: {err:?}"
    );
    assert!(f.calls().is_empty(), "가짜를 띄웠다");
}

// 검증: D4
#[tokio::test]
async fn 표준_입력이_꼭_10mb면_보낸다() {
    let f = Fake::new(Behave::ok());
    let exact = "x".repeat(CLI_MAX_STDIN_BYTES);
    call(config(&f), request(None, vec![user(&exact)]))
        .await
        .unwrap();
    assert_eq!(
        fs::metadata(f.log.join("stdin")).unwrap().len(),
        CLI_MAX_STDIN_BYTES as u64
    );
}

// ---------- 멈춘 이유 ----------

// 검증: A11
#[tokio::test]
async fn 멈춘_이유를_옮긴다() {
    let cases = [
        ("end_turn", StopReason::EndTurn),
        ("max_tokens", StopReason::MaxTokens),
        ("stop_sequence", StopReason::StopSequence),
        ("refusal", StopReason::Refusal),
        ("tool_use", StopReason::ToolUse),
        ("pause_turn", StopReason::PauseTurn),
        (
            "model_context_window_exceeded",
            StopReason::ModelContextWindowExceeded,
        ),
    ];
    for (raw, expected) in cases {
        let f = Fake::new(Behave::with_stdout(stream(&[ok_result("글", json!(raw))])));
        let response = call_fake(&f).await.unwrap();
        assert_eq!(response.stop_reason, expected, "{raw}");
    }
}

// 검증: A11
#[tokio::test]
async fn 모르는_멈춘_이유와_null은_other다() {
    for raw in [json!("siheom_reason"), Value::Null] {
        let f = Fake::new(Behave::with_stdout(stream(&[ok_result("글", raw.clone())])));
        let response = call_fake(&f).await.unwrap();
        assert!(
            matches!(response.stop_reason, StopReason::Other(_)),
            "{raw}: {:?}",
            response.stop_reason
        );
        assert_eq!(response.text, "글");
    }
}

// ---------- 읽지 못한 응답 ----------

// 검증: D4
#[tokio::test]
async fn 도구가_뚫린_결과는_읽지_못한_응답이다() {
    let mut denied = ok_result("글", json!("end_turn"));
    denied["permission_denials"] =
        json!([{"tool_name": "Bash", "tool_use_id": "siheom", "tool_input": {}}]);
    let mut turns = ok_result("글", json!("end_turn"));
    turns["num_turns"] = json!(2);
    for line in [denied, turns] {
        let f = Fake::new(Behave::with_stdout(stream(&[line])));
        assert_eq!(call_fake(&f).await.unwrap_err(), AiError::InvalidResponse);
    }
}

// 검증: A11
#[tokio::test]
async fn 결과_줄이_없거나_모양이_틀리면_읽지_못한_응답이다() {
    let mut not_text = ok_result("글", json!("end_turn"));
    not_text["result"] = json!(3);
    let mut no_text = ok_result("글", json!("end_turn"));
    no_text.as_object_mut().unwrap().remove("result");
    let outputs = [
        stream(&[]),
        "JSON이 아님\n".to_string(),
        String::new(),
        stream(&[not_text]),
        stream(&[no_text]),
        stream(&[fail_result("알 수 없는 실패", None)]),
    ];
    for stdout in outputs {
        let f = Fake::new(Behave::with_stdout(stdout.clone()));
        assert_eq!(
            call_fake(&f).await.unwrap_err(),
            AiError::InvalidResponse,
            "출력: {stdout}"
        );
    }
}

// 검증: E38
#[tokio::test]
async fn 표준_출력이_상한을_넘으면_끝내고_읽지_못한_응답이다() {
    let mut b = Behave::with_stdout(format!("{}\n", "x".repeat(200_000)));
    b.after = "/bin/sleep 5".to_string();
    let f = Fake::new(b);
    let mut cfg = config(&f);
    cfg.max_output_bytes = 10_000;

    let started = Instant::now();
    let err = call(cfg, simple_request()).await.unwrap_err();

    assert_eq!(err, AiError::InvalidResponse);
    assert!(started.elapsed() < Duration::from_secs(3), "끝내지 않았다");
    assert!(dies_soon(f.pid("pid")).await, "가짜가 살아 있다");
}

// 검증: E38
#[tokio::test]
async fn 표준_오류가_상한을_넘어도_끝내고_읽지_못한_응답이다() {
    let mut b = Behave::ok();
    b.stderr = format!("{}\n", "e".repeat(200_000));
    b.after = "/bin/sleep 5".to_string();
    let f = Fake::new(b);
    let mut cfg = config(&f);
    cfg.max_output_bytes = 10_000;

    let started = Instant::now();
    let err = call(cfg, simple_request()).await.unwrap_err();

    assert_eq!(err, AiError::InvalidResponse);
    assert!(started.elapsed() < Duration::from_secs(3), "끝내지 않았다");
    assert!(dies_soon(f.pid("pid")).await, "가짜가 살아 있다");
}

// ---------- 오류 옮기기 ----------

// 검증: A11
#[tokio::test]
async fn api_error_status로_오류를_옮긴다() {
    let cases = [
        (401, AiError::Auth { status: 401 }),
        (403, AiError::Auth { status: 403 }),
        (429, AiError::RateLimited { retry_after: None }),
        (500, AiError::ProviderUnavailable { status: 500 }),
        (503, AiError::ProviderUnavailable { status: 503 }),
        (529, AiError::ProviderUnavailable { status: 529 }),
    ];
    for (status, expected) in cases {
        let err = error_for(
            stream(&[fail_result("API Error: 무언가", Some(status))]),
            "",
            1,
        )
        .await;
        assert_eq!(err, expected, "{status}");
    }
    for status in [400, 404, 413] {
        let err = error_for(
            stream(&[fail_result("API Error: 무언가", Some(status))]),
            "",
            1,
        )
        .await;
        assert!(
            matches!(err, AiError::InvalidRequest { .. }),
            "{status}: {err:?}"
        );
    }
}

// 검증: A11
#[tokio::test]
async fn 상태가_없으면_결과_글로_오류를_옮긴다() {
    let cases = [
        (
            "Not logged in · Please run /login",
            AiError::Auth { status: 401 },
        ),
        (
            "Login expired · Please run /login",
            AiError::Auth { status: 401 },
        ),
        (
            "You've hit your session limit · resets 3:45pm",
            AiError::RateLimited { retry_after: None },
        ),
        (
            "You've hit your weekly limit · resets Mon 9:00am",
            AiError::RateLimited { retry_after: None },
        ),
        (
            "OAuth token is being refreshed: another Claude Code process is refreshing it",
            AiError::ProviderUnavailable { status: 503 },
        ),
    ];
    for (text, expected) in cases {
        let err = error_for(stream(&[fail_result(text, None)]), "", 1).await;
        assert_eq!(err, expected, "{text}");
    }
}

// 검증: A11
#[tokio::test]
async fn 글로도_모르면_마지막_api_retry_줄로_오류를_옮긴다() {
    let cases = [
        ("authentication_failed", AiError::Auth { status: 401 }),
        ("oauth_org_not_allowed", AiError::Auth { status: 403 }),
        ("account_on_hold", AiError::Auth { status: 403 }),
        ("billing_error", AiError::Auth { status: 403 }),
        ("rate_limit", AiError::RateLimited { retry_after: None }),
        ("overloaded", AiError::ProviderUnavailable { status: 529 }),
        ("server_error", AiError::ProviderUnavailable { status: 500 }),
    ];
    for (error, expected) in cases {
        let stdout = stream(&[
            api_retry(error),
            fail_result("API Error: Repeated failures", None),
        ]);
        let err = error_for(stdout, "", 1).await;
        assert_eq!(err, expected, "{error}");
    }
    for error in ["invalid_request", "model_not_found"] {
        let stdout = stream(&[
            api_retry(error),
            fail_result("API Error: Repeated failures", None),
        ]);
        let err = error_for(stdout, "", 1).await;
        assert!(
            matches!(err, AiError::InvalidRequest { .. }),
            "{error}: {err:?}"
        );
    }
    // 마지막 줄을 본다.
    let stdout = stream(&[
        api_retry("authentication_failed"),
        api_retry("overloaded"),
        fail_result("API Error: Repeated failures", None),
    ]);
    assert_eq!(
        error_for(stdout, "", 1).await,
        AiError::ProviderUnavailable { status: 529 }
    );
}

// 검증: A11
#[tokio::test]
async fn 오류_옮기기는_상태_다음_글_다음_api_retry_순서다() {
    // 상태가 글보다 먼저다.
    let stdout = stream(&[fail_result("Not logged in · Please run /login", Some(429))]);
    assert_eq!(
        error_for(stdout, "", 1).await,
        AiError::RateLimited { retry_after: None }
    );
    // 글이 api_retry보다 먼저다.
    let stdout = stream(&[
        api_retry("overloaded"),
        fail_result("Not logged in · Please run /login", None),
    ]);
    assert_eq!(
        error_for(stdout, "", 1).await,
        AiError::Auth { status: 401 }
    );
    // 결과 줄이 있으면 끝 코드보다 결과 줄이 먼저다.
    let stdout = stream(&[fail_result("Not logged in · Please run /login", None)]);
    assert_eq!(
        error_for(stdout, "error: 다른 까닭", 2).await,
        AiError::Auth { status: 401 }
    );
}

// 검증: A11, E38
#[tokio::test]
async fn 끝_코드가_0이_아니고_결과가_없으면_표준_오류_첫_줄로_잘못된_요청이다() {
    let err = error_for(
        format!("{OUTPUT_MARK}\n"),
        &format!("error: unknown option '--siheom'\n{OUTPUT_MARK}\n"),
        1,
    )
    .await;
    assert_eq!(
        err,
        AiError::InvalidRequest {
            message: "error: unknown option '--siheom'".to_string(),
        }
    );

    let long = "가".repeat(500);
    let err = error_for(String::new(), &format!("{long}\n둘째 줄\n"), 2).await;
    assert_eq!(
        err,
        AiError::InvalidRequest {
            message: "가".repeat(200),
        }
    );
}

// 검증: V31
#[tokio::test]
async fn 실행_파일이_없으면_아무것도_띄우지_않고_설정_안_됨이다() {
    let f = Fake::new(Behave::ok());
    let mut cfg = config(&f);
    cfg.command = None;
    assert_eq!(
        call(cfg, simple_request()).await.unwrap_err(),
        AiError::NotConfigured
    );

    let mut cfg = config(&f);
    cfg.command = Some(f.root.join("없는-claude"));
    assert_eq!(
        call(cfg, simple_request()).await.unwrap_err(),
        AiError::NotConfigured
    );

    assert!(f.calls().is_empty(), "가짜를 띄웠다");
    assert!(f.work_entries().is_empty(), "작업 폴더에 무언가 남았다");
}

// 검증: A11
#[tokio::test]
async fn 띄우기_실패와_신호로_죽음은_연결_실패다() {
    // 실행할 수 없는 파일
    let f = Fake::new(Behave::ok());
    fs::set_permissions(&f.bin, fs::Permissions::from_mode(0o644)).unwrap();
    assert_eq!(call_fake(&f).await.unwrap_err(), AiError::Connection);

    // 신호로 죽음
    let mut b = Behave::with_stdout(String::new());
    b.before = "kill -9 $$".to_string();
    let f = Fake::new(b);
    assert_eq!(call_fake(&f).await.unwrap_err(), AiError::Connection);
}

// ---------- 다시 시도하지 않는다 ----------

// 검증: A11
#[tokio::test]
async fn 어떤_오류도_다시_부르지_않는다() {
    let outputs = [
        (stream(&[fail_result("API Error", Some(429))]), 1),
        (stream(&[fail_result("API Error", Some(529))]), 1),
        (stream(&[fail_result("API Error", Some(500))]), 1),
        (
            stream(&[fail_result("Not logged in · Please run /login", None)]),
            1,
        ),
        (
            stream(&[fail_result(
                "another Claude Code process is refreshing it",
                None,
            )]),
            1,
        ),
        (String::new(), 2),
        (stream(&[]), 0),
    ];
    for (stdout, exit) in outputs {
        let f = Fake::new(Behave::failing(stdout.clone(), exit));
        call_fake(&f).await.unwrap_err();
        assert_eq!(f.calls().len(), 1, "다시 불렀다: {stdout}");
    }
}

// ---------- 시간 · 끝내기 ----------

// 검증: A11
#[tokio::test]
async fn 시간_제한을_넘기면_시간_초과이고_가짜와_그_자식이_죽는다() {
    let mut b = Behave::with_stdout(String::new());
    b.before = SPAWN_GRANDCHILD.to_string();
    b.after = "/bin/sleep 5".to_string();
    let f = Fake::new(b);
    let mut cfg = config(&f);
    cfg.timeout = Duration::from_millis(1500);

    let started = Instant::now();
    let err = call(cfg, simple_request()).await.unwrap_err();

    assert_eq!(err, AiError::Timeout);
    assert!(started.elapsed() < Duration::from_secs(3));
    assert!(dies_soon(f.pid("pid")).await, "가짜가 살아 있다");
    assert!(
        dies_soon(f.pid("grandchild")).await,
        "가짜의 자식이 살아 있다"
    );
}

// 검증: A11
#[tokio::test]
async fn 끝낼_때는_그룹에_sigterm을_먼저_보내고_유예_뒤_sigkill로_끝낸다() {
    // SIGTERM을 받으면 표시를 남기고 끝나는 가짜.
    let mut b = Behave::with_stdout(String::new());
    b.before = "trap 'echo term > \"$L/got_term\"; exit 0' TERM".to_string();
    b.after = "/bin/sleep 5".to_string();
    let f = Fake::new(b);
    let mut cfg = config(&f);
    cfg.timeout = Duration::from_millis(1500);
    cfg.kill_grace = Duration::from_secs(2);
    assert_eq!(
        call(cfg, simple_request()).await.unwrap_err(),
        AiError::Timeout
    );
    assert!(
        appears_soon(&f.log.join("got_term")).await,
        "SIGTERM을 받지 않았다"
    );

    // SIGTERM을 무시하는 가짜와 그 자식도 유예 뒤에는 죽는다.
    let mut b = Behave::with_stdout(String::new());
    b.before = format!("trap '' TERM\n{SPAWN_GRANDCHILD}");
    b.after = "/bin/sleep 5".to_string();
    let f = Fake::new(b);
    let mut cfg = config(&f);
    cfg.timeout = Duration::from_millis(1500);
    cfg.kill_grace = Duration::from_millis(200);
    assert_eq!(
        call(cfg, simple_request()).await.unwrap_err(),
        AiError::Timeout
    );
    assert!(dies_soon(f.pid("pid")).await, "가짜가 살아 있다");
    assert!(
        dies_soon(f.pid("grandchild")).await,
        "가짜의 자식이 살아 있다"
    );
}

// 검증: A11, E38
#[tokio::test]
async fn 버려진_호출은_프로세스와_임시_파일을_남기지_않는다() {
    let mut b = Behave::with_stdout(String::new());
    b.before = SPAWN_GRANDCHILD.to_string();
    b.after = "/bin/sleep 5".to_string();
    let f = Fake::new(b);
    let provider = ClaudeCliProvider::new(config(&f));

    let abandoned = tokio::time::timeout(
        Duration::from_millis(1500),
        provider.complete(simple_request()),
    )
    .await;
    assert!(abandoned.is_err(), "호출이 버려지지 않았다");

    assert!(f.has("grandchild"), "가짜가 아직 뜨지 않았다");
    assert!(dies_soon(f.pid("pid")).await, "가짜가 살아 있다");
    assert!(
        dies_soon(f.pid("grandchild")).await,
        "가짜의 자식이 살아 있다"
    );
    for _ in 0..60 {
        if f.work_entries().is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(f.work_entries().is_empty(), "시스템 글 파일이 남았다");
}

// 검증: A11
#[tokio::test]
async fn 실패_결과_줄이_나온_뒤_늦게_끝나도_그_결과가_우선이다() {
    let mut b = Behave::failing(
        stream(&[fail_result("Not logged in · Please run /login", None)]),
        1,
    );
    b.after = "/bin/sleep 5".to_string();
    let f = Fake::new(b);
    let mut cfg = config(&f);
    cfg.timeout = Duration::from_millis(1500);

    let err = call(cfg, simple_request()).await.unwrap_err();

    assert_eq!(err, AiError::Auth { status: 401 });
    assert!(dies_soon(f.pid("pid")).await, "가짜가 살아 있다");
}

// ---------- 동시 실행 ----------

const COUNT_RUNNING: &str = r#"/bin/mkdir -p "$L/running"
: > "$L/running/$$"
/bin/ls "$L/running" | /usr/bin/wc -l >> "$L/counts"
/bin/sleep 0.4
/bin/rm -f "$L/running/$$""#;

// 검증: A11
#[tokio::test]
async fn 동시에_산_자식은_기본_상한_2를_넘지_않는다() {
    let mut b = Behave::ok();
    b.before = COUNT_RUNNING.to_string();
    let f = Fake::new(b);
    let cfg = ClaudeCliConfig::new(
        Some(f.bin.clone()),
        f.work.clone(),
        base_env(),
        Duration::from_secs(10),
    );
    let provider = Arc::new(ClaudeCliProvider::new(cfg));

    let mut tasks = Vec::new();
    for _ in 0..4 {
        let p = Arc::clone(&provider);
        tasks.push(tokio::spawn(
            async move { p.complete(simple_request()).await },
        ));
    }
    for t in tasks {
        t.await.unwrap().unwrap();
    }

    let counts: Vec<usize> = f
        .read("counts")
        .lines()
        .map(|l| l.trim().parse().unwrap())
        .collect();
    assert_eq!(counts.len(), 4);
    assert_eq!(counts.iter().max(), Some(&2), "동시 실행 수: {counts:?}");
}

// 검증: A11
#[tokio::test]
async fn 줄에서_기다린_시간도_전체_제한에_든다() {
    let mut b = Behave::ok();
    b.before = "/bin/sleep 1.5".to_string();
    let f = Fake::new(b);
    let mut cfg = config(&f);
    cfg.max_concurrency = 1;
    cfg.timeout = Duration::from_millis(2500);
    let provider = Arc::new(ClaudeCliProvider::new(cfg));

    let a = Arc::clone(&provider);
    let b = Arc::clone(&provider);
    let (first, second) = tokio::join!(
        tokio::spawn(async move { a.complete(simple_request()).await }),
        tokio::spawn(async move { b.complete(simple_request()).await }),
    );
    let results = [first.unwrap(), second.unwrap()];

    let ok = results.iter().filter(|r| r.is_ok()).count();
    let timeouts = results
        .iter()
        .filter(|r| matches!(r, Err(AiError::Timeout)))
        .count();
    assert_eq!((ok, timeouts), (1, 1), "결과: {results:?}");
}

// ---------- 비밀이 새지 않는다 (E38) ----------

// 검증: E38
#[tokio::test]
async fn 오류에_출력_원문과_환경_값과_대화_글이_없다() {
    let marked = format!("API Error: {OUTPUT_MARK} {ENV_MARK} {TALK_MARK}");
    let cases: Vec<(String, String, i32)> = vec![
        (stream(&[fail_result(&marked, Some(400))]), String::new(), 1),
        (stream(&[fail_result(&marked, Some(401))]), String::new(), 1),
        (stream(&[fail_result(&marked, None)]), String::new(), 1),
        (
            stream(&[fail_result(
                &format!("Not logged in · Please run /login {OUTPUT_MARK}"),
                None,
            )]),
            String::new(),
            1,
        ),
        (
            stream(&[api_retry("invalid_request"), fail_result(&marked, None)]),
            String::new(),
            1,
        ),
        (
            format!("{marked}\n"),
            format!("error: bad option\n{OUTPUT_MARK} {ENV_MARK}\n"),
            2,
        ),
        (stream(&[]), format!("{OUTPUT_MARK}\n"), 0),
    ];
    for (stdout, stderr, exit) in cases {
        let err = error_for(stdout, &stderr, exit).await;
        assert_error_clean(&err);
    }

    // 시간 초과
    let mut b = Behave::with_stdout(format!("{marked}\n"));
    b.after = "/bin/sleep 5".to_string();
    let f = Fake::new(b);
    let mut cfg = config(&f);
    cfg.timeout = Duration::from_millis(1500);
    assert_error_clean(&call(cfg, simple_request()).await.unwrap_err());
}

// 검증: E38
#[test]
fn 설정의_debug에_환경_값이_없다() {
    let cfg = ClaudeCliConfig {
        command: Some(PathBuf::from("/siheom/bin/claude")),
        config_dir: None,
        work_dir: PathBuf::from("/siheom/work"),
        base_env: vec![
            kv("SIHEOM_KEEP", ENV_MARK),
            kv("ANTHROPIC_API_KEY", ENV_MARK),
        ],
        timeout: Duration::from_secs(1),
        max_concurrency: 2,
        max_output_bytes: 1000,
        kill_grace: Duration::from_millis(100),
        version_timeout: Duration::from_secs(1),
        auth_timeout: Duration::from_secs(1),
    };
    assert_clean("설정 Debug", &format!("{cfg:?}"));
    assert_clean("설정 Debug(#)", &format!("{cfg:#?}"));
}

// ---------- 설치 · 로그인 확인 ----------

fn check_fake(version: &str, auth: &str) -> Fake {
    let mut b = Behave::ok();
    b.version = version.to_string();
    b.auth = auth.to_string();
    Fake::new(b)
}

async fn check(f: &Fake) -> CliStatus {
    ClaudeCliProvider::new(config(f)).check().await
}

// 검증: D4
#[tokio::test]
async fn 확인은_버전과_로그인을_차례로_보고_모델을_부르지_않는다() {
    let f = check_fake(
        &version_says("2.1.316 (Claude Code)"),
        &auth_says(&logged_in_json("claude.ai", "max"), 0),
    );

    let status = check(&f).await;

    assert_eq!(
        status,
        CliStatus::LoggedIn {
            version: CliVersion {
                major: 2,
                minor: 1,
                patch: 316,
            },
            auth_method: "claude.ai".to_string(),
            subscription_type: Some("max".to_string()),
        }
    );
    assert_eq!(f.calls(), ["--version", "auth status --json"]);
    assert!(!f.has("args"), "모델을 불렀다");
}

// 검증: D4
#[tokio::test]
async fn 로그인_확인의_환경과_작업_폴더와_로그인_폴더는_실제_호출과_같다() {
    let f = check_fake(
        &version_says("2.1.316"),
        &auth_says(&logged_in_json("claude.ai", "pro"), 0),
    );
    let mut cfg = config(&f);
    cfg.config_dir = Some(f.login.clone());
    cfg.base_env
        .push(kv("ANTHROPIC_API_KEY", "SIHEOM-BBAEL-GAP"));
    cfg.base_env
        .push(kv("CLAUDE_CODE_OAUTH_TOKEN", "SIHEOM-BBAEL-GAP"));

    let status = ClaudeCliProvider::new(cfg).check().await;

    assert!(matches!(status, CliStatus::LoggedIn { .. }), "{status:?}");
    let env = env_map(&f.read("auth_env"));
    assert_eq!(
        env.get("CLAUDE_CONFIG_DIR").map(PathBuf::from),
        Some(f.login.clone())
    );
    assert!(!env.contains_key("ANTHROPIC_API_KEY"));
    assert!(!env.contains_key("CLAUDE_CODE_OAUTH_TOKEN"));
    assert_eq!(env.get("SIHEOM_KEEP").map(String::as_str), Some(ENV_MARK));
    assert_eq!(PathBuf::from(f.read("auth_cwd").trim()), f.work);
}

// 검증: D4
#[tokio::test]
async fn 버전이_2_1_259보다_낮으면_너무_낮음이다() {
    let auth = auth_says(&logged_in_json("claude.ai", "max"), 0);
    let f = check_fake(&version_says("2.1.258 (Claude Code)"), &auth);
    assert_eq!(
        check(&f).await,
        CliStatus::TooOld {
            version: CliVersion {
                major: 2,
                minor: 1,
                patch: 258,
            },
        }
    );
    for (text, expected) in [
        ("2.1.259 (Claude Code)", (2, 1, 259)),
        ("2.2.0", (2, 2, 0)),
        ("3.0.1 (Claude Code)", (3, 0, 1)),
    ] {
        let f = check_fake(&version_says(text), &auth);
        let (major, minor, patch) = expected;
        assert_eq!(
            check(&f).await,
            CliStatus::LoggedIn {
                version: CliVersion {
                    major,
                    minor,
                    patch,
                },
                auth_method: "claude.ai".to_string(),
                subscription_type: Some("max".to_string()),
            },
            "{text}"
        );
    }
}

// 검증: D4
#[tokio::test]
async fn 로그인_안_됨은_끝_코드_1이거나_logged_in이_거짓이다() {
    let not_logged =
        json!({"loggedIn": false, "authMethod": "none", "apiProvider": "firstParty"}).to_string();
    for auth in [
        auth_says(&not_logged, 1),
        auth_says(&not_logged, 0),
        auth_says("JSON이 아님", 1),
    ] {
        let f = check_fake(&version_says("2.1.316"), &auth);
        assert_eq!(check(&f).await, CliStatus::NotLoggedIn, "{auth}");
    }
}

// 검증: V31
#[tokio::test]
async fn 실행_파일이_없으면_설치_안_됨이다() {
    let f = Fake::new(Behave::ok());
    let mut cfg = config(&f);
    cfg.command = None;
    assert_eq!(
        ClaudeCliProvider::new(cfg).check().await,
        CliStatus::NotInstalled
    );
    let mut cfg = config(&f);
    cfg.command = Some(f.root.join("없는-claude"));
    assert_eq!(
        ClaudeCliProvider::new(cfg).check().await,
        CliStatus::NotInstalled
    );
}

// 검증: D4
#[tokio::test]
async fn 읽지_못하면_읽지_못함이다() {
    let f = check_fake(&version_says("2.1.316"), &auth_says("JSON이 아님", 0));
    assert_eq!(check(&f).await, CliStatus::Unreadable);

    let f = check_fake(
        &version_says("Claude Code"),
        &auth_says(&logged_in_json("claude.ai", "max"), 0),
    );
    assert_eq!(check(&f).await, CliStatus::Unreadable);
}

// 검증: D4
#[tokio::test]
async fn 구독이_아닌_로그인은_구독이_아님이다() {
    let f = check_fake(
        &version_says("2.1.316"),
        &auth_says(&logged_in_json("api_key", "max"), 0),
    );
    assert_eq!(
        check(&f).await,
        CliStatus::NotSubscription {
            auth_method: "api_key".to_string(),
        }
    );
}

// 검증: D4
#[tokio::test]
async fn 확인이_제한을_넘기면_시간_초과이고_로그인_확인은_죽이지_않는다() {
    // 버전 확인이 늦다.
    let f = check_fake(
        "/bin/sleep 1\nprintf '2.1.316\\n'\nexit 0",
        &auth_says(&logged_in_json("claude.ai", "max"), 0),
    );
    let mut cfg = config(&f);
    cfg.version_timeout = Duration::from_millis(150);
    assert_eq!(
        ClaudeCliProvider::new(cfg).check().await,
        CliStatus::TimedOut
    );

    // 로그인 확인이 늦다. 기다리기만 그만두고 프로세스는 끝까지 돈다.
    let slow_auth = format!(
        "/bin/sleep 1\necho done > \"$L/auth_done\"\n{}",
        auth_says(&logged_in_json("claude.ai", "max"), 0)
    );
    let f = check_fake(&version_says("2.1.316"), &slow_auth);
    let mut cfg = config(&f);
    cfg.auth_timeout = Duration::from_millis(150);
    let started = Instant::now();
    assert_eq!(
        ClaudeCliProvider::new(cfg).check().await,
        CliStatus::TimedOut
    );
    assert!(
        started.elapsed() < Duration::from_millis(800),
        "기다리기를 그만두지 않았다"
    );
    assert!(
        appears_soon(&f.log.join("auth_done")).await,
        "로그인 확인 프로세스를 죽였다"
    );
}
