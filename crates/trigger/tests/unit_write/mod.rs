//! 2차 1: 작업 상태 옮기기 + 영수증 열기를 한 단위로 하는 포트 메서드(MemoryStore가 규칙을 지킨다).

use pi_trigger::{
    DeliveryState, JobId, JobState, MemoryStore, OpenReceipt, ReceiptKey, ReceiptStatus,
    TriggerStore,
};

use crate::support::utc;

fn key(job: &str, hour: u32) -> ReceiptKey {
    ReceiptKey {
        job: JobId::new(job),
        slot: utc(2026, 11, 16, hour, 0, 0),
    }
}

fn moved(job: &str, hour: u32) -> JobState {
    let slot = utc(2026, 11, 16, hour, 0, 0);
    let mut state = JobState::new(JobId::new(job), Some(slot));
    state.pending_slot = Some(slot);
    state
}

async fn saved(store: &MemoryStore, job: &str) -> Option<JobState> {
    store
        .jobs()
        .await
        .unwrap()
        .into_iter()
        .find(|state| state.id == JobId::new(job))
}

// 검증: E16
#[tokio::test]
async fn 열면_상태와_영수증을_함께_적는다() {
    let store = MemoryStore::new();
    let at = utc(2026, 11, 16, 0, 0, 1);
    let job = moved("open", 0);
    assert_eq!(
        store
            .save_and_open(&job, &key("open", 0), at)
            .await
            .unwrap(),
        OpenReceipt::Opened
    );
    assert_eq!(saved(&store, "open").await, Some(job));
    let receipts = store.receipts().await.unwrap();
    assert_eq!(receipts.len(), 1);
    assert_eq!(receipts[0].key, key("open", 0));
    assert_eq!(receipts[0].status, ReceiptStatus::Running);
    assert_eq!(receipts[0].delivery, DeliveryState::NotStarted);
    assert_eq!(receipts[0].started_at, at);
    assert_eq!(receipts[0].finished_at, None);
}

// 검증: E16
#[tokio::test]
async fn 다른_회차가_실행_중이면_아무것도_적지_않아_회차가_기한으로_남는다() {
    let store = MemoryStore::new();
    let at = utc(2026, 11, 16, 0, 0, 1);
    let first = moved("open", 0);
    store
        .save_and_open(&first, &key("open", 0), at)
        .await
        .unwrap();

    let next = moved("open", 1);
    assert_eq!(
        store
            .save_and_open(&next, &key("open", 1), at)
            .await
            .unwrap(),
        OpenReceipt::AlreadyRunning(key("open", 0))
    );
    assert_eq!(saved(&store, "open").await, Some(first), "상태는 그대로다");
    assert_eq!(store.receipts().await.unwrap().len(), 1);

    // 다른 작업은 따로다.
    assert_eq!(
        store
            .save_and_open(&moved("close", 1), &key("close", 1), at)
            .await
            .unwrap(),
        OpenReceipt::Opened
    );
}

// 검증: E16
#[tokio::test]
async fn 이미_다룬_회차면_상태는_옮기고_영수증은_그대로_둔다() {
    let store = MemoryStore::new();
    let at = utc(2026, 11, 16, 0, 0, 1);
    let first = moved("open", 0);
    store
        .save_and_open(&first, &key("open", 0), at)
        .await
        .unwrap();
    store
        .close_receipt(&key("open", 0), ReceiptStatus::Ok, at)
        .await
        .unwrap();

    let mut again = first.clone();
    again.consecutive_errors = 2;
    assert_eq!(
        store
            .save_and_open(&again, &key("open", 0), utc(2026, 11, 16, 0, 5, 0))
            .await
            .unwrap(),
        OpenReceipt::Duplicate
    );
    assert_eq!(saved(&store, "open").await, Some(again));
    let receipts = store.receipts().await.unwrap();
    assert_eq!(receipts.len(), 1);
    assert_eq!(receipts[0].status, ReceiptStatus::Ok);
    assert_eq!(receipts[0].started_at, at);
}
