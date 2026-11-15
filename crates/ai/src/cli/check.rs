//! 설치 · 로그인 확인(ai.md 2차 11, `cli-backend.md` 5.7). 모델을 부르지 않는다.
//!
//! 확인이 제한을 넘기면 부르는 쪽에는 바로 [`CliStatus::TimedOut`]을 돌려주되 프로세스를 바로
//! 죽이지 않는다(토큰 갱신 잠금 — 원본 2.6). 따로 된 일이 표준 출력을 계속 읽어 버려(EPIPE로
//! 죽지 않게) 그 프로세스가 스스로 끝날 수 있게 하고 거둔다(ai.md 3차 10). 제한 뒤
//! [`CHECK_SETTLE_WAIT`]가 지나도 끝나지 않으면 프로세스 묶음째 `SIGTERM` → 유예 → `SIGKILL`로
//! 끝내고 거둔다(ai.md 4차 7, 결정 2026-11-15 "CLI 확인 프로세스가 끝나지 않을 때").
//! 앞 확인의 프로세스가 아직 살아 있으면 새로 띄우지 않는다(4차 2).

use std::io;
use std::path::Path;
use std::process::{Output, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use serde_json::Value;
use tokio::io::AsyncReadExt;
use tokio::process::{Child, ChildStdout, Command};
use tokio::sync::oneshot;

use super::command;
use super::groups::signal_group_async;
use super::{CLI_MIN_VERSION, ClaudeCliConfig, CliStatus, CliVersion};

/// 확인 명령이 제한을 넘긴 뒤에도 출력을 읽어 버리며 스스로 끝나기를 기다리는 시간.
/// 이것이 지나면 묶음째 끝낸다(ai.md 4차 7).
const CHECK_SETTLE_WAIT: Duration = Duration::from_secs(3);

/// 한 번에 읽는 크기.
const CHUNK: usize = 16 * 1024;

/// 구독 로그인의 `authMethod` 값.
const SUBSCRIPTION_METHOD: &str = "claude.ai";

/// 확인 명령 하나의 결과.
enum Probe {
    Done(Output),
    Missing,
    Failed,
    TimedOut,
    /// 앞 확인의 프로세스가 아직 살아 있어 띄우지 않았다.
    Busy,
}

/// 살아 있는 확인 프로세스가 있는가. 한 공급자가 하나를 쥔다.
#[derive(Default)]
pub(super) struct ProbeAlive(AtomicBool);

/// 확인 프로세스를 거두면(또는 그 일이 버려지면) 표시를 내린다.
struct AliveMark(Arc<ProbeAlive>);

impl Drop for AliveMark {
    fn drop(&mut self) {
        self.0.0.store(false, Ordering::SeqCst);
    }
}

/// 버전과 로그인을 차례로 확인한다.
pub(super) async fn check(config: &ClaudeCliConfig, alive: &Arc<ProbeAlive>) -> CliStatus {
    let Some(program) = &config.command else {
        return CliStatus::NotInstalled;
    };
    // 작업 폴더가 없으면 띄우지 않는다. 띄울 때의 `NotFound`는 실행 파일이 없다는 뜻만 남는다.
    if !config.work_dir.is_dir() {
        return CliStatus::Unreadable;
    }
    let version = match probe(
        config,
        alive,
        program,
        &["--version"],
        config.version_timeout,
    )
    .await
    {
        Probe::Done(out) if out.status.success() => match parse_version(&out.stdout) {
            Some(v) => v,
            None => return CliStatus::Unreadable,
        },
        Probe::Done(_) | Probe::Failed => return CliStatus::Unreadable,
        Probe::Missing => return CliStatus::NotInstalled,
        Probe::TimedOut | Probe::Busy => return CliStatus::TimedOut,
    };
    if version < CLI_MIN_VERSION {
        return CliStatus::TooOld { version };
    }
    let args = ["auth", "status", "--json"];
    match probe(config, alive, program, &args, config.auth_timeout).await {
        Probe::Done(out) => auth_status(&out, version),
        Probe::Failed => CliStatus::Unreadable,
        Probe::Missing => CliStatus::NotInstalled,
        Probe::TimedOut | Probe::Busy => CliStatus::TimedOut,
    }
}

/// 확인 명령을 실제 호출과 같은 환경 · 작업 폴더로, 자기 프로세스 묶음으로 띄운다.
///
/// 표준 출력은 따로 된 일([`keep`])이 끝까지 읽고(상한은 실제 호출과 같다) 자식을 거둔다.
/// 제한을 넘겨 기다리기를 그만두거나 부르는 쪽이 이 일을 버려도 그 일은 계속 돈다.
/// 앞 확인의 프로세스가 아직 살아 있으면 띄우지 않고 [`Probe::Busy`].
async fn probe(
    config: &ClaudeCliConfig,
    alive: &Arc<ProbeAlive>,
    program: &Path,
    args: &[&str],
    limit: Duration,
) -> Probe {
    if alive
        .0
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return Probe::Busy;
    }
    let mark = AliveMark(Arc::clone(alive));
    let mut cmd = Command::new(program);
    cmd.args(args)
        .env_clear()
        .envs(command::child_env(config, None))
        .current_dir(&config.work_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .process_group(0)
        .kill_on_drop(false);
    let mut child = match cmd.spawn() {
        Ok(child) => child,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Probe::Missing,
        Err(_) => return Probe::Failed,
    };
    let stdout = child.stdout.take();
    let (done, result) = oneshot::channel();
    let settle_at = tokio::time::Instant::now() + limit + CHECK_SETTLE_WAIT;
    tokio::spawn(keep(
        child,
        stdout,
        config.max_output_bytes,
        settle_at,
        config.kill_grace,
        (mark, done),
    ));
    match tokio::time::timeout(limit, result).await {
        Ok(Ok(Some(out))) => Probe::Done(out),
        Ok(_) => Probe::Failed,
        Err(_) => Probe::TimedOut,
    }
}

/// 확인 프로세스를 지키는 일. 출력을 읽어 버리며 끝나기를 기다려 거두고, `settle_at`까지
/// 끝나지 않으면 묶음째 `SIGTERM` → `grace` → `SIGKILL`로 끝내고 거둔다.
/// 거둔 뒤에 살아 있음 표시를 내리고 결과를 보낸다(다음 확인이 바로 띄울 수 있게).
async fn keep(
    mut child: Child,
    stdout: Option<ChildStdout>,
    cap: usize,
    settle_at: tokio::time::Instant,
    grace: Duration,
    (mark, done): (AliveMark, oneshot::Sender<Option<Output>>),
) {
    let pgid = child.id();
    let finished = async {
        let read = drain(stdout, cap).await;
        (child.wait().await, read)
    };
    let output = match tokio::time::timeout_at(settle_at, finished).await {
        Ok((Ok(status), Some(stdout))) => Some(Output {
            status,
            stdout,
            stderr: Vec::new(),
        }),
        Ok(_) => None,
        Err(_) => {
            if let Some(pgid) = pgid {
                signal_group_async("TERM", pgid).await;
                if tokio::time::timeout(grace, child.wait()).await.is_err() {
                    signal_group_async("KILL", pgid).await;
                }
            }
            let _ = child.wait().await;
            None
        }
    };
    drop(mark);
    let _ = done.send(output);
}

/// 표준 출력을 끝까지 읽는다. 상한까지만 모으고, 넘치면 나머지는 읽어 버리고 `None`.
/// 읽기 오류도 `None`.
async fn drain(pipe: Option<ChildStdout>, cap: usize) -> Option<Vec<u8>> {
    let mut buf = Vec::new();
    let Some(mut pipe) = pipe else {
        return Some(buf);
    };
    let mut overflow = false;
    let mut chunk = vec![0u8; CHUNK];
    loop {
        let n = pipe.read(&mut chunk).await.ok()?;
        if n == 0 {
            return (!overflow).then_some(buf);
        }
        if overflow || buf.len() + n > cap {
            overflow = true;
            buf.clear();
        } else {
            buf.extend_from_slice(&chunk[..n]);
        }
    }
}

/// 첫 낱말을 `주.부.수`로 읽는다.
fn parse_version(stdout: &[u8]) -> Option<CliVersion> {
    let text = std::str::from_utf8(stdout).ok()?;
    let word = text.split_whitespace().next()?;
    let mut parts = word.split('.').map(|p| p.parse::<u32>().ok());
    let (Some(Some(major)), Some(Some(minor)), Some(Some(patch)), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return None;
    };
    Some(CliVersion {
        major,
        minor,
        patch,
    })
}

fn auth_status(out: &Output, version: CliVersion) -> CliStatus {
    match out.status.code() {
        Some(1) => return CliStatus::NotLoggedIn,
        Some(0) => {}
        _ => return CliStatus::Unreadable,
    }
    let Ok(value) = serde_json::from_slice::<Value>(&out.stdout) else {
        return CliStatus::Unreadable;
    };
    match value.get("loggedIn").and_then(Value::as_bool) {
        Some(true) => {}
        Some(false) => return CliStatus::NotLoggedIn,
        None => return CliStatus::Unreadable,
    }
    let Some(auth_method) = value.get("authMethod").and_then(Value::as_str) else {
        return CliStatus::Unreadable;
    };
    if auth_method != SUBSCRIPTION_METHOD {
        return CliStatus::NotSubscription {
            auth_method: auth_method.to_string(),
        };
    }
    CliStatus::LoggedIn {
        version,
        auth_method: auth_method.to_string(),
        subscription_type: value
            .get("subscriptionType")
            .and_then(Value::as_str)
            .map(str::to_string),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 버전은_세_수만_읽는다() {
        assert_eq!(
            parse_version(b"2.1.316 (Claude Code)\n"),
            Some(CliVersion {
                major: 2,
                minor: 1,
                patch: 316,
            })
        );
        assert_eq!(parse_version(b"2.1"), None);
        assert_eq!(parse_version(b"2.1.3.4"), None);
        assert_eq!(parse_version(b"v2.1.3"), None);
        assert_eq!(parse_version(b""), None);
    }
}
