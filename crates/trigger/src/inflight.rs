//! 띄워 둔 깨우기(2차 2번, 원본: Hermes 1.1 12·13단계, OpenClaw).
//!
//! tick은 깨우기를 대상마다 따로 띄우고 끝나기를 기다리지 않는다. 띄운 쪽이 끝나면 작업 상태를 고치고
//! (연속 실패 수 · 물러서기 · 끄기 · 다시 걸기 상태) 영수증을 닫는다. 실행 중 영수증이 같은 작업의 중복을 막는다.

use std::collections::HashMap;
use std::mem;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use chrono::{DateTime, Utc};
use tokio::task::JoinHandle;

use crate::engine::lock;
use crate::{
    AUTO_DISABLE_AFTER, Alert, AlertReason, ConditionMemory, DisableReason, Drained, Error,
    Finished, JobId, JobState, Ports, ReceiptKey, ReceiptStatus, SettleReport, StoreError, Target,
    Trigger, TriggerEvent, TriggerStore, Wake, error_backoff,
};

/// 발사하기로 한 작업 하나.
pub(crate) struct Firing {
    pub(crate) key: ReceiptKey,
    pub(crate) target: Target,
    pub(crate) events: Vec<TriggerEvent>,
    /// 깨우기가 성공하면 저장할 다시 걸기 상태(시세 조건).
    pub(crate) on_success: Option<ConditionMemory>,
    /// 영수증과 한 단위로 저장할 옮긴 상태.
    pub(crate) state: JobState,
    /// 다시 켤 때 되살린 회차인가(쓰기가 실패하면 다음 tick에 다시 해 본다).
    pub(crate) revived: bool,
}

/// 띄워 둔 깨우기들과, 끝난 결과 가운데 아직 내지 않은 것.
#[derive(Default)]
pub(crate) struct InFlight {
    /// tick의 상태 쓰기와 끝난 결과의 반영을 하나씩만 한다(서로의 상태를 덮어쓰지 않게).
    pub(crate) writes: tokio::sync::Mutex<()>,
    count: AtomicUsize,
    /// 띄운 깨우기에 묶여 아직 끝나지 않은 작업들(작업마다 묶인 깨우기 수).
    jobs: Mutex<HashMap<JobId, usize>>,
    handles: Mutex<Vec<JoinHandle<()>>>,
    finished: Mutex<Vec<Finished>>,
    alerts: Mutex<Vec<Alert>>,
    errors: Mutex<Vec<StoreError>>,
    /// 띄운 깨우기가 하나 끝날 때마다 알린다(`settle`이 취소돼도 잃는 것이 없게 이것으로 기다린다).
    idle: tokio::sync::Notify,
}

impl InFlight {
    /// 띄우고 아직 끝나지 않은 깨우기 수.
    pub(crate) fn count(&self) -> usize {
        self.count.load(Ordering::SeqCst)
    }

    /// 그 작업이 띄운 깨우기에 묶여 아직 실행 중인가.
    pub(crate) fn is_running(&self, job: &JobId) -> bool {
        lock(&self.jobs).contains_key(job)
    }

    /// 기다리지 않고 지금까지 끝난 결과와 반영하다 생긴 저장소 오류를 가져간다(한 번만 낸다).
    pub(crate) fn drain(&self) -> Drained {
        lock(&self.handles).retain(|handle| !handle.is_finished());
        Drained {
            finished: mem::take(&mut *lock(&self.finished)),
            errors: mem::take(&mut *lock(&self.errors)),
        }
    }

    fn enter(&self, members: &[Firing]) {
        self.count.fetch_add(1, Ordering::SeqCst);
        let mut jobs = lock(&self.jobs);
        for firing in members {
            *jobs.entry(firing.key.job.clone()).or_default() += 1;
        }
    }

    fn leave(&self, jobs_done: &[JobId]) {
        {
            let mut jobs = lock(&self.jobs);
            for job in jobs_done {
                if let Some(count) = jobs.get_mut(job) {
                    *count = count.saturating_sub(1);
                    if *count == 0 {
                        jobs.remove(job);
                    }
                }
            }
        }
        self.count.fetch_sub(1, Ordering::SeqCst);
        self.idle.notify_waiters();
    }

    /// 아직 내지 않은 알림 요구를 가져간다(한 번만 낸다).
    pub(crate) fn take_alerts(&self) -> Vec<Alert> {
        mem::take(&mut *lock(&self.alerts))
    }

    /// 띄운 깨우기가 모두 끝날 때까지 기다리고, 끝난 것과 아직 내지 않은 알림 요구를 낸다.
    ///
    /// 취소해도 안전하다(4차 2번): 핸들을 꺼내 쥐고 기다리지 않고 끝난 수가 0이 되기를 기다리므로,
    /// 이 future가 버려져도 띄운 깨우기는 그대로 돌고 다음 `settle`이 끝까지 기다린다.
    pub(crate) async fn settle(&self) -> Result<SettleReport, Error> {
        loop {
            let notified = self.idle.notified();
            tokio::pin!(notified);
            // 알림을 놓치지 않게 먼저 등록하고 나서 수를 본다.
            notified.as_mut().enable();
            if self.count() == 0 {
                break;
            }
            notified.await;
        }
        // 모두 끝났으니 핸들은 곧바로 끝난다. 패닉으로 끝난 것만 남긴다.
        let handles = mem::take(&mut *lock(&self.handles));
        for handle in handles {
            if let Err(error) = handle.await {
                tracing::error!(%error, "띄운 깨우기가 끝까지 돌지 못했다");
            }
        }
        let errors = mem::take(&mut *lock(&self.errors));
        if let Some(error) = errors.into_iter().next() {
            return Err(Error::Store(error));
        }
        Ok(SettleReport {
            finished: mem::take(&mut *lock(&self.finished)),
            alerts: self.take_alerts(),
        })
    }
}

/// 깨우기 하나를 띄운다. 끝나면 묶인 작업마다 상태를 고치고 영수증을 닫는다.
pub(crate) fn launch(trigger: &Trigger, wake: Wake, members: Vec<Firing>) {
    let ports = trigger.ports.clone();
    let inflight = Arc::clone(&trigger.inflight);
    inflight.enter(&members);
    let task = Arc::clone(&inflight);
    let handle = tokio::spawn(async move {
        let ok = wake_guarded(&ports, &wake).await;
        let jobs: Vec<JobId> = members
            .iter()
            .map(|firing| firing.key.job.clone())
            .collect();
        // 저장소가 패닉해도 `in_flight`가 줄고 작업이 실행 중으로 굳지 않게 drop에서 나간다(4차 1번).
        let _leave = LeaveGuard {
            inflight: Arc::clone(&task),
            jobs,
        };
        settle_members(&ports, &task, members, ok).await;
        lock(&task.finished).push(Finished { wake, ok });
    });
    let mut handles = lock(&inflight.handles);
    handles.retain(|handle| !handle.is_finished());
    handles.push(handle);
}

/// 띄운 바깥 작업이 어떻게 끝나든(패닉 포함) 묶인 작업과 수를 풀어 주는 지킴이.
struct LeaveGuard {
    inflight: Arc<InFlight>,
    jobs: Vec<JobId>,
}

impl Drop for LeaveGuard {
    fn drop(&mut self) {
        self.inflight.leave(&self.jobs);
    }
}

/// 깨우기를 따로 띄워 돌린다. 실패하거나 패닉하면 거짓(실패로 센다, 3차 1번).
async fn wake_guarded(ports: &Ports, wake: &Wake) -> bool {
    let waker = Arc::clone(&ports.waker);
    let sent = wake.clone();
    match tokio::spawn(async move { waker.wake(sent).await }).await {
        Ok(Ok(())) => true,
        Ok(Err(error)) => {
            tracing::warn!(target = ?wake.target, %error, "깨운 실행이 실패했다");
            false
        }
        Err(error) => {
            tracing::error!(target = ?wake.target, %error, "깨운 실행이 패닉했다. 실패로 닫는다");
            false
        }
    }
}

async fn settle_members(ports: &Ports, inflight: &InFlight, members: Vec<Firing>, ok: bool) {
    let _writes = inflight.writes.lock().await;
    let finished_at = ports.clock.now();
    for firing in members {
        let (alert, errors) = finish(ports.store.as_ref(), firing, ok, finished_at).await;
        lock(&inflight.alerts).extend(alert);
        lock(&inflight.errors).extend(errors);
    }
}

/// 깨운 결과로 작업 상태를 고치고(저장소에서 지금 상태를 다시 읽는다) 영수증을 닫는다.
/// 상태를 저장하지 못해도 영수증은 닫는다(실행 중으로 남아 작업이 막히지 않게). 끈 순간에만 알림 요구를 낸다.
async fn finish(
    store: &dyn TriggerStore,
    firing: Firing,
    ok: bool,
    finished_at: DateTime<Utc>,
) -> (Option<Alert>, Vec<StoreError>) {
    let mut errors = Vec::new();
    let mut alert = None;
    match store.jobs().await {
        Ok(jobs) => {
            if let Some(mut state) = jobs.into_iter().find(|job| job.id == firing.key.job) {
                let raised = apply_result(&mut state, firing.on_success, ok, finished_at);
                // 끝난 회차는 주인 없는 회차가 아니다. 영수증이 정리돼도 옛 회차를 되살리지 않게 지운다.
                if state.pending_slot == Some(firing.key.slot) {
                    state.pending_slot = None;
                }
                match store.save_job(&state).await {
                    Ok(()) => alert = raised,
                    Err(error) => errors.push(error),
                }
            }
        }
        Err(error) => errors.push(error),
    }
    let status = if ok {
        ReceiptStatus::Ok
    } else {
        ReceiptStatus::Error
    };
    if let Err(error) = store.close_receipt(&firing.key, status, finished_at).await {
        errors.push(error);
    }
    for error in &errors {
        tracing::warn!(job = %firing.key.job, %error, "깨운 결과를 저장소에 적지 못했다");
    }
    (alert, errors)
}

/// 성공이면 연속 실패를 지우고 다시 걸기 상태를 적는다. 실패면 물러서고 10번이면 끈다.
fn apply_result(
    state: &mut JobState,
    on_success: Option<ConditionMemory>,
    ok: bool,
    finished_at: DateTime<Utc>,
) -> Option<Alert> {
    if ok {
        state.consecutive_errors = 0;
        state.retry_at = None;
        if let Some(memory) = on_success {
            state.condition = Some(memory);
        }
        return None;
    }
    state.consecutive_errors = state.consecutive_errors.saturating_add(1);
    let backoff = chrono::Duration::from_std(error_backoff(state.consecutive_errors))
        .unwrap_or(chrono::TimeDelta::MAX);
    state.retry_at = finished_at.checked_add_signed(backoff);
    if state.consecutive_errors < AUTO_DISABLE_AFTER || state.disabled.is_some() {
        return None;
    }
    state.disabled = Some(DisableReason::ConsecutiveFailures);
    tracing::warn!(job = %state.id, errors = state.consecutive_errors, "연달아 실패해 작업을 껐다");
    Some(Alert {
        job: state.id.clone(),
        reason: AlertReason::AutoDisabled {
            consecutive_errors: state.consecutive_errors,
        },
    })
}
