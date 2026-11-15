//! CLI 한 번 띄우기 · 읽기 · 끝내기(ai.md 2차 6 · 9 · 10).
//!
//! 자식은 자기 프로세스 그룹으로 띄운다. 정상으로 끝나지 않은 채 버려지면(시간 초과 ·
//! 호출 버림 · 출력 넘침 · 실패 결과 줄) [`GroupGuard`]가 그룹 전체에 `SIGTERM`, 유예 뒤
//! `SIGKILL`을 보낸다. `unsafe` 없이 시스템 `kill` 명령을 부른다.

use std::io;
use std::os::fd::{AsFd, OwnedFd};
use std::path::Path;
use std::process::{Command as StdCommand, Stdio};
use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

use super::ClaudeCliConfig;
use super::command::{self, SystemFile};
use super::output::{self, StreamState};
use crate::error::AiError;
use crate::types::{CompletionRequest, CompletionResponse};

/// 그룹에 신호를 보내는 시스템 명령.
const KILL_COMMAND: &str = "/bin/kill";

/// 한 번에 읽는 크기.
const CHUNK: usize = 16 * 1024;

/// CLI를 한 번 띄워 응답을 받는다. 전체 제한은 부르는 쪽이 건다.
pub(super) async fn run_once(
    config: &ClaudeCliConfig,
    slots: &Arc<Semaphore>,
    program: &Path,
    request: &CompletionRequest,
    input: String,
) -> Result<CompletionResponse, AiError> {
    let permit = Arc::clone(slots)
        .acquire_owned()
        .await
        .map_err(|_| AiError::Connection)?;
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
    let stdin = child.stdin.take();
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let mut guard = GroupGuard::new(child, config.kill_grace, permit, held);

    let collected = collect(
        stdin,
        input.into_bytes(),
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

/// 표준 입력을 쓰면서 두 출력을 읽는다. 어느 쪽이든 상한을 넘으면 `InvalidResponse`.
async fn collect(
    stdin: Option<ChildStdin>,
    input: Vec<u8>,
    stdout: Option<impl AsyncRead + Unpin>,
    stderr: Option<impl AsyncRead + Unpin>,
    limit: usize,
) -> Result<Collected, AiError> {
    let write = write_stdin(stdin, input);
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
async fn write_stdin(stdin: Option<ChildStdin>, input: Vec<u8>) {
    if let Some(mut pipe) = stdin
        && pipe.write_all(&input).await.is_ok()
    {
        let _ = pipe.shutdown().await;
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
    /// 자식을 거뒀다(그룹을 끝낼 필요가 없다).
    reaped: bool,
}

impl GroupGuard {
    fn new(
        child: Child,
        grace: Duration,
        permit: OwnedSemaphorePermit,
        held: Vec<OwnedFd>,
    ) -> Self {
        Self {
            pgid: child.id(),
            child: Some(child),
            grace,
            permit: Some(permit),
            held,
            reaped: false,
        }
    }

    /// 자식이 끝나기를 기다려 거둔다.
    async fn wait(&mut self) -> io::Result<std::process::ExitStatus> {
        let Some(child) = self.child.as_mut() else {
            return Err(io::Error::other("자식이 없다"));
        };
        let status = child.wait().await?;
        self.reaped = true;
        Ok(status)
    }
}

impl Drop for GroupGuard {
    fn drop(&mut self) {
        let child = self.child.take();
        let permit = self.permit.take();
        let held = std::mem::take(&mut self.held);
        if self.reaped {
            return;
        }
        let Some(pgid) = self.pgid else {
            return;
        };
        let grace = self.grace;
        let spawned = std::thread::Builder::new()
            .name("pi-ai-cli-kill".to_string())
            .spawn(move || {
                signal_group("TERM", pgid);
                std::thread::sleep(grace);
                signal_group("KILL", pgid);
                // 그룹을 끝낸 뒤에 거두고 자리표를 놓는다.
                if let Some(mut child) = child {
                    let _ = child.try_wait();
                }
                drop(held);
                drop(permit);
            });
        if spawned.is_err() {
            signal_group("KILL", pgid);
        }
    }
}

/// 그룹 전체에 신호를 보낸다(`kill -<신호> -- -<그룹>`).
fn signal_group(signal: &str, pgid: u32) {
    let _ = StdCommand::new(KILL_COMMAND)
        .arg(format!("-{signal}"))
        .arg("--")
        .arg(format!("-{pgid}"))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}
