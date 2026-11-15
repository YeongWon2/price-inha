//! CLI 한 번 띄우기 · 읽기 · 끝내기(ai.md 2차 6 · 9 · 10).
//!
//! 자식은 자기 프로세스 그룹으로 띄운다. 정상으로 끝나지 않은 채 버려지면(시간 초과 ·
//! 호출 버림 · 출력 넘침 · 실패 결과 줄) [`GroupGuard`]가 그룹 전체에 `SIGTERM`, 유예 뒤
//! `SIGKILL`을 보낸다. 정상으로 거둔 뒤에도 그룹에 `SIGTERM`을 한 번 보내 남은 손주를 끝낸다.
//! 자리표를 받자마자 공급자의 명부([`Groups`])에 자리를 잡고 띄운 그룹을 적어 끝내기가 찾을 수 있게
//! 한다. 띄우는 사이 끝내기가 시작돼 적기가 거부되면 그 자리에서 끝내고 거둔 뒤 돌아간다(ai.md 4차 1).
//! `unsafe` 없이 시스템 `kill` 명령을 부른다.

use std::io;
use std::os::fd::{AsFd, OwnedFd};
use std::path::Path;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::{OwnedSemaphorePermit, Semaphore, oneshot};

use super::command::{self, SystemFile};
use super::groups::{Groups, Registration, signal_group, signal_group_async};
use super::output::{self, StreamState};
use super::{CLI_STDIN_CLOSE_WAIT, ClaudeCliConfig};
use crate::error::AiError;
use crate::types::{CompletionRequest, CompletionResponse};

/// `SIGKILL` 뒤 자식을 거두려고 기다리는 가장 긴 시간.
const REAP_WAIT: Duration = Duration::from_secs(1);

/// 한 번에 읽는 크기.
const CHUNK: usize = 16 * 1024;

/// CLI를 한 번 띄워 응답을 받는다. 전체 제한은 부르는 쪽이 건다.
pub(super) async fn run_once(
    config: &ClaudeCliConfig,
    slots: &Arc<Semaphore>,
    groups: &Arc<Groups>,
    program: &Path,
    request: &CompletionRequest,
    input: String,
) -> Result<CompletionResponse, AiError> {
    let permit = Arc::clone(slots)
        .acquire_owned()
        .await
        .map_err(|_| AiError::Connection)?;
    // 자리표를 받는 순간부터 명부에 센다. 끝내기가 이미 시작됐으면 띄우지 않는다(ai.md 4차 1).
    let reservation = groups.reserve().ok_or(AiError::Connection)?;
    let system_file = match &request.system {
        Some(text) => {
            Some(SystemFile::create(&config.work_dir, text).map_err(|_| AiError::Connection)?)
        }
        None => None,
    };
    let args = command::args(&request.model, system_file.as_ref().map(SystemFile::path));
    let mut cmd = Command::new(program);
    cmd.args(&args)
        .env_clear()
        .envs(command::child_env(config, Some(request.max_tokens)))
        .current_dir(&config.work_dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0);
    let mut child = cmd.spawn().map_err(|e| spawn_error(&e))?;
    let held = held_outputs(&child);
    let mut stdin = child.stdin.take();
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let (close_stdin, stdin_closed) = oneshot::channel();
    let attached = child
        .id()
        .is_some_and(|pgid| reservation.attach(pgid, close_stdin));
    let mut guard = GroupGuard::new(child, config.kill_grace, permit, held, Some(reservation));
    if !attached {
        // 띄우는 사이에 끝내기가 시작됐다. 끝내기와 같은 순서로 이 자리에서 끝내고 거둔 뒤 돌아간다.
        // 명부의 자리는 거둔 뒤에 놓으므로 끝내기도 그때까지 기다린다.
        drop(stdin.take());
        guard.terminate().await;
        return Err(AiError::Connection);
    }

    let collected = collect(
        (stdin, input.into_bytes(), stdin_closed),
        stdout,
        stderr,
        config.max_output_bytes,
    )
    .await?;
    if collected.failed_early {
        return Err(output::early_failure(&collected.state));
    }
    let status = guard.wait().await.map_err(|_| AiError::Connection)?;
    output::finish(&collected.state, status, &collected.stderr)
}

/// 표준 출력 · 표준 오류 읽는 끝의 사본. 그룹을 끝낼 때까지 쥐고 있어서,
/// 읽기를 그만둔 뒤에도 자식이 `SIGPIPE`로 먼저 죽지 않고 `SIGTERM`을 처리할 수 있게 한다.
fn held_outputs(child: &Child) -> Vec<OwnedFd> {
    let out = child
        .stdout
        .as_ref()
        .map(|p| p.as_fd().try_clone_to_owned());
    let err = child
        .stderr
        .as_ref()
        .map(|p| p.as_fd().try_clone_to_owned());
    [out, err]
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .collect()
}

fn spawn_error(e: &io::Error) -> AiError {
    if e.kind() == io::ErrorKind::NotFound {
        AiError::NotConfigured
    } else {
        AiError::Connection
    }
}

/// 표준 출력 · 표준 오류에서 모은 것.
struct Collected {
    state: StreamState,
    stderr: Vec<u8>,
    /// 실패 결과 줄을 보고 일찍 그만두었다.
    failed_early: bool,
}

/// 표준 입력에 쓸 것: 파이프, 글, 끝내기가 닫으라고 알리는 받는 쪽.
type StdinJob = (Option<ChildStdin>, Vec<u8>, oneshot::Receiver<()>);

/// 표준 입력을 쓰면서 두 출력을 읽는다. 어느 쪽이든 상한을 넘으면 `InvalidResponse`.
async fn collect(
    (stdin, input, stdin_closed): StdinJob,
    stdout: Option<impl AsyncRead + Unpin>,
    stderr: Option<impl AsyncRead + Unpin>,
    limit: usize,
) -> Result<Collected, AiError> {
    let write = write_stdin(stdin, input, stdin_closed);
    let read_out = read_stdout(stdout, limit);
    let read_err = read_capped(stderr, limit);
    tokio::pin!(write, read_out, read_err);
    let mut wrote = false;
    let mut err_buf: Option<Vec<u8>> = None;
    let (state, failed_early) = loop {
        tokio::select! {
            () = &mut write, if !wrote => wrote = true,
            r = &mut read_err, if err_buf.is_none() => err_buf = Some(r?),
            r = &mut read_out => break r?,
        }
    };
    if failed_early {
        return Ok(Collected {
            state,
            stderr: err_buf.unwrap_or_default(),
            failed_early,
        });
    }
    let stderr = match err_buf {
        Some(buf) => buf,
        None => read_err.await?,
    };
    Ok(Collected {
        state,
        stderr,
        failed_early,
    })
}

/// 표준 입력에 한 번 쓰고 닫는다. 자식이 먼저 끝나 못 쓰면 그만둔다.
/// 끝내기가 닫으라고 알리면 쓰기를 그만두고 바로 닫는다(원본 2.7).
async fn write_stdin(stdin: Option<ChildStdin>, input: Vec<u8>, closed: oneshot::Receiver<()>) {
    let Some(mut pipe) = stdin else {
        return;
    };
    tokio::select! {
        written = pipe.write_all(&input) => {
            if written.is_ok() {
                let _ = pipe.shutdown().await;
            }
        }
        _ = closed => {}
    }
}

/// 표준 출력을 줄마다 읽는다. 실패 결과 줄을 보면 바로 돌려준다(참과 함께).
async fn read_stdout(
    pipe: Option<impl AsyncRead + Unpin>,
    limit: usize,
) -> Result<(StreamState, bool), AiError> {
    let mut state = StreamState::default();
    let Some(mut pipe) = pipe else {
        return Ok((state, false));
    };
    let mut pending: Vec<u8> = Vec::new();
    let mut total = 0usize;
    let mut chunk = vec![0u8; CHUNK];
    loop {
        let n = pipe
            .read(&mut chunk)
            .await
            .map_err(|_| AiError::Connection)?;
        if n == 0 {
            let failed = !pending.is_empty() && state.feed(&pending);
            return Ok((state, failed));
        }
        total += n;
        if total > limit {
            return Err(AiError::InvalidResponse);
        }
        pending.extend_from_slice(&chunk[..n]);
        while let Some(pos) = pending.iter().position(|b| *b == b'\n') {
            let line: Vec<u8> = pending.drain(..=pos).collect();
            if state.feed(&line[..pos]) {
                return Ok((state, true));
            }
        }
    }
}

/// 끝까지 읽는다. 상한을 넘으면 `InvalidResponse`.
async fn read_capped(
    pipe: Option<impl AsyncRead + Unpin>,
    limit: usize,
) -> Result<Vec<u8>, AiError> {
    let mut buf = Vec::new();
    let Some(mut pipe) = pipe else {
        return Ok(buf);
    };
    let mut chunk = vec![0u8; CHUNK];
    loop {
        let n = pipe
            .read(&mut chunk)
            .await
            .map_err(|_| AiError::Connection)?;
        if n == 0 {
            return Ok(buf);
        }
        if buf.len() + n > limit {
            return Err(AiError::InvalidResponse);
        }
        buf.extend_from_slice(&chunk[..n]);
    }
}

/// 띄운 자식과 그 그룹. 정상으로 거두지 못한 채 버려지면 그룹을 끝낸다.
///
/// 동시 실행 자리표도 쥐고 있다가 그룹을 끝낸 뒤에 놓는다.
struct GroupGuard {
    child: Option<Child>,
    pgid: Option<u32>,
    grace: Duration,
    permit: Option<OwnedSemaphorePermit>,
    /// 출력 읽는 끝의 사본(그룹을 끝낸 뒤에 닫는다).
    held: Vec<OwnedFd>,
    /// 공급자 명부의 자리(그룹을 끝낸 뒤에 놓는다).
    registration: Option<Registration>,
    /// 자식을 거뒀다(그룹을 끝낼 필요가 없다).
    reaped: bool,
    /// 거두기 전에 그룹 전체에 신호를 보냈다(거둔 뒤에 다시 보내지 않는다 — ai.md 5차 6).
    signalled: bool,
}

impl GroupGuard {
    fn new(
        child: Child,
        grace: Duration,
        permit: OwnedSemaphorePermit,
        held: Vec<OwnedFd>,
        registration: Option<Registration>,
    ) -> Self {
        Self {
            pgid: child.id(),
            child: Some(child),
            grace,
            permit: Some(permit),
            held,
            registration,
            reaped: false,
            signalled: false,
        }
    }

    /// 표준 입력이 닫힌 자식을 끝내기와 같은 순서로 끝내고 거둔다: [`CLI_STDIN_CLOSE_WAIT`] 동안
    /// 스스로 끝나기를 기다리고, 그래도 살아 있으면 그룹에 `SIGTERM` → 유예 → `SIGKILL`.
    /// 신호 명령을 띄우지 못하면 자식 하나에 `SIGKILL`로 물러서고, 마지막 기다림에는 상한을 둔다
    /// (못 거두면 버릴 때 다시 끝낸다).
    async fn terminate(&mut self) {
        let (Some(child), Some(pgid)) = (self.child.as_mut(), self.pgid) else {
            return;
        };
        if tokio::time::timeout(CLI_STDIN_CLOSE_WAIT, child.wait())
            .await
            .is_err()
        {
            self.signalled |= signal_group_async("TERM", pgid).await;
            if tokio::time::timeout(self.grace, child.wait())
                .await
                .is_err()
            {
                if signal_group_async("KILL", pgid).await {
                    self.signalled = true;
                } else {
                    let _ = child.start_kill();
                }
            }
        }
        if let Ok(Ok(_)) = tokio::time::timeout(REAP_WAIT, child.wait()).await {
            self.set_reaped();
        }
    }

    /// 거뒀다고 적는다. 명부도 그 그룹 번호를 잊는다(ai.md 5차 6).
    fn set_reaped(&mut self) {
        self.reaped = true;
        if let Some(registration) = &self.registration {
            registration.mark_reaped();
        }
    }

    /// 자식이 끝나기를 기다려 거둔다.
    async fn wait(&mut self) -> io::Result<std::process::ExitStatus> {
        let Some(child) = self.child.as_mut() else {
            return Err(io::Error::other("자식이 없다"));
        };
        let status = child.wait().await?;
        self.set_reaped();
        Ok(status)
    }
}

impl Drop for GroupGuard {
    /// 거두지 못했으면 그룹에 `SIGTERM` → 유예 → `SIGKILL`을 보내고 거둔다.
    /// 거뒀으면 남은 손주가 있을 수 있으니 그룹에 `SIGTERM`을 한 번 보내되, 거두기 전에 이미 그룹에
    /// 신호를 보냈으면 다시 보내지 않는다(ai.md 5차 6). 신호 명령을 띄우지 못하면 자식 하나에
    /// `SIGKILL`로 물러선다. 자리표 · 명부 자리는 그 뒤에 놓는다. 막히지 않게 따로 된 실에서 한다.
    fn drop(&mut self) {
        let child = self.child.take();
        let permit = self.permit.take();
        let registration = self.registration.take();
        let held = std::mem::take(&mut self.held);
        let Some(pgid) = self.pgid else {
            return;
        };
        let (reaped, signalled, grace) = (self.reaped, self.signalled, self.grace);
        if reaped && signalled {
            return;
        }
        let spawned = std::thread::Builder::new()
            .name("pi-ai-cli-kill".to_string())
            .spawn(move || {
                if reaped {
                    signal_group("TERM", pgid);
                } else {
                    end_unreaped(child, pgid, grace);
                }
                drop(held);
                drop(permit);
                drop(registration);
            });
        if spawned.is_err() {
            signal_group(if reaped { "TERM" } else { "KILL" }, pgid);
        }
    }
}

/// 거두지 못한 자식의 그룹에 `SIGTERM` → 유예 → `SIGKILL`을 보내고 잠깐 기다려 거둔다.
/// 신호 명령을 띄우지 못하면 자식 하나에 `SIGKILL`로 물러선다.
fn end_unreaped(mut child: Option<Child>, pgid: u32, grace: Duration) {
    if !signal_group("TERM", pgid)
        && let Some(child) = child.as_mut()
    {
        let _ = child.start_kill();
    }
    std::thread::sleep(grace);
    if !signal_group("KILL", pgid)
        && let Some(child) = child.as_mut()
    {
        let _ = child.start_kill();
    }
    if let Some(child) = child {
        reap(child);
    }
}

/// `SIGKILL`을 보낸 자식을 잠깐 기다려 거둔다. 그 안에 못 거두면 런타임에 맡긴다.
fn reap(mut child: Child) {
    let step = Duration::from_millis(10);
    let mut waited = Duration::ZERO;
    while waited < REAP_WAIT {
        if !matches!(child.try_wait(), Ok(None)) {
            return;
        }
        std::thread::sleep(step);
        waited += step;
    }
}
