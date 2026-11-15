//! 한 공급자가 띄우는 중이거나 띄운, 아직 살아 있는 자식 그룹의 명부(ai.md 3차 12 · 13, 4차 1 · 4).
//!
//! 호출은 자리표를 받자마자 명부에 자리를 잡고([`Groups::reserve`]), 띄운 뒤 그 자리에 그룹을
//! 적는다([`Registration::attach`]). [`ClaudeCliProvider::shutdown`](super::ClaudeCliProvider::shutdown)은
//! 이 명부를 보고 표준 입력을 닫은 뒤 [`CLI_STDIN_CLOSE_WAIT`] 동안 스스로 끝나기를 기다리고,
//! 남은 그룹마다 `SIGTERM` → 유예 → `SIGKILL`을 보내고, 명부가 빌 때까지 기다린다.
//! 띄우는 중이던 자리도 명부에 있으므로 끝내기는 그 호출이 자식을 거두고 자리를 놓을 때까지 기다린다.

use std::collections::HashMap;
use std::process::{Command as StdCommand, Stdio};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use tokio::sync::{Notify, oneshot};

use super::CLI_STDIN_CLOSE_WAIT;

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
    /// 띄운 그룹. 아직 띄우는 중이면 `None`.
    pgid: Option<u32>,
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

impl Registration {
    /// 띄운 그룹을 자리에 적는다. 그 사이 끝내기가 시작됐으면 적지 않고 거짓을 돌려준다
    /// (그때는 부르는 쪽이 그 자리에서 자식을 끝내고 거둔 뒤 자리를 놓는다).
    pub(super) fn attach(&self, pgid: u32, close_stdin: oneshot::Sender<()>) -> bool {
        let mut inner = self.groups.lock();
        if inner.closed {
            return false;
        }
        match inner.live.get_mut(&self.id) {
            Some(live) => {
                live.pgid = Some(pgid);
                live.close_stdin = Some(close_stdin);
                true
            }
            None => false,
        }
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

    /// 띄우기 전에 명부에 자리를 잡는다. 끝내기가 이미 시작됐으면 잡지 않고 `None`.
    pub(super) fn reserve(self: &Arc<Self>) -> Option<Registration> {
        let mut inner = self.lock();
        if inner.closed {
            return None;
        }
        let id = inner.next_id;
        inner.next_id += 1;
        inner.live.insert(
            id,
            Live {
                pgid: None,
                close_stdin: None,
            },
        );
        Some(Registration {
            groups: Arc::clone(self),
            id,
        })
    }

    /// 새 그룹을 받지 않게 하고, 살아 있는 그룹을 모두 끝낸 뒤 명부가 빌 때까지 기다린다.
    ///
    /// 표준 입력을 먼저 닫고 [`CLI_STDIN_CLOSE_WAIT`] 동안 스스로 끝나기를 기다린다(원본 2.7).
    /// 남은 그룹에 `SIGTERM`, 유예 안에 다 끝나지 않으면 남은 그룹에 `SIGKILL`.
    pub(super) async fn shutdown(&self, grace: Duration) {
        let senders: Vec<oneshot::Sender<()>> = {
            let mut inner = self.lock();
            inner.closed = true;
            inner
                .live
                .values_mut()
                .filter_map(|live| live.close_stdin.take())
                .collect()
        };
        if self.lock().live.is_empty() {
            return;
        }
        for tx in senders {
            let _ = tx.send(());
        }
        if tokio::time::timeout(CLI_STDIN_CLOSE_WAIT, self.wait_empty())
            .await
            .is_ok()
        {
            return;
        }
        for pgid in self.live_pgids() {
            signal_group_async("TERM", pgid).await;
        }
        if tokio::time::timeout(grace, self.wait_empty()).await.is_ok() {
            return;
        }
        for pgid in self.live_pgids() {
            signal_group_async("KILL", pgid).await;
        }
        self.wait_empty().await;
    }

    /// 명부에서 이미 띄운 그룹들.
    fn live_pgids(&self) -> Vec<u32> {
        self.lock().live.values().filter_map(|l| l.pgid).collect()
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
pub(super) async fn signal_group_async(signal: &str, pgid: u32) {
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
        let first = groups.reserve();
        assert!(first.is_some());
        drop(first);
        groups.shutdown(Duration::from_millis(10)).await;
        assert!(groups.is_closed());
        assert!(groups.reserve().is_none());
    }

    #[tokio::test]
    async fn 띄우는_중에_끝내기가_오면_적기를_거부하고_자리를_놓을_때까지_기다린다() {
        let groups = Arc::new(Groups::default());
        let reserved = groups.reserve();
        assert!(reserved.is_some(), "끝내기 전에는 자리를 잡는다");
        let shutting = {
            let groups = Arc::clone(&groups);
            tokio::spawn(async move { groups.shutdown(Duration::from_millis(10)).await })
        };
        while !groups.is_closed() {
            tokio::task::yield_now().await;
        }
        let (tx, _rx) = oneshot::channel();
        assert!(!reserved.as_ref().is_some_and(|r| r.attach(u32::MAX, tx)));
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(
            !shutting.is_finished(),
            "자리를 놓기 전에 끝내기가 돌아왔다"
        );
        drop(reserved);
        assert!(shutting.await.is_ok());
    }
}
