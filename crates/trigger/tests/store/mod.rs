//! 영수증 `(작업, 예정 시각)`과 실행 중은 하나(MemoryStore가 포트 규칙을 지킨다).

use pi_trigger::{
    DeliveryState, DisableReason, JobId, JobState, MemoryStore, OpenReceipt, ReceiptKey,
    ReceiptStatus, TriggerStore,
};

use crate::support::utc;

fn key(job: &str, hour: u32) -> ReceiptKey {
    ReceiptKey {
        job: JobId::new(job),
        slot: utc(2026, 11, 16, hour, 0, 0),
    }
}

// 검증: E16
#[tokio::test]
async fn 같은_회차는_두_번_열리지_않고_실행_중은_작업마다_하나다() {
    let store = MemoryStore::new();
    let at = utc(2026, 11, 16, 0, 0, 1);
    assert_eq!(
        store.open_receipt(&key("open", 0), at).await.unwrap(),
        OpenReceipt::Opened
    );
    assert_eq!(
        store.open_receipt(&key("open", 0), at).await.unwrap(),
        OpenReceipt::Duplicate
    );
    assert_eq!(
        store.open_receipt(&key("open", 1), at).await.unwrap(),
        OpenReceipt::AlreadyRunning(key("open", 0))
    );
    // 다른 작업은 따로다.
    assert_eq!(
        store.open_receipt(&key("close", 1), at).await.unwrap(),
        OpenReceipt::Opened
    );
    store
        .close_receipt(&key("open", 0), ReceiptStatus::Ok, at)
        .await
        .unwrap();
    assert_eq!(
        store.open_receipt(&key("open", 1), at).await.unwrap(),
        OpenReceipt::Opened
    );
    // 닫힌 회차도 다시 열리지 않는다.
    assert_eq!(
        store.open_receipt(&key("open", 0), at).await.unwrap(),
        OpenReceipt::Duplicate
    );
}

// 검증: E16
#[tokio::test]
async fn 영수증은_넘기기_시작과_닫은_시각을_적는다() {
    let store = MemoryStore::new();
    let started = utc(2026, 11, 16, 0, 0, 1);
    let finished = utc(2026, 11, 16, 0, 0, 9);
    store.open_receipt(&key("open", 0), started).await.unwrap();
    let opened = store.receipts().await.unwrap();
    assert_eq!(opened.len(), 1);
    assert_eq!(opened[0].status, ReceiptStatus::Running);
    assert_eq!(opened[0].delivery, DeliveryState::NotStarted);
    assert_eq!(opened[0].started_at, started);
    assert_eq!(opened[0].finished_at, None);

    store.mark_delivery_started(&key("open", 0)).await.unwrap();
    store
        .close_receipt(&key("open", 0), ReceiptStatus::Error, finished)
        .await
        .unwrap();
    let closed = store.receipts().await.unwrap();
    assert_eq!(closed[0].delivery, DeliveryState::Started);
    assert_eq!(closed[0].status, ReceiptStatus::Error);
    assert_eq!(closed[0].finished_at, Some(finished));
}

// 검증: E16
#[tokio::test]
async fn 작업_상태는_저장하고_같은_id는_바꾼다() {
    let store = MemoryStore::new();
    let mut job = JobState::new(JobId::new("open"), Some(utc(2026, 11, 16, 0, 0, 0)));
    store.save_job(&job).await.unwrap();
    job.consecutive_errors = 3;
    job.disabled = Some(DisableReason::Manual);
    store.save_job(&job).await.unwrap();
    store
        .save_job(&JobState::new(JobId::new("close"), None))
        .await
        .unwrap();
    let mut jobs = store.jobs().await.unwrap();
    jobs.sort_by(|a, b| a.id.cmp(&b.id));
    assert_eq!(jobs.len(), 2);
    assert_eq!(jobs[1], job);
    assert_eq!(jobs[0].id, JobId::new("close"));
}
