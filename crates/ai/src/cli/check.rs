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
use tokio::sync::{Notify, oneshot, watch};
use tokio::time::Instant;

use super::command;
use super::groups::{KILL_COMMAND, kill_lingering_with, signal_group_with};
use super::{CLI_MIN_VERSION, ClaudeCliConfig, CliCheck, CliStatus, CliVersion};

/// 확인 명령이 제한을 넘긴 뒤에도 출력을 읽어 버리며 스스로 끝나기를 기다리는 시간.
/// 이것이 지나면 묶음째 끝낸다(ai.md 4차 7).
const CHECK_SETTLE_WAIT: Duration = Duration::from_secs(3);

/// 끝내기의 마지막 `SIGKILL`(또는 물러선 `start_kill`) 뒤 자식을 거두려고 기다리는 가장 긴 시간
/// (ai.md 5차 2). 그 안에 못 거두면 런타임에 맡기고 살아 있음 표시를 내린다.
const REAP_WAIT: Duration = Duration::from_secs(1);

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

/// 확인 프로세스의 살아 있음 · 끝내기 표시. 한 공급자가 하나를 쥔다.
pub(super) struct ProbeAlive {
    /// 살아 있는 확인 프로세스가 있는가(앞 확인을 버렸어도).
    alive: AtomicBool,
    /// 끝내기가 시작됐다. 참이 되면 지키는 일이 묶음째 끝내고, 새 확인은 띄우지 않는다(ai.md 5차 3).
    stop: watch::Sender<bool>,
    /// 살아 있음 표시가 내려갈 때 깨운다.
    gone: Notify,
}

impl Default for ProbeAlive {
    fn default() -> Self {
        Self {
            alive: AtomicBool::new(false),
            stop: watch::channel(false).0,
            gone: Notify::new(),
        }
    }
}

impl ProbeAlive {
    /// 새 확인을 받지 않게 하고, 살아 있는 확인 프로세스를 묶음째 끝내 거둘 때까지 기다린다.
    pub(super) async fn shutdown(&self) {
        self.stop.send_replace(true);
        loop {
            let gone = self.gone.notified();
            if !self.alive.load(Ordering::SeqCst) {
                return;
            }
            gone.await;
        }
    }
}

/// 확인 프로세스를 거두면(또는 그 일이 버려지면) 표시를 내린다.
struct AliveMark(Arc<ProbeAlive>);

impl Drop for AliveMark {
    fn drop(&mut self) {
        self.0.alive.store(false, Ordering::SeqCst);
        self.0.gone.notify_waiters();
    }
}

/// 버전과 로그인을 차례로 확인한다. 앞 확인이 아직 돌면 [`CliCheck::PreviousProbeAlive`].
pub(super) async fn check(config: &ClaudeCliConfig, alive: &Arc<ProbeAlive>) -> CliCheck {
    let Some(program) = &config.command else {
        return CliCheck::Status(CliStatus::NotInstalled);
    };
    // 작업 폴더가 없으면 띄우지 않는다. 띄울 때의 `NotFound`는 실행 파일이 없다는 뜻만 남는다.
    if !config.work_dir.is_dir() {
        return CliCheck::Status(CliStatus::Unreadable);
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
            None => return CliCheck::Status(CliStatus::Unreadable),
        },
        Probe::Busy => return CliCheck::PreviousProbeAlive,
        other => return CliCheck::Status(not_done(other)),
    };
    if version < CLI_MIN_VERSION {
        return CliCheck::Status(CliStatus::TooOld { version });
    }
    let args = ["auth", "status", "--json"];
    match probe(config, alive, program, &args, config.auth_timeout).await {
        Probe::Done(out) => CliCheck::Status(auth_status(&out, version)),
        Probe::Busy => CliCheck::PreviousProbeAlive,
        other => CliCheck::Status(not_done(other)),
    }
}

/// 결과를 읽을 수 없는 확인 명령의 상태.
fn not_done(probe: Probe) -> CliStatus {
    match probe {
        Probe::Missing => CliStatus::NotInstalled,
        Probe::TimedOut | Probe::Busy => CliStatus::TimedOut,
        Probe::Done(_) | Probe::Failed => CliStatus::Unreadable,
    }
}

/// 확인 명령을 실제 호출과 같은 환경 · 작업 폴더로, 자기 프로세스 묶음으로 띄운다.
///
/// 표준 출력은 따로 된 일([`keep`])이 끝까지 읽고(상한은 실제 호출과 같다) 자식을 거둔다.
/// 제한을 넘겨 기다리기를 그만두거나 부르는 쪽이 이 일을 버려도 그 일은 계속 돈다.
/// 앞 확인의 프로세스가 아직 살아 있으면 띄우지 않고 [`Probe::Busy`].
/// 끝내기가 시작됐으면 띄우지 않고 [`Probe::Failed`].
async fn probe(
    config: &ClaudeCliConfig,
    alive: &Arc<ProbeAlive>,
    program: &Path,
    args: &[&str],
    limit: Duration,
) -> Probe {
    if alive
        .alive
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return Probe::Busy;
    }
    let mark = AliveMark(Arc::clone(alive));
    let stop = alive.stop.subscribe();
    if *stop.borrow() {
        return Probe::Failed;
    }
    // 끝 시각은 띄우기 전에 센다. 넘치면 제한 없음(ai.md 5차 1).
    let deadline = Instant::now().checked_add(limit);
    let settle_at = deadline.and_then(|d| d.checked_add(CHECK_SETTLE_WAIT));
    let mut cmd = Command::new(program);
    cmd.args(args)
        .env_clear()
        .envs(command::child_env(config, None))
        .current_dir(&config.work_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(false);
    super::os::own_group(&mut cmd);
    let mut child = match super::spawn::spawn_retrying(|| cmd.spawn()).await {
        Ok(child) => child,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Probe::Missing,
        Err(_) => return Probe::Failed,
    };
    let stdout = child.stdout.take();
    let (done, result) = oneshot::channel();
    let watch = Watch {
        cap: config.max_output_bytes,
        settle_at,
        stop,
        ender: Ender {
            grace: config.kill_grace,
            kill: Path::new(KILL_COMMAND),
        },
    };
    tokio::spawn(keep(child, stdout, watch, (mark, done)));
    let waited = match deadline {
        Some(deadline) => tokio::time::timeout_at(deadline, result).await,
        None => Ok(result.await),
    };
    match waited {
        Ok(Ok(Some(out))) => Probe::Done(out),
        Ok(_) => Probe::Failed,
        Err(_) => Probe::TimedOut,
    }
}

/// 지키는 일이 볼 것.
struct Watch {
    /// 표준 출력 상한.
    cap: usize,
    /// 이때까지 끝나지 않으면 묶음째 끝낸다. `None`이면 제한 없음.
    settle_at: Option<Instant>,
    /// 끝내기 표시.
    stop: watch::Receiver<bool>,
    ender: Ender,
}

/// 확인 프로세스를 끝내는 법.
struct Ender {
    /// `SIGTERM` 뒤 `SIGKILL`까지의 유예.
    grace: Duration,
    /// 신호 명령.
    kill: &'static Path,
}

impl Ender {
    /// 그룹에 `SIGTERM` → 유예 → `SIGKILL`을 보내고 거둔다. 자식이 유예 안에 끝나도 그룹이 유예
    /// 뒤까지 남아 있으면 `SIGKILL`(ai.md 6차 1). 신호 명령을 띄우지 못하면
    /// `start_kill`로 물러서고, 마지막 기다림에는 [`REAP_WAIT`] 상한을 둔다(ai.md 5차 2).
    async fn end(&self, child: &mut Child, pgid: Option<u32>) {
        let termed = self.signal("TERM", pgid).await;
        if !termed {
            let _ = child.start_kill();
        }
        let until = Instant::now().checked_add(self.grace);
        if tokio::time::timeout(self.grace, child.wait()).await.is_ok() {
            if termed {
                self.linger(pgid, until).await;
            }
            return;
        }
        if !self.signal("KILL", pgid).await {
            let _ = child.start_kill();
        }
        let _ = tokio::time::timeout(REAP_WAIT, child.wait()).await;
    }

    /// 거둔 뒤 `SIGTERM`을 받은 그룹이 `until`까지 비지 않으면 `SIGKILL`(ai.md 6차 1).
    /// 끝 시각이 넘치면 지금 본다.
    async fn linger(&self, pgid: Option<u32>, until: Option<Instant>) {
        if let Some(pgid) = pgid {
            kill_lingering_with(self.kill, pgid, until.unwrap_or_else(Instant::now)).await;
        }
    }

    async fn signal(&self, signal: &str, pgid: Option<u32>) -> bool {
        match pgid {
            Some(pgid) => signal_group_with(self.kill, signal, pgid).await,
            None => false,
        }
    }
}

/// 확인 프로세스를 지키는 일. 출력을 읽어 버리며 끝나기를 기다려 거두고, 정상으로 거두면 묶음에
/// 남은 손주에 `SIGTERM`을 보내고 유예 뒤에도 남으면 `SIGKILL`(ai.md 5차 4 · 6차 1). `settle_at`까지 끝나지 않거나 끝내기가
/// 시작되면 묶음째 `SIGTERM` → 유예 → `SIGKILL`로 끝내고 거둔다(4차 7 · 5차 3).
/// 거둔 그룹 번호에는 신호를 다시 보내지 않는다(5차 6).
/// 거둔 뒤에 살아 있음 표시를 내리고 결과를 보낸다(다음 확인이 바로 띄울 수 있게).
async fn keep(
    mut child: Child,
    stdout: Option<ChildStdout>,
    watch: Watch,
    (mark, done): (AliveMark, oneshot::Sender<Option<Output>>),
) {
    let Watch {
        cap,
        settle_at,
        mut stop,
        ender,
    } = watch;
    let pgid = child.id();
    let finished = {
        let finished = async {
            let read = drain(stdout, cap).await;
            (child.wait().await, read)
        };
        let settled = async {
            match settle_at {
                Some(at) => tokio::time::sleep_until(at).await,
                None => std::future::pending().await,
            }
        };
        let stopped = async {
            if stop.wait_for(|s| *s).await.is_err() {
                std::future::pending::<()>().await;
            }
        };
        tokio::select! {
            r = finished => Some(r),
            () = settled => None,
            () = stopped => None,
        }
    };
    let output = match finished {
        Some((Ok(status), read)) => {
            // 거뒀다. 묶음에 남은 손주에 `SIGTERM`, 유예 뒤에도 남으면 `SIGKILL`(6차 1).
            let until = Instant::now().checked_add(ender.grace);
            if ender.signal("TERM", pgid).await {
                ender.linger(pgid, until).await;
            }
            read.map(|stdout| Output {
                status,
                stdout,
                stderr: Vec::new(),
            })
        }
        Some((Err(_), _)) | None => {
            ender.end(&mut child, pgid).await;
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

    fn sleeper() -> Child {
        Command::new("/bin/sleep")
            .arg("30")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .process_group(0)
            .kill_on_drop(true)
            .spawn()
            .unwrap()
    }

    fn gone(pid: u32) -> bool {
        let out = std::process::Command::new("/bin/ps")
            .args(["-o", "stat=", "-p", &pid.to_string()])
            .output()
            .unwrap();
        let stat = String::from_utf8_lossy(&out.stdout).trim().to_string();
        !out.status.success() || stat.is_empty() || stat.starts_with('Z')
    }

    #[tokio::test]
    async fn 신호_명령을_띄우지_못해도_자식을_끝내고_살아_있음을_내린다() {
        let alive = Arc::new(ProbeAlive::default());
        alive.alive.store(true, Ordering::SeqCst);
        let mark = AliveMark(Arc::clone(&alive));
        let child = sleeper();
        let pid = child.id().unwrap();
        let (done, result) = oneshot::channel();
        let watch = Watch {
            cap: 1024,
            settle_at: Some(Instant::now()),
            stop: alive.stop.subscribe(),
            ender: Ender {
                grace: Duration::from_millis(50),
                kill: Path::new("/nonexistent/pi-ai-no-kill"),
            },
        };
        let kept = tokio::time::timeout(
            Duration::from_secs(5),
            keep(child, None, watch, (mark, done)),
        )
        .await;
        assert!(kept.is_ok(), "지키는 일이 끝나지 않았다");
        assert!(
            !alive.alive.load(Ordering::SeqCst),
            "살아 있음 표시가 남았다"
        );
        assert_eq!(result.await.ok(), Some(None));
        assert!(gone(pid), "물러선 start_kill이 자식을 끝내지 못했다");
    }

    #[tokio::test]
    async fn 끝내기가_시작되면_제한_없는_확인도_끝내고_거둔다() {
        let alive = Arc::new(ProbeAlive::default());
        alive.alive.store(true, Ordering::SeqCst);
        let mark = AliveMark(Arc::clone(&alive));
        let child = sleeper();
        let pid = child.id().unwrap();
        let (done, _result) = oneshot::channel();
        let watch = Watch {
            cap: 1024,
            settle_at: None,
            stop: alive.stop.subscribe(),
            ender: Ender {
                grace: Duration::from_millis(50),
                kill: Path::new(KILL_COMMAND),
            },
        };
        tokio::spawn(keep(child, None, watch, (mark, done)));
        tokio::time::timeout(Duration::from_secs(5), alive.shutdown())
            .await
            .expect("끝내기가 돌아오지 않았다");
        assert!(gone(pid), "끝내기가 돌아왔는데 확인 프로세스가 살아 있다");
    }
}
