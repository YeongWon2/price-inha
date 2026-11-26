//! 한 공급자가 띄우는 중이거나 띄운, 아직 살아 있는 자식 그룹의 명부(ai.md 3차 12 · 13, 4차 1 · 4).
//!
//! 호출은 자리표를 받자마자 명부에 자리를 잡고([`Groups::reserve`]), 띄운 뒤 그 자리에 그룹을
//! 적는다([`Registration::attach`]). [`ClaudeCliProvider::shutdown`](super::ClaudeCliProvider::shutdown)은
//! 이 명부를 보고 표준 입력을 닫은 뒤 [`CLI_STDIN_CLOSE_WAIT`] 동안 스스로 끝나기를 기다리고,
//! 남은 그룹마다 `SIGTERM` → 유예 → `SIGKILL`을 보내고, 명부가 빌 때까지 기다린다.
//! 띄우는 중이던 자리도 명부에 있으므로 끝내기는 그 호출이 자식을 거두고 자리를 놓을 때까지 기다린다.

use std::collections::HashMap;
use std::path::Path;
use std::process::{Command as StdCommand, Stdio};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use tokio::sync::{Notify, oneshot};

use super::CLI_STDIN_CLOSE_WAIT;

/// 그룹에 신호를 보내는 시스템 명령(유닉스 `/bin/kill`, 윈도 `taskkill` — [`super::os`]).
pub(super) const KILL_COMMAND: &str = super::os::KILL_COMMAND;

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

    /// 자식을 거뒀다고 적는다. 그 뒤로 끝내기는 이 그룹 번호에 신호를 보내지 않는다(ai.md 5차 6).
    /// 자리는 그대로 두므로 끝내기는 이 호출이 자리를 놓을 때까지 기다린다.
    pub(super) fn mark_reaped(&self) {
        if let Some(live) = self.groups.lock().live.get_mut(&self.id) {
            live.pgid = None;
            live.close_stdin = None;
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

    /// 명부에서 이미 띄웠고 아직 거두지 않은 그룹들.
    pub(super) fn live_pgids(&self) -> Vec<u32> {
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

/// `program`을 신호 명령으로 써서 그룹 전체에 신호를 보낸다(`kill -<신호> -- -<그룹>`).
/// 막히는 함수 안에서 쓴다. 신호 명령이 떠서 성공으로 끝났으면 참.
pub(super) fn signal_group(program: &Path, signal: &str, pgid: u32) -> bool {
    let Some(args) = kill_args(signal, pgid) else {
        return false;
    };
    StdCommand::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// [`KILL_COMMAND`]로 그룹 전체에 신호를 보내는 비동기판.
pub(super) async fn signal_group_async(signal: &str, pgid: u32) -> bool {
    signal_group_with(Path::new(KILL_COMMAND), signal, pgid).await
}

/// `program`을 신호 명령으로 써서 그룹 전체에 신호를 보낸다. 떠서 성공으로 끝났으면 참.
pub(super) async fn signal_group_with(program: &Path, signal: &str, pgid: u32) -> bool {
    let Some(args) = kill_args(signal, pgid) else {
        return false;
    };
    tokio::process::Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await
        .is_ok_and(|s| s.success())
}

/// 그룹에 `SIGTERM`을 보낸 뒤 `until`까지 그룹이 비기를 기다리고, 그때도 남아 있으면 `SIGKILL`을
/// 보낸다(ai.md 6차 1 — `SIGTERM`을 무시하는 손주가 남지 않게). 그룹 이끔이를 이미 거둔 뒤에도 쓴다:
/// 보내기 바로 앞에 그룹이 아직 있는지(`kill -0`) 확인하므로, 그룹이 비어 번호가 다른 데 다시 쓰일 수
/// 있게 된 뒤에는 신호를 보내지 않는다(그룹에 프로세스가 하나라도 있으면 그 번호는 다시 쓰이지 않는다).
///
/// init(좀비를 거두는 1번 프로세스)이 없는 컨테이너에서는 끝난 손주가 좀비로 남아 그룹이 비지 않는다
/// (`kill -0`이 좀비에도 성공한다). 그때는 매번 `until`까지 다 기다리고 `SIGKILL`을 보낸 뒤 돌아온다.
/// 기다림에 상한이 있으므로 자원 문제는 아니다(ai.md 7차 6).
pub(super) async fn kill_lingering_with(program: &Path, pgid: u32, until: tokio::time::Instant) {
    loop {
        if !signal_group_with(program, "0", pgid).await {
            return;
        }
        let now = tokio::time::Instant::now();
        if now >= until {
            signal_group_with(program, "KILL", pgid).await;
            return;
        }
        tokio::time::sleep(LINGER_POLL.min(until - now)).await;
    }
}

/// [`kill_lingering_with`]의 막히는 판. `grace` 동안 기다린다.
pub(super) fn kill_lingering(program: &Path, pgid: u32, grace: Duration) {
    let until = Instant::now().checked_add(grace);
    loop {
        if !signal_group(program, "0", pgid) {
            return;
        }
        let now = Instant::now();
        match until {
            Some(until) if now < until => std::thread::sleep(LINGER_POLL.min(until - now)),
            _ => {
                signal_group(program, "KILL", pgid);
                return;
            }
        }
    }
}

/// 남은 그룹이 비었는지 다시 보는 간격.
const LINGER_POLL: Duration = Duration::from_millis(20);

/// 신호 명령 인자(유닉스 `kill -<신호> -- -<그룹>`, 윈도 `taskkill /PID <번호> /T [/F]`). 보낼 수 없으면 `None`.
fn kill_args(signal: &str, pgid: u32) -> Option<Vec<String>> {
    super::os::kill_args(signal, pgid)
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

    #[tokio::test]
    async fn 거둔_그룹_번호는_끝내기가_신호를_보내지_않고_자리는_기다린다() {
        let groups = Arc::new(Groups::default());
        let reserved = groups.reserve().unwrap();
        let (tx, _rx) = oneshot::channel();
        assert!(reserved.attach(u32::MAX, tx));
        assert_eq!(groups.live_pgids(), vec![u32::MAX]);
        reserved.mark_reaped();
        assert!(
            groups.live_pgids().is_empty(),
            "거둔 그룹 번호가 명부에 남았다"
        );
        let shutting = {
            let groups = Arc::clone(&groups);
            tokio::spawn(async move { groups.shutdown(Duration::from_millis(10)).await })
        };
        tokio::time::sleep(Duration::from_millis(600)).await;
        assert!(
            !shutting.is_finished(),
            "자리를 놓기 전에 끝내기가 돌아왔다"
        );
        drop(reserved);
        assert!(shutting.await.is_ok());
    }
}
