//! pi-ai 3차: CLI 공급자. docs/dev/modules/ai.md "3차 작업" 9~15번.
//!
//! 실제 `claude`를 부르지 않는다. 테스트가 임시 폴더에 가짜 실행 파일(셸 스크립트)을 만들어
//! 경로로 준다. 끝내기 · 표준 오류 가리기는 원본(OpenClaw `claude-cli`)의 동작
//! (`docs/research/cli-backend.md` 2.7 · 5.6)을 따른다.
//! macOS에서 새로 쓴 셸 스크립트의 첫 실행이 수백 ms 걸리므로 시간 예산은 1.5초 이상으로 잡는다.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, Instant};

use pi_ai::{
    AiError, AiProvider, ClaudeCliConfig, ClaudeCliProvider, CliStatus, CompletionRequest,
    CompletionResponse, Message, Role,
};
use serde_json::{Value, json};
use tempfile::TempDir;

const TEST_MODEL: &str = "siheom-model-d";

// ---------- 가짜 CLI ----------

/// 가짜가 할 일. 갈래마다 셸 줄을 받는다.
struct Behave {
    /// `-p` 호출: 표준 입력을 읽어 둔 뒤 돌릴 셸 줄(끝 코드까지 스스로 정한다).
    call: String,
    /// `--version` 갈래의 셸 줄(스스로 `exit` 한다).
    version: String,
    /// `auth status --json` 갈래의 셸 줄(스스로 `exit` 한다).
    auth: String,
    /// 참이면 `-p` 호출이 표준 입력을 읽지 않는다.
    skip_stdin: bool,
}

impl Behave {
    fn ok() -> Self {
        Self {
            call: emit_ok(),
            version: "printf '2.1.316 (Claude Code)\\n'\nexit 0".to_string(),
            auth: format!("printf '%s\\n' '{}'\nexit 0", logged_in_json()),
            skip_stdin: false,
        }
    }

    fn call(call: String) -> Self {
        Self { call, ..Self::ok() }
    }
}

fn logged_in_json() -> String {
    json!({
        "loggedIn": true,
        "authMethod": "claude.ai",
        "apiProvider": "firstParty",
        "subscriptionType": "max",
    })
    .to_string()
}

fn ok_line() -> Value {
    json!({
        "type": "result",
        "subtype": "success",
        "is_error": false,
        "result": "안녕하세요.",
        "stop_reason": "end_turn",
        "num_turns": 1,
        "permission_denials": [],
        "usage": {"input_tokens": 12, "output_tokens": 5},
        "session_id": "siheom",
    })
}

/// 성공 결과 줄을 내고 0으로 끝나는 셸 줄.
fn emit_ok() -> String {
    format!("printf '%s\\n' '{}'\nexit 0", ok_line())
}

struct Fake {
    _root: TempDir,
    root: PathBuf,
    log: PathBuf,
    bin: PathBuf,
    work: PathBuf,
}

impl Fake {
    fn new(behave: Behave) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(dir.path()).unwrap();
        let log = root.join("log");
        let work = root.join("work");
        for d in [&log, &work] {
            fs::create_dir(d).unwrap();
        }
        let bin = root.join("claude");
        fs::write(&bin, script(&log, &behave)).unwrap();
        fs::set_permissions(&bin, fs::Permissions::from_mode(0o755)).unwrap();
        Self {
            _root: dir,
            root,
            log,
            bin,
            work,
        }
    }

    fn read(&self, name: &str) -> String {
        fs::read_to_string(self.log.join(name)).unwrap()
    }

    fn has(&self, name: &str) -> bool {
        self.log.join(name).exists()
    }

    fn calls(&self) -> Vec<String> {
        if !self.has("calls") {
            return Vec::new();
        }
        self.read("calls").lines().map(str::to_string).collect()
    }

    fn pid(&self, name: &str) -> u32 {
        self.read(name).trim().parse().unwrap()
    }
}

/// 가짜 실행 파일의 셸 글. 부모 환경이 없어도 돌도록 명령은 절대 경로로 부른다.
fn script(log: &Path, b: &Behave) -> String {
    let log = log.display();
    let stdin = if b.skip_stdin {
        ""
    } else {
        "/bin/cat > \"$L/stdin\""
    };
    format!(
        r#"#!/bin/sh
L='{log}'
printf '%s\n' "$*" >> "$L/calls"
case "$1" in
--version)
{version}
;;
auth)
{auth}
;;
esac
echo $$ > "$L/pid"
{stdin}
{call}
"#,
        version = b.version,
        auth = b.auth,
        call = b.call,
    )
}

/// 손주 프로세스를 띄워 그 번호를 남기는 셸 줄.
const SPAWN_GRANDCHILD: &str =
    "/bin/sleep 5 </dev/null >/dev/null 2>&1 &\necho $! > \"$L/grandchild\"";

fn config(f: &Fake) -> ClaudeCliConfig {
    ClaudeCliConfig {
        command: Some(f.bin.clone()),
        config_dir: None,
        work_dir: f.work.clone(),
        base_env: vec![("PATH".to_string(), "/usr/bin:/bin".to_string())],
        timeout: Duration::from_secs(5),
        max_concurrency: 2,
        max_output_bytes: 1_000_000,
        kill_grace: Duration::from_millis(300),
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

fn request(system: Option<String>, messages: Vec<Message>) -> CompletionRequest {
    CompletionRequest {
        model: TEST_MODEL.to_string(),
        system,
        messages,
        max_tokens: 100,
    }
}

fn simple_request() -> CompletionRequest {
    request(
        Some("너는 시험 담당이다".to_string()),
        vec![user("오늘 시장은?")],
    )
}

async fn call(cfg: ClaudeCliConfig, req: CompletionRequest) -> Result<CompletionResponse, AiError> {
    ClaudeCliProvider::new(cfg).complete(req).await
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

/// `limit` 안에 파일이 생기면 참.
async fn appears_within(path: &Path, limit: Duration) -> bool {
    let started = Instant::now();
    while started.elapsed() < limit {
        if path.exists() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    path.exists()
}

// ---------- 9. 정상으로 끝난 뒤에도 그룹을 치운다 ----------

// 검증: A11
#[tokio::test]
async fn 정상으로_끝난_뒤에도_그룹에_남은_손주를_끝낸다() {
    let f = Fake::new(Behave::call(format!("{SPAWN_GRANDCHILD}\n{}", emit_ok())));

    let response = call(config(&f), simple_request()).await.unwrap();

    assert_eq!(response.text, "안녕하세요.");
    assert!(f.has("grandchild"), "가짜가 손주를 띄우지 않았다");
    assert!(
        dies_soon(f.pid("grandchild")).await,
        "정상으로 끝난 뒤 손주가 살아 있다"
    );
}

// 검증: A11
#[tokio::test]
async fn 실패로_끝난_뒤에도_그룹에_남은_손주를_끝낸다() {
    let fail = json!({
        "type": "result",
        "subtype": "success",
        "is_error": true,
        "result": "Not logged in · Please run /login",
        "num_turns": 1,
        "permission_denials": [],
    });
    let f = Fake::new(Behave::call(format!(
        "{SPAWN_GRANDCHILD}\nprintf '%s\\n' '{fail}'\nexit 1"
    )));

    let err = call(config(&f), simple_request()).await.unwrap_err();

    assert_eq!(err, AiError::Auth { status: 401 });
    assert!(
        dies_soon(f.pid("grandchild")).await,
        "실패로 끝난 뒤 손주가 살아 있다"
    );
}

// ---------- 10. 확인 명령이 쌓이지 않는다 ----------

/// 늦게 시작해 파이프 버퍼(64KB)보다 많이 쓴 뒤 표시를 남기고 끝나는 셸 줄.
/// 표시는 쓰기가 끝까지 성공했을 때만 남는다(EPIPE · SIGPIPE로 죽으면 남지 않는다).
fn slow_big_output(mark: &str) -> String {
    format!(
        "/bin/sleep 0.5\n/bin/dd if=/dev/zero bs=1000 count=300 2>/dev/null && echo done > \"$L/{mark}\"\nexit 0"
    )
}

// 검증: D4
#[tokio::test]
async fn 시간_초과된_로그인_확인은_출력을_끝까지_읽어_스스로_끝나게_한다() {
    let mut b = Behave::ok();
    b.auth = slow_big_output("auth_done");
    let f = Fake::new(b);
    let mut cfg = config(&f);
    cfg.auth_timeout = Duration::from_millis(150);

    let status = ClaudeCliProvider::new(cfg).check().await;

    assert_eq!(status, CliStatus::TimedOut);
    // 읽는 쪽이 없으면 가짜는 쓰기에서 막히거나(EPIPE · SIGPIPE) 죽어 표시를 남기지 못한다.
    assert!(
        appears_within(&f.log.join("auth_done"), Duration::from_secs(4)).await,
        "로그인 확인 프로세스가 스스로 끝나지 못했다"
    );
}

// 검증: D4
#[tokio::test]
async fn 시간_초과된_버전_확인도_출력을_끝까지_읽어_스스로_끝나게_한다() {
    let mut b = Behave::ok();
    b.version = slow_big_output("version_done");
    let f = Fake::new(b);
    let mut cfg = config(&f);
    cfg.version_timeout = Duration::from_millis(150);

    let status = ClaudeCliProvider::new(cfg).check().await;

    assert_eq!(status, CliStatus::TimedOut);
    assert!(
        appears_within(&f.log.join("version_done"), Duration::from_secs(4)).await,
        "버전 확인 프로세스가 스스로 끝나지 못했다"
    );
}

/// 지금 돌고 있는 확인 프로세스 수를 적고 0.3초 머무는 셸 줄.
const COUNT_RUNNING: &str = r#"/bin/mkdir -p "$L/running"
: > "$L/running/$$"
/bin/ls "$L/running" | /usr/bin/wc -l >> "$L/counts"
/bin/sleep 0.3
/bin/rm -f "$L/running/$$""#;

// 검증: D4
#[tokio::test]
async fn 확인은_한_공급자에서_동시에_하나만_돈다() {
    let mut b = Behave::ok();
    b.version = format!("{COUNT_RUNNING}\nprintf '2.1.316\\n'\nexit 0");
    b.auth = format!(
        "{COUNT_RUNNING}\nprintf '%s\\n' '{}'\nexit 0",
        logged_in_json()
    );
    let f = Fake::new(b);
    let provider = ClaudeCliProvider::new(config(&f));

    let (first, second) = tokio::join!(provider.check(), provider.check());

    assert!(matches!(first, CliStatus::LoggedIn { .. }), "{first:?}");
    assert!(matches!(second, CliStatus::LoggedIn { .. }), "{second:?}");
    let counts: Vec<String> = f
        .read("counts")
        .lines()
        .map(|l| l.trim().to_string())
        .collect();
    assert_eq!(counts.len(), 4, "버전 2번 + 로그인 2번");
    assert!(
        counts.iter().all(|c| c == "1"),
        "확인이 겹쳐 돌았다: {counts:?}"
    );
}

// ---------- 11. NotFound를 나눈다 ----------

// 검증: V31
#[tokio::test]
async fn 작업_폴더가_없으면_띄우지_않고_연결_실패다() {
    let f = Fake::new(Behave::ok());
    let mut cfg = config(&f);
    cfg.work_dir = f.root.join("없는-작업-폴더");

    let err = call(cfg, simple_request()).await.unwrap_err();

    assert_eq!(err, AiError::Connection);
    assert!(f.calls().is_empty(), "가짜를 띄웠다");
}

// 검증: V31
#[tokio::test]
async fn 작업_폴더가_없으면_확인은_설치_안_됨이_아니라_읽지_못함이다() {
    let f = Fake::new(Behave::ok());
    let mut cfg = config(&f);
    cfg.work_dir = f.root.join("없는-작업-폴더");

    let status = ClaudeCliProvider::new(cfg).check().await;

    assert_eq!(status, CliStatus::Unreadable);
    assert!(f.calls().is_empty(), "가짜를 띄웠다");
}

// ---------- 12. 끝내기 함수 ----------

// 검증: A11
#[tokio::test]
async fn 끝내기는_살아_있는_자식_그룹을_sigterm_유예_sigkill로_끝내고_기다린다() {
    // SIGTERM을 받으면 표시만 남기고 계속 자는 가짜. 유예 뒤 SIGKILL로만 끝난다.
    let f = Fake::new(Behave::call(format!(
        "trap 'echo term > \"$L/got_term\"' TERM\n{SPAWN_GRANDCHILD}\n/bin/sleep 5\n/bin/sleep 5\n/bin/sleep 5\nexit 0"
    )));
    let mut cfg = config(&f);
    cfg.timeout = Duration::from_secs(20);
    cfg.kill_grace = Duration::from_millis(300);
    let provider = Arc::new(ClaudeCliProvider::new(cfg));

    let running = {
        let provider = provider.clone();
        tokio::spawn(async move { provider.complete(simple_request()).await })
    };
    assert!(
        appears_within(&f.log.join("grandchild"), Duration::from_secs(3)).await,
        "가짜가 뜨지 않았다"
    );
    let pid = f.pid("pid");

    let started = Instant::now();
    tokio::time::timeout(Duration::from_secs(5), provider.shutdown())
        .await
        .unwrap();

    assert!(
        started.elapsed() >= Duration::from_millis(250),
        "유예를 기다리지 않았다: {:?}",
        started.elapsed()
    );
    assert!(!alive(pid), "끝내기가 돌아왔는데 가짜가 살아 있다");
    assert!(
        dies_soon(f.pid("grandchild")).await,
        "가짜의 자식이 살아 있다"
    );
    assert!(f.has("got_term"), "SIGTERM을 먼저 보내지 않았다");
    let result = tokio::time::timeout(Duration::from_secs(2), running)
        .await
        .unwrap()
        .unwrap();
    assert!(result.is_err(), "끝낸 호출이 성공했다");
}

// 검증: A11
#[tokio::test]
async fn 끝낸_뒤의_호출은_띄우지_않고_연결_실패다() {
    let f = Fake::new(Behave::ok());
    let provider = ClaudeCliProvider::new(config(&f));

    provider.shutdown().await;
    let err = provider.complete(simple_request()).await.unwrap_err();

    assert_eq!(err, AiError::Connection);
    assert!(f.calls().is_empty(), "끝낸 뒤에 가짜를 띄웠다");
}

// 검증: A11
#[tokio::test]
async fn 살아_있는_자식이_없으면_끝내기는_곧바로_돌아온다() {
    let f = Fake::new(Behave::ok());
    let provider = ClaudeCliProvider::new(config(&f));
    provider.complete(simple_request()).await.unwrap();

    let started = Instant::now();
    provider.shutdown().await;

    assert!(
        started.elapsed() < Duration::from_millis(250),
        "{:?}",
        started.elapsed()
    );
}

// 검증: A11
#[tokio::test]
async fn 끝내기는_동시에_산_자식_모두를_끝낸다() {
    let f = Fake::new(Behave::call(format!(
        "/bin/mkdir -p \"$L/pids\"\n: > \"$L/pids/$$\"\n/bin/sleep 5\n{}",
        emit_ok()
    )));
    let mut cfg = config(&f);
    cfg.timeout = Duration::from_secs(20);
    let provider = Arc::new(ClaudeCliProvider::new(cfg));

    let tasks: Vec<_> = (0..2)
        .map(|_| {
            let provider = provider.clone();
            tokio::spawn(async move { provider.complete(simple_request()).await })
        })
        .collect();
    let pids_dir = f.log.join("pids");
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(4) {
        if fs::read_dir(&pids_dir).map(|d| d.count()).unwrap_or(0) == 2 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let pids: Vec<u32> = fs::read_dir(&pids_dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().parse().unwrap())
        .collect();
    assert_eq!(pids.len(), 2, "가짜 둘이 뜨지 않았다");

    tokio::time::timeout(Duration::from_secs(5), provider.shutdown())
        .await
        .unwrap();

    for pid in pids {
        assert!(!alive(pid), "가짜 {pid}가 살아 있다");
    }
    for task in tasks {
        let result = tokio::time::timeout(Duration::from_secs(2), task)
            .await
            .unwrap()
            .unwrap();
        assert!(result.is_err());
    }
}

// ---------- 13. 끝내기 전 표준 입력 닫기 ----------

// 검증: A11
#[tokio::test]
async fn 끝낼_때_표준_입력이_열려_있으면_먼저_닫는다() {
    // 표준 입력을 읽지 않는 가짜. SIGTERM을 받으면 남은 표준 입력을 끝까지 읽고 표시를 남긴다.
    // 표준 입력이 열린 채면 읽기가 끝나지 않아 표시가 생기지 않는다(유예 뒤 SIGKILL).
    let mut b = Behave::call(
        "trap '/bin/cat > /dev/null; echo eof > \"$L/stdin_eof\"; exit 0' TERM\n/bin/sleep 5\nexit 0"
            .to_string(),
    );
    b.skip_stdin = true;
    let f = Fake::new(b);
    let mut cfg = config(&f);
    cfg.timeout = Duration::from_millis(1500);
    cfg.kill_grace = Duration::from_secs(2);
    // 파이프 버퍼보다 큰 표준 입력: 가짜가 읽지 않으니 쓰기가 끝나지 않는다.
    let big = "x".repeat(1_000_000);

    let err = call(cfg, request(None, vec![user(&big)]))
        .await
        .unwrap_err();

    assert_eq!(err, AiError::Timeout);
    assert!(
        appears_within(&f.log.join("stdin_eof"), Duration::from_secs(2)).await,
        "끝낼 때 표준 입력이 닫히지 않았다"
    );
}

// ---------- 14. 표준 오류 가리기 ----------

/// 결과 줄 없이 `stderr`를 내고 1로 끝나는 가짜의 오류.
async fn error_for_stderr(stderr: &str) -> AiError {
    let f = Fake::new(Behave::call("/bin/cat \"$L/err\" >&2\nexit 1".to_string()));
    fs::write(f.log.join("err"), stderr).unwrap();
    call(config(&f), simple_request()).await.unwrap_err()
}

fn message_of(err: AiError) -> String {
    match err {
        AiError::InvalidRequest { message } => message,
        other => panic!("잘못된 요청이 아니다: {other:?}"),
    }
}

/// 실제 키처럼 보이지 않는 `sk-ant-` 낱말. 소스에 그 글자가 통째로 남지 않게 조합한다.
fn key_like_word() -> String {
    format!("sk-{}-siheom0001", "ant")
}

// 검증: E38
#[tokio::test]
async fn 표준_오류의_sk_ant_낱말을_가린다() {
    let key = key_like_word();
    let err = error_for_stderr(&format!("auth failed for {key} please retry\n둘째 줄\n")).await;

    assert_eq!(message_of(err), "auth failed for *** please retry");
}

// 검증: E38
#[tokio::test]
async fn 표준_오류의_32자_이상_덩어리를_가린다() {
    let run32 = "aB3_-".repeat(7)[..32].to_string();
    let err = error_for_stderr(&format!("token {run32} end\n")).await;
    assert_eq!(message_of(err), "token *** end");

    let run40 = "Q".repeat(40);
    let err = error_for_stderr(&format!("x={run40};\n")).await;
    assert_eq!(message_of(err), "x=***;");
}

// 검증: E38
#[tokio::test]
async fn 표준_오류의_31자_덩어리는_그대로_둔다() {
    let run31 = "Q".repeat(31);
    let err = error_for_stderr(&format!("token {run31} end\n")).await;

    assert_eq!(message_of(err), format!("token {run31} end"));
}

// 검증: E38
#[tokio::test]
async fn 가린_뒤에_200자로_자른다() {
    // 자른 뒤 가리면 190자 뒤의 덩어리 조각(10자)이 가려지지 않고 남는다.
    let line = format!("{}{}{}", "가".repeat(190), "Q".repeat(40), "가".repeat(10));
    let err = error_for_stderr(&format!("{line}\n")).await;

    let message = message_of(err);
    assert!(!message.contains('Q'), "덩어리 조각이 남았다: {message}");
    assert_eq!(
        message,
        format!("{}***{}", "가".repeat(190), "가".repeat(7))
    );
    assert_eq!(message.chars().count(), 200);
}

// ---------- 15. 시스템 글도 크기 상한에 든다 ----------

// 검증: V31
#[tokio::test]
async fn 시스템_글과_표준_입력을_합쳐_10mb를_넘으면_띄우지_않고_잘못된_요청이다() {
    let f = Fake::new(Behave::ok());
    let system = "s".repeat(5_000_001);
    let talk = "x".repeat(5_000_000);

    let err = call(config(&f), request(Some(system), vec![user(&talk)]))
        .await
        .unwrap_err();

    assert!(
        matches!(err, AiError::InvalidRequest { .. }),
        "오류: {err:?}"
    );
    assert!(f.calls().is_empty(), "가짜를 띄웠다");
    assert!(
        fs::read_dir(&f.work).unwrap().next().is_none(),
        "작업 폴더에 시스템 글 파일이 남았다"
    );
}

// 검증: V31
#[tokio::test]
async fn 시스템_글과_표준_입력을_합쳐_꼭_10mb면_보낸다() {
    let f = Fake::new(Behave::ok());
    let system = "s".repeat(5_000_000);
    let talk = "x".repeat(5_000_000);

    call(config(&f), request(Some(system), vec![user(&talk)]))
        .await
        .unwrap();

    assert_eq!(f.calls().len(), 1);
    assert_eq!(fs::metadata(f.log.join("stdin")).unwrap().len(), 5_000_000);
}
