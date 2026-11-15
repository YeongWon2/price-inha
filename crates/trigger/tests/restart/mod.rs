//! 다시 켠 뒤(E16)와 놓친 회차 · 장 밖(E18, 결정 20).

use pi_market::Market;
use pi_trigger::{
    DeliveryState, JobId, JobState, Receipt, ReceiptKey, ReceiptStatus, ScheduleKind, Target,
    TriggerEvent, WakeReason,
};

use crate::support::{Harness, config, kr_next_week_day, schedule, utc};

fn running(job: &str, slot: chrono::DateTime<chrono::Utc>, delivery: DeliveryState) -> Receipt {
    Receipt {
        key: ReceiptKey {
            job: JobId::new(job),
            slot,
        },
        status: ReceiptStatus::Running,
        delivery,
        started_at: slot,
        finished_at: None,
    }
}

// 검증: E16
#[tokio::test]
async fn 다시_켜면_실행_중이던_것은_중단됨으로_닫고_다시_돌리지_않는다() {
    let open_slot = utc(2026, 11, 16, 0, 0, 0);
    let close_slot = utc(2026, 11, 16, 6, 30, 0);
    let now = utc(2026, 11, 16, 6, 35, 0);
    let h = Harness::new(now);
    h.store
        .inner_save(&JobState::new(JobId::new("open"), Some(open_slot)))
        .await;
    h.store
        .inner_save(&JobState::new(JobId::new("close"), Some(close_slot)))
        .await;
    h.store
        .inner
        .insert_receipt(running("open", open_slot, DeliveryState::Started));
    h.store
        .inner
        .insert_receipt(running("close", close_slot, DeliveryState::NotStarted));

    let trigger = h.trigger(config(
        vec![
            schedule("open", ScheduleKind::MarketOpen, Target::Team),
            schedule("close", ScheduleKind::MarketClose, Target::Team),
        ],
        vec![],
    ));
    let report = trigger.start().await.unwrap();
    let mut interrupted = report.interrupted.clone();
    interrupted.sort_by(|a, b| a.key.cmp(&b.key));
    assert_eq!(interrupted.len(), 2);
    assert_eq!(interrupted[0].key.job, JobId::new("close"));
    assert_eq!(interrupted[0].delivery, DeliveryState::NotStarted);
    assert_eq!(interrupted[1].key.job, JobId::new("open"));
    assert_eq!(interrupted[1].delivery, DeliveryState::Started);

    for (job, slot) in [("open", open_slot), ("close", close_slot)] {
        let receipt = h.store.receipt(job, slot).await.unwrap();
        assert_eq!(receipt.status, ReceiptStatus::Interrupted);
        assert_eq!(receipt.finished_at, Some(now));
    }

    // 넘기기를 시작하지 않았던 것도 다시 돌리지 않는다.
    trigger.tick().await.unwrap();
    h.clock.set(utc(2026, 11, 16, 6, 40, 0));
    trigger.tick().await.unwrap();
    assert_eq!(h.waker.count(), 0);
}

// 검증: E16
#[tokio::test]
async fn 유예_안이면_지금_한_번_발사하고_앞서_놓친_회차는_세기만_한다() {
    // 금요일 회차 전부터 꺼져 있다가 월요일 09:10 KST에 켜졌다.
    let h = Harness::new(utc(2026, 11, 16, 0, 10, 0));
    h.store
        .inner_save(&JobState::new(
            JobId::new("open"),
            Some(utc(2026, 11, 12, 12, 0, 0)),
        ))
        .await;
    let trigger = h.trigger(config(
        vec![schedule("open", ScheduleKind::MarketOpen, Target::Team)],
        vec![],
    ));
    trigger.start().await.unwrap();
    trigger.tick().await.unwrap();

    let wakes = h.waker.wakes();
    assert_eq!(wakes.len(), 1);
    assert_eq!(wakes[0].reason, WakeReason::Schedule);
    assert_eq!(
        wakes[0].events,
        vec![TriggerEvent::Schedule {
            job: JobId::new("open"),
            kind: ScheduleKind::MarketOpen,
            market: Market::Kr,
            slot: utc(2026, 11, 16, 0, 0, 0),
            missed: 1,
        }]
    );
    // 놓친 금요일 회차는 따로 발사하지 않는다.
    assert!(
        h.store
            .receipt("open", utc(2026, 11, 13, 0, 0, 0))
            .await
            .is_none()
    );
}

// 검증: E16, E18
#[tokio::test]
async fn 유예를_넘었어도_정규장이_열려_있으면_한_번만_따라잡는다() {
    // 09:00 KST 회차를 11:30 KST에 처음 본다(2시간 30분 늦음 > 유예 2시간).
    let h = Harness::new(utc(2026, 11, 16, 2, 30, 0));
    h.store
        .inner_save(&JobState::new(
            JobId::new("open"),
            Some(utc(2026, 11, 15, 12, 0, 0)),
        ))
        .await;
    let trigger = h.trigger(config(
        vec![schedule("open", ScheduleKind::MarketOpen, Target::Team)],
        vec![],
    ));
    trigger.start().await.unwrap();
    trigger.tick().await.unwrap();
    h.clock.set(utc(2026, 11, 16, 2, 31, 0));
    trigger.tick().await.unwrap();

    assert_eq!(h.waker.count(), 1);
    match &h.waker.wakes()[0].events[0] {
        TriggerEvent::Schedule { slot, missed, .. } => {
            assert_eq!(*slot, utc(2026, 11, 16, 0, 0, 0));
            assert_eq!(*missed, 0);
        }
        other => panic!("일정 사건이어야 한다: {other:?}"),
    }
}

// 검증: E18, E16
#[tokio::test]
async fn 장_밖이면_따라잡지_않고_다음_장_시작_하나로_합친다() {
    // 월요일 19:00 KST에 켜졌다. 장 시작(09:00)과 마감 뒤(16:00)를 모두 놓쳤고 둘 다 유예를 넘었다.
    let h = Harness::new(utc(2026, 11, 16, 10, 0, 0));
    for id in ["open", "after"] {
        h.store
            .inner_save(&JobState::new(
                JobId::new(id),
                Some(utc(2026, 11, 15, 12, 0, 0)),
            ))
            .await;
    }
    let jobs = vec![
        schedule("open", ScheduleKind::MarketOpen, Target::Team),
        schedule("after", ScheduleKind::AfterClose, Target::Team),
    ];
    let trigger = h.trigger(config(jobs.clone(), vec![]));
    trigger.start().await.unwrap();
    trigger.tick().await.unwrap();
    assert_eq!(h.waker.count(), 0);

    let open = h.store.job("open").await;
    assert_eq!(open.pending_catch_up, vec![utc(2026, 11, 16, 0, 0, 0)]);
    assert_eq!(open.last_slot, Some(utc(2026, 11, 16, 0, 0, 0)));
    let after = h.store.job("after").await;
    assert_eq!(after.pending_catch_up, vec![utc(2026, 11, 16, 7, 0, 0)]);

    // 밤 동안에도 깨우지 않는다.
    h.clock.set(utc(2026, 11, 16, 12, 0, 0));
    trigger.tick().await.unwrap();
    assert_eq!(h.waker.count(), 0);

    // 다시 꺼졌다 켜져도 미뤄 둔 것은 남는다.
    drop(trigger);
    h.market.set_calendar(kr_next_week_day());
    h.clock.set(utc(2026, 11, 16, 23, 50, 0));
    let trigger = h.trigger(config(jobs, vec![]));
    trigger.start().await.unwrap();
    trigger.tick().await.unwrap();
    assert_eq!(h.waker.count(), 0);

    // 화요일 장 시작: 한 번에 묶어 깨운다.
    h.clock.set(utc(2026, 11, 17, 0, 0, 0));
    trigger.tick().await.unwrap();
    let wakes = h.waker.wakes();
    assert_eq!(wakes.len(), 1);
    let wake = &wakes[0];
    assert_eq!(wake.target, Target::Team);
    assert_eq!(wake.reason, WakeReason::CatchUp);
    assert_eq!(wake.events.len(), 3);
    assert!(wake.events.contains(&TriggerEvent::CatchUp {
        job: JobId::new("open"),
        kind: ScheduleKind::MarketOpen,
        market: Market::Kr,
        slots: vec![utc(2026, 11, 16, 0, 0, 0)],
    }));
    assert!(wake.events.contains(&TriggerEvent::CatchUp {
        job: JobId::new("after"),
        kind: ScheduleKind::AfterClose,
        market: Market::Kr,
        slots: vec![utc(2026, 11, 16, 7, 0, 0)],
    }));
    assert!(wake.events.contains(&TriggerEvent::Schedule {
        job: JobId::new("open"),
        kind: ScheduleKind::MarketOpen,
        market: Market::Kr,
        slot: utc(2026, 11, 17, 0, 0, 0),
        missed: 0,
    }));
    assert!(h.store.job("open").await.pending_catch_up.is_empty());
    assert!(h.store.job("after").await.pending_catch_up.is_empty());
    let receipt = h
        .store
        .receipt("after", utc(2026, 11, 16, 7, 0, 0))
        .await
        .unwrap();
    assert_eq!(receipt.status, ReceiptStatus::Ok);

    // 한 번 넘긴 것은 다시 넘기지 않는다.
    h.clock.set(utc(2026, 11, 17, 0, 1, 0));
    trigger.tick().await.unwrap();
    assert_eq!(h.waker.count(), 1);
}

// 검증: E18
#[tokio::test]
async fn 장_밖이라도_유예_안의_마감_뒤_회차는_그냥_발사한다() {
    let h = Harness::new(utc(2026, 11, 16, 6, 59, 0));
    let trigger = h.trigger(config(
        vec![schedule("after", ScheduleKind::AfterClose, Target::Team)],
        vec![],
    ));
    trigger.tick().await.unwrap();
    h.clock.set(utc(2026, 11, 16, 7, 30, 0));
    trigger.tick().await.unwrap();
    let wakes = h.waker.wakes();
    assert_eq!(wakes.len(), 1);
    assert_eq!(
        wakes[0].events,
        vec![TriggerEvent::Schedule {
            job: JobId::new("after"),
            kind: ScheduleKind::AfterClose,
            market: Market::Kr,
            slot: utc(2026, 11, 16, 7, 0, 0),
            missed: 0,
        }]
    );
}
