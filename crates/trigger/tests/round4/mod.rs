//! 4차(3차 검토의 참고): 바깥 작업 안에서 저장소가 패닉해도 `in_flight`를 줄이고, `settle()`이 취소돼도
//! 띄운 깨우기를 잃지 않고, 일반 발사도 끝나면 `pending_slot`을 지운다.
//!
//! 실제로 잠들지 않는다. 오래 걸리는 깨우기는 가짜가 붙잡아 두고, 기다림은 차례 넘기기 횟수로 막는다.

use std::future::Future;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use pi_trigger::{
    JobState, OpenReceipt, Receipt, ReceiptKey, ReceiptStatus, ScheduleKind, StoreError, Target,
    Trigger, TriggerStore,
};

use crate::support::{FlakyStore, Harness, config, schedule, settled_tick, utc};

fn before_open() -> DateTime<Utc> {
    utc(2026, 11, 15, 23, 59, 0)
}

fn open_slot() -> DateTime<Utc> {
    utc(2026, 11, 16, 0, 0, 0)
}

fn open_job() -> Vec<pi_trigger::ScheduleJob> {
    vec![schedule("open", ScheduleKind::MarketOpen, Target::Team)]
}

/// 조건이 참이 될 때까지 다른 작업에 차례를 넘긴다(실제로 잠들지 않는다).
async fn until<F, Fut>(what: &str, mut check: F)
where
    F: FnMut() -> Fut,
    Fut: Future<Output = bool>,
{
    for _ in 0..1_000 {
        if check().await {
            return;
        }
        tokio::task::yield_now().await;
    }
    panic!("기다려도 되지 않았다: {what}");
}

/// 켜 두면 작업 상태를 읽을 때 패닉하는 저장소(안은 [`FlakyStore`]).
struct PanicStore {
    inner: Arc<FlakyStore>,
    armed: AtomicBool,
}

#[async_trait]
impl TriggerStore for PanicStore {
    async fn jobs(&self) -> Result<Vec<JobState>, StoreError> {
        if self.armed.load(Ordering::SeqCst) {
            panic!("가짜 저장소가 패닉함");
        }
        self.inner.jobs().await
    }

    async fn save_job(&self, job: &JobState) -> Result<(), StoreError> {
        self.inner.save_job(job).await
    }

    async fn open_receipt(
        &self,
        key: &ReceiptKey,
        started_at: DateTime<Utc>,
    ) -> Result<OpenReceipt, StoreError> {
        self.inner.open_receipt(key, started_at).await
    }

    async fn save_and_open(
        &self,
        job: &JobState,
        key: &ReceiptKey,
        started_at: DateTime<Utc>,
    ) -> Result<OpenReceipt, StoreError> {
        self.inner.save_and_open(job, key, started_at).await
    }

    async fn mark_delivery_started(&self, key: &ReceiptKey) -> Result<(), StoreError> {
        self.inner.mark_delivery_started(key).await
    }

    async fn close_receipt(
        &self,
        key: &ReceiptKey,
        status: ReceiptStatus,
        finished_at: DateTime<Utc>,
    ) -> Result<(), StoreError> {
        self.inner.close_receipt(key, status, finished_at).await
    }

    async fn receipts(&self) -> Result<Vec<Receipt>, StoreError> {
        self.inner.receipts().await
    }
}

// ---------------------------------------------------------------------------------------------
// 4차 1: 바깥 작업 안에서 저장소가 패닉해도 in_flight가 줄고 작업이 실행 중으로 굳지 않는다.

// 검증: E16
#[tokio::test]
async fn 바깥_작업_안에서_저장소가_패닉해도_in_flight가_줄어든다() {
    let h = Harness::new(before_open());
    let store = Arc::new(PanicStore {
        inner: h.store.clone(),
        armed: AtomicBool::new(false),
    });
    let mut ports = h.ports();
    ports.store = store.clone();
    let trigger = Trigger::new(config(open_job(), vec![]), ports).unwrap();
    settled_tick(&trigger).await;

    h.waker.hold(true);
    h.clock.set(open_slot());
    let report = trigger.tick().await.unwrap();
    assert_eq!(report.wakes.len(), 1);
    h.waker.entered().await;
    assert_eq!(trigger.in_flight(), 1);

    // 깨우기가 끝난 뒤 결과를 적으려고 저장소를 읽다가 패닉한다.
    store.armed.store(true, Ordering::SeqCst);
    h.waker.release();
    let _ = trigger.settle().await;
    until("패닉해도 끝난 것으로 센다", || async {
        trigger.in_flight() == 0
    })
    .await;
    assert_eq!(trigger.in_flight(), 0);
}

// ---------------------------------------------------------------------------------------------
// 4차 2: settle() future가 취소돼도 띄운 깨우기를 잃지 않는다.

// 검증: E16
#[tokio::test]
async fn settle이_취소돼도_띄운_깨우기를_다음_settle이_기다린다() {
    let h = Harness::new(before_open());
    let trigger = h.trigger(config(open_job(), vec![]));
    settled_tick(&trigger).await;

    h.waker.hold(true);
    h.clock.set(open_slot());
    trigger.tick().await.unwrap();
    h.waker.entered().await;

    // 첫 settle은 기다리다가 취소된다(버려진다).
    tokio::select! {
        biased;
        _ = trigger.settle() => panic!("붙잡힌 깨우기가 끝나기 전에 settle이 돌아옴"),
        () = std::future::ready(()) => {}
    }
    assert_eq!(trigger.in_flight(), 1, "취소해도 깨우기는 계속 돈다");

    h.waker.release();
    let report = trigger.settle().await.unwrap();
    assert_eq!(trigger.in_flight(), 0, "다음 settle이 끝까지 기다린다");
    assert_eq!(report.finished.len(), 1, "끝난 결과를 잃지 않는다");
    let receipt = h.store.receipt("open", open_slot()).await.unwrap();
    assert_eq!(receipt.status, ReceiptStatus::Ok);
}

// ---------------------------------------------------------------------------------------------
// 4차 3: 일반 발사도 끝나면 pending_slot을 지운다(영수증이 정리돼도 옛 회차를 되살리지 않게).

// 검증: E16
#[tokio::test]
async fn 일반_발사가_끝나면_pending_slot을_지운다() {
    let h = Harness::new(before_open());
    let trigger = h.trigger(config(open_job(), vec![]));
    settled_tick(&trigger).await;

    h.clock.set(open_slot());
    let report = settled_tick(&trigger).await;
    assert_eq!(report.wakes.len(), 1);

    let state = h.store.job("open").await;
    assert_eq!(state.last_slot, Some(open_slot()));
    assert_eq!(
        state.pending_slot, None,
        "끝난 회차는 주인 없는 회차가 아니다"
    );
    let receipt = h.store.receipt("open", open_slot()).await.unwrap();
    assert_eq!(receipt.status, ReceiptStatus::Ok);
}

// 검증: E16
#[tokio::test]
async fn 실패로_끝나도_pending_slot을_지운다() {
    let h = Harness::new(before_open());
    let trigger = h.trigger(config(open_job(), vec![]));
    settled_tick(&trigger).await;

    h.waker.fail(true);
    h.clock.set(open_slot());
    settled_tick(&trigger).await;

    let state = h.store.job("open").await;
    assert_eq!(state.consecutive_errors, 1);
    assert_eq!(state.pending_slot, None);
}
