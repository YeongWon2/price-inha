//! pi-ai 4차: CLI 공급자. docs/dev/modules/ai.md "4차 작업" 2 · 4 · 7번.
//!
//! 실제 `claude`를 부르지 않는다. 테스트가 임시 폴더에 가짜 실행 파일(셸 스크립트)을 만들어
//! 경로로 준다. 사용자 `~/.claude`를 열지 않는다(자식 환경은 `PATH`만 준다).
//! 끝내기의 "표준 입력을 닫고 0.5초"는 원본(OpenClaw `cli-transport.ts`, `docs/research/cli-backend.md`
//! 2.7)과 결정(`docs/dev/decisions.md` 2026-11-15)을 따른다.
//! macOS에서 새로 쓴 셸 스크립트의 첫 실행이 수백 ms 걸리므로 시간 예산은 1.5초 이상으로 잡는다.
//! 1번(띄우는 중의 끝내기)은 프로세스 전체의 자식을 세므로 따로 된 `cli4_shutdown_race.rs`에 있다.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, Instant};

use pi_ai::{
    AiProvider, CLI_STDIN_CLOSE_WAIT, ClaudeCliConfig, ClaudeCliProvider, CliStatus,
    CompletionRequest, Message, Role,
};
use serde_json::json;
use tempfile::TempDir;

const TEST_MODEL: &str = "siheom-model-f";

// ---------- 가짜 CLI ----------

/// 가짜가 할 일. 갈래마다 셸 줄을 받는다.
struct Behave {
    /// `-p` 호출: 표준 입력을 읽어 둔 뒤 돌릴 셸 줄.
    call: String,
    /// `--version` 갈래의 셸 줄(스스로 `exit` 한다).
    version: String,
    /// `auth status --json` 갈래의 셸 줄(스스로 `exit` 한다).
    auth: String,
}

impl Behave {
    fn ok() -> Self {
        Self {
            call: "exit 0".to_string(),
            version: "printf '2.1.316 (Claude Code)\\n'\nexit 0".to_string(),
            auth: format!("printf '%s\\n' '{}'\nexit 0", logged_in_json()),
        }
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

    fn has(&self, name: &str) -> bool {
        self.log.join(name).exists()
    }

    fn pid(&self, name: &str) -> u32 {
        fs::read_to_string(self.log.join(name))
            .unwrap()
            .trim()
            .parse()
            .unwrap()
    }

    /// `prefix`로 시작하는 표시 파일 이름 끝의 번호들.
    fn pids_with(&self, prefix: &str) -> Vec<u32> {
        fs::read_dir(&self.log)
            .unwrap()
            .filter_map(|e| {
                let name = e.unwrap().file_name().to_string_lossy().to_string();
                name.strip_prefix(prefix).and_then(|n| n.parse().ok())
            })
            .collect()
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
echo $$ > "$L/pid"
/bin/cat > "$L/stdin"
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

fn simple_request() -> CompletionRequest {
    CompletionRequest {
        model: TEST_MODEL.to_string(),
        system: None,
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

/// `limit` 안에 프로세스 표에서 사라지면(죽고 거둬지면) 참.
async fn gone_within(pid: u32, limit: Duration) -> bool {
    let started = Instant::now();
    while started.elapsed() < limit {
        if stat(pid).is_none() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    stat(pid).is_none()
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

/// 시험이 끝날 때(실패해도) 적힌 번호의 프로세스를 `SIGKILL`로 치운다.
struct Reaper(Vec<u32>);

impl Drop for Reaper {
    fn drop(&mut self) {
        for pid in &self.0 {
            let _ = Command::new("/bin/kill")
                .args(["-KILL", &pid.to_string()])
                .output();
        }
    }
}

// ---------- 7. 확인 명령이 끝나지 않아도 남지 않는다 ----------

/// 손주를 띄워 번호를 남기고, 제 번호를 남긴 뒤 끝없이 출력하는 셸 줄.
fn endless_output(mark: &str) -> String {
    format!(
        "/bin/sleep 30 </dev/null >/dev/null 2>&1 &\necho $! > \"$L/{mark}_grandchild\"\necho $$ > \"$L/{mark}_pid\"\nexec /usr/bin/yes"
    )
}

// 검증: D4
#[tokio::test]
async fn 끝없이_출력하는_로그인_확인은_시간_제한_뒤_묶음째_끝내고_거둔다() {
    let mut b = Behave::ok();
    b.auth = endless_output("auth");
    let f = Fake::new(&b);

    let status = tokio::time::timeout(
        Duration::from_secs(10),
        ClaudeCliProvider::new(config(&f)).check(),
    )
    .await
    .expect("확인이 돌아오지 않았다");

    assert_eq!(status, CliStatus::TimedOut);
    assert!(f.has("auth_pid"), "로그인 확인 가짜가 뜨지 않았다");
    let pid = f.pid("auth_pid");
    let grandchild = f.pid("auth_grandchild");
    let _reaper = Reaper(vec![pid, grandchild]);
    assert!(
        gone_within(pid, Duration::from_secs(8)).await,
        "끝없이 출력하는 로그인 확인 프로세스가 남았다(또는 거두지 않았다)"
    );
    assert!(
        dies_within(grandchild, Duration::from_secs(3)).await,
        "로그인 확인 프로세스의 자식이 남았다"
    );
}

// 검증: D4
#[tokio::test]
async fn 끝없이_출력하는_버전_확인도_시간_제한_뒤_묶음째_끝내고_거둔다() {
    let mut b = Behave::ok();
    b.version = endless_output("version");
    let f = Fake::new(&b);

    let status = tokio::time::timeout(
        Duration::from_secs(10),
        ClaudeCliProvider::new(config(&f)).check(),
    )
    .await
    .expect("확인이 돌아오지 않았다");

    assert_eq!(status, CliStatus::TimedOut);
    assert!(f.has("version_pid"), "버전 확인 가짜가 뜨지 않았다");
    let pid = f.pid("version_pid");
    let grandchild = f.pid("version_grandchild");
    let _reaper = Reaper(vec![pid, grandchild]);
    assert!(
        gone_within(pid, Duration::from_secs(8)).await,
        "끝없이 출력하는 버전 확인 프로세스가 남았다(또는 거두지 않았다)"
    );
    assert!(
        dies_within(grandchild, Duration::from_secs(3)).await,
        "버전 확인 프로세스의 자식이 남았다"
    );
}

// ---------- 2. 멈춘 로그인 확인 프로세스가 쌓이지 않는다 ----------

/// 제 번호를 `auth-<번호>`로 남기고, 그때 살아 있는 다른 로그인 확인 프로세스가 있으면
/// `overlap`을 남긴 뒤 오래 자는 셸 줄. 판단을 마치면 `checked-<번호>`를 남긴다.
const STUCK_AUTH: &str = r#": > "$L/auth-$$"
for f in "$L"/auth-*; do
  p=${f##*/auth-}
  [ "$p" = "$$" ] && continue
  s=$(/bin/ps -o stat= -p "$p" 2>/dev/null)
  case "$s" in
    ""|Z*) ;;
    *) : > "$L/overlap" ;;
  esac
done
: > "$L/checked-$$"
exec /bin/sleep 30"#;

// 검증: D4
#[tokio::test]
async fn 앞_확인의_프로세스가_살아_있으면_새로_띄우지_않는다() {
    let mut b = Behave::ok();
    b.auth = STUCK_AUTH.to_string();
    let f = Fake::new(&b);
    let mut cfg = config(&f);
    // 부르는 쪽이 첫 확인을 버리기 전에 제한이 지나지 않게 넉넉히 둔다.
    cfg.auth_timeout = Duration::from_secs(4);
    let provider = Arc::new(ClaudeCliProvider::new(cfg));

    // 첫 확인: 로그인 확인이 뜬 뒤 부르는 쪽이 기다리기를 그만둔다(작업을 버린다).
    let first = {
        let provider = provider.clone();
        tokio::spawn(async move { provider.check().await })
    };
    let started = Instant::now();
    while f.pids_with("checked-").is_empty() && started.elapsed() < Duration::from_secs(4) {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    first.abort();
    let _ = first.await;
    let first_pids = f.pids_with("auth-");
    assert_eq!(first_pids.len(), 1, "첫 로그인 확인이 뜨지 않았다");
    let mut reaper = Reaper(first_pids.clone());

    // 둘째 확인.
    let status = tokio::time::timeout(Duration::from_secs(10), provider.check())
        .await
        .expect("둘째 확인이 돌아오지 않았다");

    let all = f.pids_with("auth-");
    reaper.0.clone_from(&all);
    if all.len() == 1 {
        // 새로 띄우지 않았다: "확인하지 못함"이다.
        assert!(
            matches!(status, CliStatus::TimedOut | CliStatus::Unreadable),
            "새로 띄우지 않았는데 {status:?}"
        );
    } else {
        // 새로 띄웠다면 앞의 것은 이미 끝나 있어야 한다(겹쳐 살아 있으면 안 된다).
        let waited = Instant::now();
        while f.pids_with("checked-").len() < all.len() && waited.elapsed() < Duration::from_secs(2)
        {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert!(
            !f.has("overlap"),
            "앞 확인 프로세스가 살아 있는데 새로 띄웠다: {all:?}"
        );
    }
}

// 검증: D4
#[tokio::test]
async fn 확인을_거듭_버려도_살아_있는_로그인_확인_프로세스는_하나를_넘지_않는다() {
    let mut b = Behave::ok();
    b.auth = STUCK_AUTH.to_string();
    let f = Fake::new(&b);
    let mut cfg = config(&f);
    cfg.auth_timeout = Duration::from_secs(4);
    let provider = Arc::new(ClaudeCliProvider::new(cfg));
    let mut reaper = Reaper(Vec::new());

    for _ in 0..3 {
        let before = f.pids_with("checked-").len();
        let task = {
            let provider = provider.clone();
            tokio::spawn(async move { provider.check().await })
        };
        let started = Instant::now();
        // 새 확인 프로세스가 판단을 남기거나, 확인이 스스로 돌아올 때까지.
        while f.pids_with("checked-").len() == before
            && !task.is_finished()
            && started.elapsed() < Duration::from_secs(3)
        {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        task.abort();
        let _ = task.await;
        reaper.0 = f.pids_with("auth-");
    }

    let live: Vec<u32> = f
        .pids_with("auth-")
        .into_iter()
        .filter(|p| alive(*p))
        .collect();
    assert!(
        live.len() <= 1,
        "살아 있는 확인 프로세스가 쌓였다: {live:?}"
    );
    assert!(!f.has("overlap"), "확인 프로세스가 겹쳐 살아 있었다");
}

// ---------- 4. 끝낼 때 표준 입력을 닫고 0.5초 기다린다 ----------

// 검증: A11
#[test]
fn 표준_입력을_닫은_뒤_기다리는_시간은_0_5초다() {
    assert_eq!(CLI_STDIN_CLOSE_WAIT, Duration::from_millis(500));
}

// 검증: A11
#[tokio::test]
async fn 끝낼_때_0_5초_안에_스스로_끝나는_자식에게는_sigterm을_보내지_않는다() {
    // 뜬 표시를 남긴 뒤 0.3초 안에 스스로 끝나는 가짜. SIGTERM을 받으면 표시를 남긴다.
    let b = Behave {
        call: "trap 'echo term > \"$L/got_term\"; exit 143' TERM\n/bin/sleep 0.3 &\ns=$!\necho ready > \"$L/ready\"\nwait $s\necho clean > \"$L/clean\"\nexit 0".to_string(),
        ..Behave::ok()
    };
    let f = Fake::new(&b);
    let mut cfg = config(&f);
    cfg.kill_grace = Duration::from_secs(2);
    let provider = Arc::new(ClaudeCliProvider::new(cfg));

    let running = {
        let provider = provider.clone();
        tokio::spawn(async move { provider.complete(simple_request()).await })
    };
    assert!(
        appears_within(&f.log.join("ready"), Duration::from_secs(4)).await,
        "가짜가 뜨지 않았다"
    );
    let _reaper = Reaper(vec![f.pid("pid")]);

    tokio::time::timeout(Duration::from_secs(5), provider.shutdown())
        .await
        .expect("끝내기가 돌아오지 않았다");

    assert!(
        !f.has("got_term"),
        "표준 입력을 닫고 0.5초 기다리지 않고 SIGTERM을 보냈다"
    );
    assert!(f.has("clean"), "가짜가 스스로 끝나지 못했다");
    let _ = tokio::time::timeout(Duration::from_secs(2), running).await;
}

// 검증: A11
#[tokio::test]
async fn 끝낼_때_스스로_끝나지_않는_자식에게는_0_5초_뒤에_sigterm을_보낸다() {
    // 표준 입력이 닫혀도 끝나지 않고, SIGTERM을 받으면 표시를 남기고 끝나는 가짜.
    let b = Behave {
        call: "trap 'echo term > \"$L/got_term\"; exit 143' TERM\necho ready > \"$L/ready\"\n/bin/sleep 10 &\nwait $!\nexit 0".to_string(),
        ..Behave::ok()
    };
    let f = Fake::new(&b);
    let mut cfg = config(&f);
    cfg.kill_grace = Duration::from_secs(2);
    let provider = Arc::new(ClaudeCliProvider::new(cfg));

    let running = {
        let provider = provider.clone();
        tokio::spawn(async move { provider.complete(simple_request()).await })
    };
    assert!(
        appears_within(&f.log.join("ready"), Duration::from_secs(4)).await,
        "가짜가 뜨지 않았다"
    );
    let pid = f.pid("pid");
    let _reaper = Reaper(vec![pid]);

    let started = Instant::now();
    tokio::time::timeout(Duration::from_secs(6), provider.shutdown())
        .await
        .expect("끝내기가 돌아오지 않았다");
    let elapsed = started.elapsed();

    assert!(
        elapsed >= Duration::from_millis(480),
        "표준 입력을 닫고 0.5초 기다리지 않았다: {elapsed:?}"
    );
    assert!(f.has("got_term"), "0.5초 뒤 SIGTERM을 보내지 않았다");
    assert!(!alive(pid), "끝내기가 돌아왔는데 가짜가 살아 있다");
    let result = tokio::time::timeout(Duration::from_secs(2), running)
        .await
        .expect("끝낸 호출이 돌아오지 않았다")
        .unwrap();
    assert!(result.is_err(), "끝낸 호출이 성공했다");
}
