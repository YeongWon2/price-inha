//! 2차 3: 다시 켤 때 상태는 옮겼는데 영수증이 없는 회차(원본 Hermes의 주인 없는 대기 회차)는 되살린다.

use pi_trigger::{
    DeliveryState, JobId, JobState, Receipt, ReceiptKey, ReceiptStatus, ScheduleKind, Target,
    TriggerEvent,
};

use crate::support::{Harness, config, schedule, secs, settled_tick, utc};

fn open_slot() -> chrono::DateTime<chrono::Utc> {
    utc(2026, 11, 16, 0, 0, 0)
}

fn brief_slot() -> chrono::DateTime<chrono::Utc> {
    utc(2026, 11, 15, 23, 30, 0)
}

/// 상태를 그 회차로 옮기고 발사하기로 적어 둔 모습(영수증은 아직 없다).
fn moved(job: &str, slot: chrono::DateTime<chrono::Utc>) -> JobState {
    let mut state = JobState::new(JobId::new(job), Some(slot));
    state.pending_slot = Some(slot);
    state
}

fn receipt(
    job: &str,
    slot: chrono::DateTime<chrono::Utc>,
    status: ReceiptStatus,
    finished: Option<chrono::DateTime<chrono::Utc>>,
) -> Receipt {
    Receipt {
        key: ReceiptKey {
            job: JobId::new(job),
            slot,
        },
        status,
        delivery: DeliveryState::Started,
        started_at: slot,
        finished_at: finished,
    }
}

// 검증: E16
#[tokio::test]
async fn 영수증_없이_옮겨진_회차는_되살려_한_번_발사한다() {
    let h = Harness::new(utc(2026, 11, 16, 0, 5, 0));
    h.store.inner_save(&moved("open", open_slot())).await;
    let jobs = vec![schedule("open", ScheduleKind::MarketOpen, Target::Team)];
    let trigger = h.trigger(config(jobs.clone(), vec![]));

    let report = trigger.start().await.unwrap();
    assert!(report.interrupted.is_empty());
    assert_eq!(
        report.revived,
        vec![ReceiptKey {
            job: JobId::new("open"),
            slot: open_slot(),
        }]
    );

    settled_tick(&trigger).await;
    let wakes = h.waker.wakes();
    assert_eq!(wakes.len(), 1);
    assert_eq!(wakes[0].target, Target::Team);
    assert!(
        wakes[0].events.iter().any(|event| matches!(
            event,
            TriggerEvent::Schedule { job, slot, .. }
                if *job == JobId::new("open") && *slot == open_slot()
        )),
        "되살린 회차로 발사해야 한다: {:?}",
        wakes[0].events
    );
    let opened = h.store.receipt("open", open_slot()).await.unwrap();
    assert_eq!(opened.status, ReceiptStatus::Ok);

    // 한 번 나간 회차는 다시 나가지 않는다.
    h.clock.advance(secs(60));
    settled_tick(&trigger).await;
    assert_eq!(h.waker.count(), 1);

    // 다시 꺼졌다 켜져도 영수증이 있으므로 되살리지 않는다.
    drop(trigger);
    let trigger = h.trigger(config(jobs, vec![]));
    let again = trigger.start().await.unwrap();
    assert!(again.revived.is_empty());
    settled_tick(&trigger).await;
    assert_eq!(h.waker.count(), 1);
}

// 검증: E16
#[tokio::test]
async fn 영수증이_있는_회차는_닫혔든_실행_중이든_되살리지_않는다() {
    let now = utc(2026, 11, 16, 0, 5, 0);
    let h = Harness::new(now);
    // 장 시작은 끝까지 돌았고, 브리핑은 실행 중에 꺼졌다.
    h.store.inner_save(&moved("open", open_slot())).await;
    h.store.inner.insert_receipt(receipt(
        "open",
        open_slot(),
        ReceiptStatus::Ok,
        Some(open_slot()),
    ));
    h.store.inner_save(&moved("brief", brief_slot())).await;
    h.store
        .inner
        .insert_receipt(receipt("brief", brief_slot(), ReceiptStatus::Running, None));

    let trigger = h.trigger(config(
        vec![
            schedule("open", ScheduleKind::MarketOpen, Target::Team),
            schedule("brief", ScheduleKind::PreOpenBriefing, Target::Team),
        ],
        vec![],
    ));
    let report = trigger.start().await.unwrap();
    assert!(report.revived.is_empty());
    assert_eq!(report.interrupted.len(), 1);
    assert_eq!(report.interrupted[0].key.job, JobId::new("brief"));

    settled_tick(&trigger).await;
    h.clock.advance(secs(60));
    settled_tick(&trigger).await;
    assert_eq!(h.waker.count(), 0);
    assert_eq!(
        h.store.receipt("brief", brief_slot()).await.unwrap().status,
        ReceiptStatus::Interrupted
    );
}

// 검증: E16
#[tokio::test]
async fn 발사하기로_적지_않은_회차는_영수증이_없어도_되살리지_않는다() {
    // 처음 보는 작업(지금 시각으로 채움)과 놓친 것으로 옮긴 작업은 발사하기로 한 회차가 없다.
    let h = Harness::new(utc(2026, 11, 16, 0, 5, 0));
    h.store
        .inner_save(&JobState::new(JobId::new("open"), Some(open_slot())))
        .await;
    let trigger = h.trigger(config(
        vec![schedule("open", ScheduleKind::MarketOpen, Target::Team)],
        vec![],
    ));
    let report = trigger.start().await.unwrap();
    assert!(report.revived.is_empty());
    settled_tick(&trigger).await;
    assert_eq!(h.waker.count(), 0);
}
