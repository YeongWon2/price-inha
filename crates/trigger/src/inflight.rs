//! 띄워 둔 깨우기(2차 2번, 원본: Hermes 1.1 12·13단계, OpenClaw).
//!
//! tick은 깨우기를 대상마다 따로 띄우고 끝나기를 기다리지 않는다. 띄운 쪽이 끝나면 작업 상태를 고치고
//! (연속 실패 수 · 물러서기 · 끄기 · 다시 걸기 상태) 영수증을 닫는다. 실행 중 영수증이 같은 작업의 중복을 막는다.

use std::mem;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use chrono::{DateTime, Utc};
use tokio::task::JoinHandle;

use crate::engine::lock;
use crate::{
    AUTO_DISABLE_AFTER, Alert, AlertReason, ConditionMemory, DisableReason, Error, Finished,
    JobState, Ports, ReceiptKey, ReceiptStatus, SettleReport, StoreError, Target, Trigger,
    TriggerEvent, TriggerStore, Wake, error_backoff,
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
    handles: Mutex<Vec<JoinHandle<()>>>,
    finished: Mutex<Vec<Finished>>,
    alerts: Mutex<Vec<Alert>>,
    errors: Mutex<Vec<StoreError>>,
}

impl InFlight {
    /// 띄우고 아직 끝나지 않은 깨우기 수.
    pub(crate) fn count(&self) -> usize {
        self.count.load(Ordering::SeqCst)
    }

    /// 아직 내지 않은 알림 요구를 가져간다(한 번만 낸다).
    pub(crate) fn take_alerts(&self) -> Vec<Alert> {
        mem::take(&mut *lock(&self.alerts))
    }

    /// 띄운 깨우기가 모두 끝날 때까지 기다리고, 끝난 것과 아직 내지 않은 알림 요구를 낸다.
    pub(crate) async fn settle(&self) -> Result<SettleReport, Error> {
        loop {
            let handles = mem::take(&mut *lock(&self.handles));
            if handles.is_empty() {
                break;
            }
            for handle in handles {
                if let Err(error) = handle.await {
                    tracing::error!(%error, "띄운 깨우기가 끝까지 돌지 못했다");
                }
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
    inflight.count.fetch_add(1, Ordering::SeqCst);
    let task = Arc::clone(&inflight);
    let handle = tokio::spawn(async move {
        let result = ports.waker.wake(wake.clone()).await;
        if let Err(error) = &result {
            tracing::warn!(target = ?wake.target, %error, "깨운 실행이 실패했다");
        }
        let ok = result.is_ok();
        settle_members(&ports, &task, members, ok).await;
        lock(&task.finished).push(Finished { wake, ok });
        task.count.fetch_sub(1, Ordering::SeqCst);
    });
    let mut handles = lock(&inflight.handles);
    handles.retain(|handle| !handle.is_finished());
    handles.push(handle);
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
