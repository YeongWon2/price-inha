//! pi-ai 6차: CLI 공급자. docs/dev/modules/ai.md "6차 작업" 1번.
//!
//! 실제 `claude`를 부르지 않는다. 테스트가 임시 폴더에 가짜 실행 파일(셸 스크립트)을 만들어
//! 경로로 준다. 사용자 `~/.claude`를 열지 않는다(자식 환경은 `PATH`만 준다).
//! 2번(호출 쪽 신호 명령을 못 띄울 때 물러서기)은 공개 API로 일으킬 수 없어 `src` 안 단위 테스트 몫이다.
//! macOS에서 새로 쓴 셸 스크립트의 첫 실행이 수백 ms 걸리므로 시간 예산은 넉넉히 잡는다.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use pi_ai::{
    AiProvider, ClaudeCliConfig, ClaudeCliProvider, CliStatus, CliVersion, CompletionRequest,
    Message, Role,
};
use serde_json::json;
use tempfile::TempDir;

const TEST_MODEL: &str = "siheom-model-h";
const VERSION_OK: &str = "printf '2.1.316 (Claude Code)\\n'\nexit 0";

// ---------- 가짜 CLI ----------

/// 가짜가 할 일. 갈래마다 셸 줄을 받는다(스스로 `exit` 한다).
struct Behave {
    call: String,
    version: String,
    auth: String,
}

impl Behave {
    fn ok() -> Self {
        Self {
            call: format!("printf '%s\\n' '{}'\nexit 0", result_line()),
            version: VERSION_OK.to_string(),
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

    fn pid(&self, name: &str) -> u32 {
        fs::read_to_string(self.log.join(name))
            .unwrap()
            .trim()
            .parse()
            .unwrap()
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

fn request() -> CompletionRequest {
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

/// 살아 있는가(거둬졌거나 좀비면 죽은 것으로 본다).
fn alive(pid: u32) -> bool {
    let out = Command::new("/bin/ps")
        .args(["-o", "stat=", "-p", &pid.to_string()])
        .output()
        .unwrap();
    let stat = String::from_utf8_lossy(&out.stdout).trim().to_string();
    out.status.success() && !stat.is_empty() && !stat.starts_with('Z')
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

/// 시험이 끝날 때(실패해도) 표시 폴더의 `_grandchild` 파일에 적힌 프로세스를 `SIGKILL`로 치운다.
struct Cleanup(PathBuf);

impl Drop for Cleanup {
    fn drop(&mut self) {
        let Ok(entries) = fs::read_dir(&self.0) else {
            return;
        };
        for entry in entries.flatten() {
            if !entry.file_name().to_string_lossy().ends_with("_grandchild") {
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

/// `SIGTERM`을 무시하고(무시는 손주에게 물려진다) 출력 파이프를 닫은 손주를 묶음 안에 남긴 뒤,
/// `then`으로 정상으로 끝나는 셸 줄. 손주는 `SIGKILL`로만 죽는다.
fn leave_stubborn_grandchild(mark: &str, then: &str) -> String {
    format!(
        "trap '' TERM\n/bin/sleep 30 </dev/null >/dev/null 2>&1 &\necho $! > \"$L/{mark}_grandchild\"\n{then}"
    )
}

// ---------- 1. 정상으로 끝나도 SIGTERM을 무시하는 손주를 끝까지 끝낸다 ----------

// 검증: D4
#[tokio::test]
async fn 버전_확인이_정상으로_끝나도_sigterm을_무시하는_손주를_유예_뒤_끝낸다() {
    let mut b = Behave::ok();
    b.version = leave_stubborn_grandchild("version", VERSION_OK);
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
        "정상으로 끝난 버전 확인의 묶음에 SIGTERM을 무시하는 손주가 남았다(유예 뒤 SIGKILL이 없다)"
    );
}

// 검증: D4
#[tokio::test]
async fn 로그인_확인이_정상으로_끝나도_sigterm을_무시하는_손주를_유예_뒤_끝낸다() {
    let mut b = Behave::ok();
    b.auth = leave_stubborn_grandchild(
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
        "정상으로 끝난 로그인 확인의 묶음에 SIGTERM을 무시하는 손주가 남았다(유예 뒤 SIGKILL이 없다)"
    );
}

// 검증: D4
#[tokio::test]
async fn 호출이_정상으로_끝나도_sigterm을_무시하는_손주를_유예_뒤_끝낸다() {
    let mut b = Behave::ok();
    b.call = leave_stubborn_grandchild(
        "call",
        &format!("printf '%s\\n' '{}'\nexit 0", result_line()),
    );
    let f = Fake::new(&b);
    let _cleanup = f.cleanup();
    let provider = ClaudeCliProvider::new(config(&f));

    let resp = tokio::time::timeout(Duration::from_secs(10), provider.complete(request()))
        .await
        .expect("호출이 돌아오지 않았다")
        .expect("정상 호출이 실패했다");

    assert_eq!(resp.text, "가짜 답");
    let grandchild = f.pid("call_grandchild");
    assert!(
        dies_within(grandchild, Duration::from_secs(3)).await,
        "정상으로 끝난 호출의 묶음에 SIGTERM을 무시하는 손주가 남았다(유예 뒤 SIGKILL이 없다)"
    );
}
