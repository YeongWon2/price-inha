//! 한 공급자가 띄운, 아직 살아 있는 자식 그룹의 명부(ai.md 3차 12 · 13).
//!
//! [`ClaudeCliProvider::shutdown`](super::ClaudeCliProvider::shutdown)이 이 명부를 보고
//! 표준 입력을 닫은 뒤 그룹마다 `SIGTERM` → 유예 → `SIGKILL`을 보내고, 명부가 빌 때까지 기다린다.

use std::collections::HashMap;
use std::process::{Command as StdCommand, Stdio};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use tokio::sync::{Notify, oneshot};

/// 그룹에 신호를 보내는 시스템 명령.
const KILL_COMMAND: &str = "/bin/kill";

/// 살아 있는 자식 그룹의 명부.
#[derive(Default)]
pub(super) struct Groups {
    inner: Mutex<Inner>,
    /// 명부에서 하나가 빠질 때 깨운다.
    changed: Notify,
}

#[derive(Default)]
struct Inner {
    /// 끝내기가 시작됐다. 새 그룹을 받지 않는다.
    closed: bool,
    next_id: u64,
    live: HashMap<u64, Live>,
}

struct Live {
    pgid: u32,
    /// 보내면(또는 버리면) 그 호출이 표준 입력 쓰기를 그만두고 닫는다.
    close_stdin: Option<oneshot::Sender<()>>,
}

/// 명부의 한 자리. 버려지면 명부에서 빠진다.
pub(super) struct Registration {
    groups: Arc<Groups>,
    id: u64,
}

impl Drop for Registration {
    fn drop(&mut self) {
        self.groups.lock().live.remove(&self.id);
        self.groups.changed.notify_waiters();
    }
}

impl Groups {
    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// 끝내기가 시작됐는가.
    pub(super) fn is_closed(&self) -> bool {
        self.lock().closed
    }

    /// 띄운 그룹을 적는다. 끝내기가 이미 시작됐으면 적지 않고 `None`.
    pub(super) fn register(
        self: &Arc<Self>,
        pgid: u32,
        close_stdin: oneshot::Sender<()>,
    ) -> Option<Registration> {
        let mut inner = self.lock();
        if inner.closed {
            return None;
        }
        let id = inner.next_id;
        inner.next_id += 1;
        inner.live.insert(
            id,
            Live {
                pgid,
                close_stdin: Some(close_stdin),
            },
        );
        Some(Registration {
            groups: Arc::clone(self),
            id,
        })
    }

    /// 새 그룹을 받지 않게 하고, 살아 있는 그룹을 모두 끝낸 뒤 명부가 빌 때까지 기다린다.
    ///
    /// 표준 입력을 먼저 닫고 `SIGTERM`, 유예 안에 다 끝나지 않으면 남은 그룹에 `SIGKILL`.
    pub(super) async fn shutdown(&self, grace: Duration) {
        let targets: Vec<(u32, Option<oneshot::Sender<()>>)> = {
            let mut inner = self.lock();
            inner.closed = true;
            inner
                .live
                .values_mut()
                .map(|live| (live.pgid, live.close_stdin.take()))
                .collect()
        };
        if targets.is_empty() {
            return;
        }
        let mut pgids = Vec::with_capacity(targets.len());
        for (pgid, close_stdin) in targets {
            if let Some(tx) = close_stdin {
                let _ = tx.send(());
            }
            pgids.push(pgid);
        }
        // 표준 입력을 닫는 쪽이 먼저 돌 수 있게 한 번 양보한다.
        tokio::task::yield_now().await;
        for &pgid in &pgids {
            signal_group_async("TERM", pgid).await;
        }
        if tokio::time::timeout(grace, self.wait_empty()).await.is_ok() {
            return;
        }
        let remaining: Vec<u32> = self.lock().live.values().map(|l| l.pgid).collect();
        for pgid in remaining {
            signal_group_async("KILL", pgid).await;
        }
        self.wait_empty().await;
    }

    /// 명부가 빌 때까지 기다린다.
    async fn wait_empty(&self) {
        loop {
            let notified = self.changed.notified();
            if self.lock().live.is_empty() {
                return;
            }
            notified.await;
        }
    }
}

/// 그룹 전체에 신호를 보낸다(`kill -<신호> -- -<그룹>`). 막히는 함수 안에서 쓴다.
pub(super) fn signal_group(signal: &str, pgid: u32) {
    let _ = StdCommand::new(KILL_COMMAND)
        .args(kill_args(signal, pgid))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

/// [`signal_group`]의 비동기판.
async fn signal_group_async(signal: &str, pgid: u32) {
    let _ = tokio::process::Command::new(KILL_COMMAND)
        .args(kill_args(signal, pgid))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await;
}

fn kill_args(signal: &str, pgid: u32) -> [String; 3] {
    [format!("-{signal}"), "--".to_string(), format!("-{pgid}")]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn 끝내기가_시작되면_새_그룹을_받지_않는다() {
        let groups = Arc::new(Groups::default());
        let (tx, _rx) = oneshot::channel();
        let first = groups.register(u32::MAX, tx);
        assert!(first.is_some());
        drop(first);
        groups.shutdown(Duration::from_millis(10)).await;
        assert!(groups.is_closed());
        let (tx, _rx) = oneshot::channel();
        assert!(groups.register(u32::MAX, tx).is_none());
    }
}
