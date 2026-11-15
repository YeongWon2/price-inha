//! pi-ai 5차: CLI 공급자. docs/dev/modules/ai.md "5차 작업" 1 · 3 · 4 · 5 · 7번.
//!
//! 실제 `claude`를 부르지 않는다. 테스트가 임시 폴더에 가짜 실행 파일(셸 스크립트)을 만들어
//! 경로로 준다. 사용자 `~/.claude`를 열지 않는다(자식 환경은 `PATH`만 준다).
//! 2번(신호 명령을 띄우지 못할 때 물러서기)과 6번(이미 거둔 그룹 번호에 다시 신호하지 않기)은
//! 공개 API로 일으킬 수 없어 여기서 다루지 않는다.
//! macOS에서 새로 쓴 셸 스크립트의 첫 실행이 수백 ms 걸리므로 시간 예산은 넉넉히 잡는다.

use std::ffi::OsStr;
use std::fs;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, Instant};

use pi_ai::{
    AiProvider, ClaudeCliConfig, ClaudeCliProvider, CliCheck, CliStatus, CliVersion,
    CompletionRequest, Message, Role, StopReason, is_own_work_file,
};
use serde_json::json;
use tempfile::TempDir;

const TEST_MODEL: &str = "siheom-model-g";

// ---------- 가짜 CLI ----------

/// 가짜가 할 일. 갈래마다 셸 줄을 받는다.
struct Behave {
    /// `-p` 호출: 인자 · 작업 폴더를 적고 표준 입력을 읽은 뒤 돌릴 셸 줄.
    call: String,
    /// `--version` 갈래의 셸 줄(스스로 `exit` 한다).
    version: String,
    /// `auth status --json` 갈래의 셸 줄(스스로 `exit` 한다).
    auth: String,
}

impl Behave {
    fn ok() -> Self {
        Self {
            call: format!("printf '%s\\n' '{}'\nexit 0", result_line()),
            version: "printf '2.1.316 (Claude Code)\\n'\nexit 0".to_string(),
            auth: format!("printf '%s\\n' '{}'\nexit 0", logged_in_json()),
        }
    }
}

fn result_line() -> String {
    json!({
        "type": "result",
        "subtype": "success",
        "is_error": false,
        "result": "가짜 답",
        "stop_reason": "end_turn",
        "num_turns": 1,
        "permission_denials": [],
        "usage": {"input_tokens": 3, "output_tokens": 4},
    })
    .to_string()
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

fn logged_in() -> CliStatus {
    CliStatus::LoggedIn {
        version: CliVersion {
            major: 2,
            minor: 1,
            patch: 316,
        },
        auth_method: "claude.ai".to_string(),
        subscription_type: Some("max".to_string()),
    }
}

struct Fake {
    _root: TempDir,
    log: PathBuf,
    bin: PathBuf,
    work: PathBuf,
}

impl Fake {
    fn new(behave: &Behave) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(dir.path()).unwrap();
        let log = root.join("log");
        let work = root.join("work");
        for d in [&log, &work] {
            fs::create_dir(d).unwrap();
        }
        let bin = root.join("claude");
        fs::write(&bin, script(&log, behave)).unwrap();
        fs::set_permissions(&bin, fs::Permissions::from_mode(0o755)).unwrap();
        Self {
            _root: dir,
            log,
            bin,
            work,
        }
    }

    fn mark(&self, name: &str) -> PathBuf {
        self.log.join(name)
    }

    fn pid(&self, name: &str) -> u32 {
        fs::read_to_string(self.log.join(name))
            .unwrap()
            .trim()
            .parse()
            .unwrap()
    }

    /// 가짜가 적은 줄들(없으면 빈 목록).
    fn lines(&self, name: &str) -> Vec<String> {
        fs::read_to_string(self.log.join(name))
            .unwrap_or_default()
            .lines()
            .map(str::to_string)
            .collect()
    }

    /// 시험이 끝날 때(실패해도) 가짜가 적은 번호의 프로세스를 모두 치우는 것.
    fn cleanup(&self) -> Cleanup {
        Cleanup(self.log.clone())
    }
}

/// 가짜 실행 파일의 셸 글. 부모 환경이 없어도 돌도록 명령은 절대 경로로 부른다.
fn script(log: &Path, b: &Behave) -> String {
    let log = log.display();
    format!(
        r#"#!/bin/sh
L='{log}'
case "$1" in
--version)
{version}
;;
auth)
{auth}
;;
esac
echo $$ > "$L/call_pid"
prev=''
for a in "$@"; do
  if [ "$prev" = "--system-prompt-file" ]; then /usr/bin/basename "$a" >> "$L/sysname"; fi
  prev="$a"
done
/bin/ls -A > "$L/workls"
/bin/cat > /dev/null
{call}
"#,
        version = b.version,
        auth = b.auth,
        call = b.call,
    )
}

fn config(f: &Fake) -> ClaudeCliConfig {
    ClaudeCliConfig {
        command: Some(f.bin.clone()),
        config_dir: None,
        work_dir: f.work.clone(),
        base_env: vec![("PATH".to_string(), "/usr/bin:/bin".to_string())],
        timeout: Duration::from_secs(20),
        max_concurrency: 2,
        max_output_bytes: 1_000_000,
        kill_grace: Duration::from_millis(300),
        version_timeout: Duration::from_millis(1500),
        auth_timeout: Duration::from_millis(1500),
    }
}

fn request(system: Option<&str>) -> CompletionRequest {
    CompletionRequest {
        model: TEST_MODEL.to_string(),
        system: system.map(str::to_string),
        messages: vec![Message {
            role: Role::User,
            content: "오늘 시장은?".to_string(),
        }],
        max_tokens: 100,
    }
}

/// `ps`가 본 상태 글자. 없으면(거둬졌으면) `None`.
fn stat(pid: u32) -> Option<String> {
    let out = Command::new("/bin/ps")
        .args(["-o", "stat=", "-p", &pid.to_string()])
        .output()
        .unwrap();
    let stat = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (out.status.success() && !stat.is_empty()).then_some(stat)
}

/// 살아 있는가(좀비는 죽은 것으로 본다).
fn alive(pid: u32) -> bool {
    stat(pid).is_some_and(|s| !s.starts_with('Z'))
}

/// `limit` 안에 죽으면(좀비 포함) 참.
async fn dies_within(pid: u32, limit: Duration) -> bool {
    let started = Instant::now();
    while started.elapsed() < limit {
        if !alive(pid) {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    !alive(pid)
}

/// `limit` 안에 파일이 생기면 참. 짧게 자주 본다.
async fn appears_within(path: &Path, limit: Duration) -> bool {
    let started = Instant::now();
    while started.elapsed() < limit {
        if path.exists() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    path.exists()
}

/// 시험이 끝날 때(실패해도) 표시 폴더의 `_pid` · `_grandchild` 파일에 적힌 프로세스를 `SIGKILL`로 치운다.
struct Cleanup(PathBuf);

impl Drop for Cleanup {
    fn drop(&mut self) {
        let Ok(entries) = fs::read_dir(&self.0) else {
            return;
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if !(name.ends_with("_pid") || name.ends_with("_grandchild")) {
                continue;
            }
            let Ok(text) = fs::read_to_string(entry.path()) else {
                continue;
            };
            for pid in text.split_whitespace() {
                let _ = Command::new("/bin/kill").args(["-KILL", pid]).output();
            }
        }
    }
}

/// 손주를 띄워 번호를 남기고, 제 번호를 남긴 뒤 끝나지 않는 셸 줄(표준 출력을 쥔 채 잔다).
fn hang(mark: &str) -> String {
    format!(
        "echo $$ > \"$L/{mark}_pid\"\n/bin/sleep 30 </dev/null >/dev/null 2>&1 &\necho $! > \"$L/{mark}_grandchild\"\nexec /bin/sleep 30"
    )
}

/// 표준 출력을 쥐지 않는 손주를 묶음 안에 남겨 두고, `then`으로 정상으로 끝나는 셸 줄.
fn leave_grandchild(mark: &str, then: &str) -> String {
    format!(
        "/bin/sleep 30 </dev/null >/dev/null 2>&1 &\necho $! > \"$L/{mark}_grandchild\"\n{then}"
    )
}

// ---------- 1. 제한이 아주 커도 공황하지 않는다 ----------

// 검증: D4
#[tokio::test]
async fn 호출_제한이_duration_max여도_공황하지_않고_답을_받는다() {
    let f = Fake::new(&Behave::ok());
    let _cleanup = f.cleanup();
    let mut c = config(&f);
    c.timeout = Duration::MAX;
    let provider = ClaudeCliProvider::new(c);

    let resp = tokio::time::timeout(Duration::from_secs(10), provider.complete(request(None)))
        .await
        .expect("호출이 돌아오지 않았다")
        .expect("제한이 아주 커도 정상 호출은 성공해야 한다");

    assert_eq!(resp.text, "가짜 답");
    assert_eq!(resp.stop_reason, StopReason::EndTurn);
    assert_eq!(resp.usage.input_tokens, 3);
    assert_eq!(resp.usage.output_tokens, 4);
}

// 검증: D4
#[tokio::test]
async fn 확인_제한이_duration_max여도_공황하지_않고_로그인됨을_돌려준다() {
    let f = Fake::new(&Behave::ok());
    let _cleanup = f.cleanup();
    let mut c = config(&f);
    c.version_timeout = Duration::MAX;
    c.auth_timeout = Duration::MAX;
    let provider = ClaudeCliProvider::new(c);

    let status = tokio::time::timeout(Duration::from_secs(10), provider.check())
        .await
        .expect("확인이 돌아오지 않았다");

    assert_eq!(status, logged_in());
}

// 검증: D4
#[tokio::test]
async fn 확인_제한이_duration_max여도_자세한_확인이_결과를_낸다() {
    let f = Fake::new(&Behave::ok());
    let _cleanup = f.cleanup();
    let mut c = config(&f);
    c.version_timeout = Duration::MAX;
    c.auth_timeout = Duration::MAX;
    let provider = ClaudeCliProvider::new(c);

    let detail = tokio::time::timeout(Duration::from_secs(10), provider.check_detailed())
        .await
        .expect("확인이 돌아오지 않았다");

    assert_eq!(detail, CliCheck::Status(logged_in()));
}

// ---------- 3. 꺼질 때 확인 프로세스도 끝낸다 ----------

// 검증: D4
#[tokio::test]
async fn 꺼질_때_도는_중인_로그인_확인을_묶음째_끝내고_거둔다() {
    let mut b = Behave::ok();
    b.auth = hang("auth");
    let f = Fake::new(&b);
    let _cleanup = f.cleanup();
    let mut c = config(&f);
    c.auth_timeout = Duration::from_secs(20);
    let provider = Arc::new(ClaudeCliProvider::new(c));

    let checking = tokio::spawn({
        let provider = Arc::clone(&provider);
        async move { provider.check().await }
    });
    assert!(
        appears_within(&f.mark("auth_grandchild"), Duration::from_secs(5)).await,
        "로그인 확인 가짜가 뜨지 않았다"
    );
    let pid = f.pid("auth_pid");
    let grandchild = f.pid("auth_grandchild");

    tokio::time::timeout(Duration::from_secs(5), provider.shutdown())
        .await
        .expect("끝내기가 돌아오지 않았다");

    assert!(
        !alive(pid),
        "끝내기가 돌아왔는데 도는 중인 로그인 확인 프로세스가 살아 있다"
    );
    assert!(
        dies_within(grandchild, Duration::from_secs(1)).await,
        "끝내기 뒤에 로그인 확인 프로세스 묶음의 손주가 남았다"
    );
    let status = tokio::time::timeout(Duration::from_secs(3), checking)
        .await
        .expect("끝내기 뒤에도 확인이 돌아오지 않았다")
        .unwrap();
    assert!(
        !matches!(status, CliStatus::LoggedIn { .. }),
        "끝낸 확인이 로그인됨을 돌려주었다: {status:?}"
    );
}

// 검증: D4
#[tokio::test]
async fn 꺼질_때_기다리기를_그만둔_확인_프로세스도_끝내고_거둔다() {
    let mut b = Behave::ok();
    b.auth = hang("auth");
    let f = Fake::new(&b);
    let _cleanup = f.cleanup();
    let mut c = config(&f);
    c.auth_timeout = Duration::from_millis(400);
    let provider = ClaudeCliProvider::new(c);

    let status = tokio::time::timeout(Duration::from_secs(10), provider.check())
        .await
        .expect("확인이 돌아오지 않았다");
    assert_eq!(status, CliStatus::TimedOut);
    assert!(
        appears_within(&f.mark("auth_grandchild"), Duration::from_secs(3)).await,
        "로그인 확인 가짜가 뜨지 않았다"
    );
    let pid = f.pid("auth_pid");
    let grandchild = f.pid("auth_grandchild");
    assert!(
        alive(pid),
        "꺼지는 중이 아닌데 제한을 넘긴 확인을 바로 죽였다(cli-backend.md 5.6-5)"
    );

    tokio::time::timeout(Duration::from_secs(5), provider.shutdown())
        .await
        .expect("끝내기가 돌아오지 않았다");

    assert!(
        !alive(pid),
        "끝내기가 돌아왔는데 기다리기를 그만둔 확인 프로세스가 살아 있다"
    );
    assert!(
        dies_within(grandchild, Duration::from_secs(1)).await,
        "끝내기 뒤에 확인 프로세스 묶음의 손주가 남았다"
    );
}

// ---------- 4. 정상으로 끝난 확인의 묶음도 치운다 ----------

// 검증: D4
#[tokio::test]
async fn 버전_확인이_정상으로_끝나도_묶음에_남은_손주를_끝낸다() {
    let mut b = Behave::ok();
    b.version = leave_grandchild("version", "printf '2.1.316 (Claude Code)\\n'\nexit 0");
    let f = Fake::new(&b);
    let _cleanup = f.cleanup();
    let provider = ClaudeCliProvider::new(config(&f));

    let status = tokio::time::timeout(Duration::from_secs(10), provider.check())
        .await
        .expect("확인이 돌아오지 않았다");

    assert_eq!(status, logged_in());
    let grandchild = f.pid("version_grandchild");
    assert!(
        dies_within(grandchild, Duration::from_secs(3)).await,
        "정상으로 끝난 버전 확인의 묶음에 손주가 남았다"
    );
}

// 검증: D4
#[tokio::test]
async fn 로그인_확인이_정상으로_끝나도_묶음에_남은_손주를_끝낸다() {
    let mut b = Behave::ok();
    b.auth = leave_grandchild(
        "auth",
        &format!("printf '%s\\n' '{}'\nexit 0", logged_in_json()),
    );
    let f = Fake::new(&b);
    let _cleanup = f.cleanup();
    let provider = ClaudeCliProvider::new(config(&f));

    let status = tokio::time::timeout(Duration::from_secs(10), provider.check())
        .await
        .expect("확인이 돌아오지 않았다");

    assert_eq!(status, logged_in());
    let grandchild = f.pid("auth_grandchild");
    assert!(
        dies_within(grandchild, Duration::from_secs(3)).await,
        "정상으로 끝난 로그인 확인의 묶음에 손주가 남았다"
    );
}

// ---------- 5. "앞 확인이 아직 돈다"를 가른다 ----------

// 검증: D4
#[tokio::test]
async fn 자세한_확인은_정상_결과를_상태로_돌려준다() {
    let f = Fake::new(&Behave::ok());
    let _cleanup = f.cleanup();
    let provider = ClaudeCliProvider::new(config(&f));
    let detail = tokio::time::timeout(Duration::from_secs(10), provider.check_detailed())
        .await
        .expect("확인이 돌아오지 않았다");
    assert_eq!(detail, CliCheck::Status(logged_in()));

    let mut c = config(&f);
    c.command = None;
    let missing = ClaudeCliProvider::new(c).check_detailed().await;
    assert_eq!(missing, CliCheck::Status(CliStatus::NotInstalled));
}

// 검증: D4
#[tokio::test]
async fn 앞_확인이_아직_돌면_자세한_확인은_제한_넘김과_따로_가른다() {
    let mut b = Behave::ok();
    b.auth = hang("auth");
    let f = Fake::new(&b);
    let _cleanup = f.cleanup();
    let mut c = config(&f);
    c.auth_timeout = Duration::from_millis(400);
    let provider = ClaudeCliProvider::new(c);

    let first = tokio::time::timeout(Duration::from_secs(10), provider.check_detailed())
        .await
        .expect("첫 확인이 돌아오지 않았다");
    assert_eq!(
        first,
        CliCheck::Status(CliStatus::TimedOut),
        "이번 확인 자신이 제한을 넘긴 것은 상태의 TimedOut이다"
    );
    assert!(
        appears_within(&f.mark("auth_pid"), Duration::from_secs(3)).await,
        "로그인 확인 가짜가 뜨지 않았다"
    );
    let pid = f.pid("auth_pid");
    assert!(alive(pid), "앞 확인 프로세스가 벌써 죽었다");

    let second = tokio::time::timeout(Duration::from_secs(5), provider.check_detailed())
        .await
        .expect("둘째 확인이 돌아오지 않았다");
    assert_eq!(
        second,
        CliCheck::PreviousProbeAlive,
        "앞 확인이 아직 살아 있으면 따로 된 갈래로 알려야 한다"
    );
    let plain = tokio::time::timeout(Duration::from_secs(5), provider.check())
        .await
        .expect("셋째 확인이 돌아오지 않았다");
    assert_eq!(
        plain,
        CliStatus::TimedOut,
        "check()는 지금처럼 TimedOut으로 보인다"
    );

    tokio::time::timeout(Duration::from_secs(5), provider.shutdown())
        .await
        .expect("끝내기가 돌아오지 않았다");
}

// ---------- 7. 작업 폴더의 자기 임시 파일 ----------

// 검증: D4 E38
#[tokio::test]
async fn 호출이_작업_폴더에_두는_시스템_글_임시_파일은_자기_작업_파일이다() {
    let f = Fake::new(&Behave::ok());
    let _cleanup = f.cleanup();
    let provider = ClaudeCliProvider::new(config(&f));

    let resp = tokio::time::timeout(
        Duration::from_secs(10),
        provider.complete(request(Some("시스템 글"))),
    )
    .await
    .expect("호출이 돌아오지 않았다")
    .expect("정상 호출이 실패했다");
    assert_eq!(resp.text, "가짜 답");

    let names = f.lines("sysname");
    assert_eq!(
        names.len(),
        1,
        "시스템 글 파일 인자를 받지 못했다: {names:?}"
    );
    assert!(
        is_own_work_file(OsStr::new(&names[0])),
        "이 모듈이 쓴 시스템 글 임시 파일을 알아보지 못했다: {}",
        names[0]
    );
    let listed = f.lines("workls");
    assert!(
        !listed.is_empty(),
        "호출 중 작업 폴더에 임시 파일이 보이지 않았다"
    );
    for name in &listed {
        assert!(
            is_own_work_file(OsStr::new(name)),
            "호출 중 작업 폴더에 있던 파일을 자기 것으로 알아보지 못했다: {name}"
        );
    }
}

// 검증: D4
#[test]
fn 시스템_글_임시_파일_이름_모양은_자기_작업_파일이다() {
    for name in [".pi-ai-system-4242-0.txt", ".pi-ai-system-1-17.txt"] {
        assert!(is_own_work_file(OsStr::new(name)), "{name}");
    }
}

// 검증: D4
#[test]
fn 다른_이름은_자기_작업_파일이_아니다() {
    for name in [
        "",
        "notes.txt",
        "CLAUDE.md",
        ".claude",
        ".DS_Store",
        "pi-ai-system-4242-0.txt",
        "x.pi-ai-system-4242-0.txt",
        ".pi-ai-other-4242-0.txt",
        ".pi-ai",
        ".PI-AI-SYSTEM-4242-0.txt",
    ] {
        assert!(!is_own_work_file(OsStr::new(name)), "{name:?}");
    }
    let not_utf8 = OsStr::from_bytes(b"\xff\xfe.txt");
    assert!(!is_own_work_file(not_utf8));
}
