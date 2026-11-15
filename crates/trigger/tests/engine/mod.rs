//! tick 순서(잠금 → 전체 정지 → 다시 읽기 → 기한 판정 → 먼저 옮기고 발사), 묶어 깨우기, 설정 확인.

use std::sync::Arc;

use pi_market::{FakeAdapter, Market, MarketAdapter, Symbol};
use pi_trigger::{
    ConditionJob, ConditionKind, DeliveryState, DisableReason, Error, JobId, Receipt, ReceiptKey,
    ReceiptStatus, ScheduleJob, ScheduleKind, Target, TickOutcome, Trigger, TriggerEvent, Wake,
    WakeReason,
};

use crate::support::{
    Harness, condition, config, d, date, holiday, kr_calendar, kr_day, schedule, utc,
};

fn open_job() -> ScheduleJob {
    schedule("open", ScheduleKind::MarketOpen, Target::Team)
}

fn before_open() -> chrono::DateTime<chrono::Utc> {
    utc(2026, 11, 15, 23, 59, 0)
}

fn open_slot() -> chrono::DateTime<chrono::Utc> {
    utc(2026, 11, 16, 0, 0, 0)
}

// 검증: F1, E16
#[tokio::test]
async fn 처음_보는_작업은_지난_회차를_발사하지_않는다() {
    let h = Harness::new(utc(2026, 11, 16, 0, 10, 0));
    let trigger = h.trigger(config(vec![open_job()], vec![]));
    let report = trigger.tick().await.unwrap();
    assert_eq!(report.outcome, TickOutcome::Ran);
    assert!(report.wakes.is_empty());
    assert_eq!(h.waker.count(), 0);
    let job = h.store.job("open").await;
    assert_eq!(job.last_slot, Some(utc(2026, 11, 16, 0, 10, 0)));
}

// 검증: F1, E16
#[tokio::test]
async fn 장_시작_회차에_한_번_깨우고_영수증을_닫는다() {
    let h = Harness::new(before_open());
    let trigger = h.trigger(config(vec![open_job()], vec![]));
    trigger.tick().await.unwrap();
    assert_eq!(h.waker.count(), 0);

    h.clock.set(open_slot());
    let report = trigger.tick().await.unwrap();
    let expected = Wake {
        target: Target::Team,
        reason: WakeReason::Schedule,
        events: vec![TriggerEvent::Schedule {
            job: JobId::new("open"),
            kind: ScheduleKind::MarketOpen,
            market: Market::Kr,
            slot: open_slot(),
            missed: 0,
        }],
    };
    assert_eq!(h.waker.wakes(), vec![expected.clone()]);
    assert_eq!(report.wakes, vec![expected]);

    let receipt = h.store.receipt("open", open_slot()).await.unwrap();
    assert_eq!(receipt.status, ReceiptStatus::Ok);
    assert_eq!(receipt.delivery, DeliveryState::Started);
    assert_eq!(receipt.started_at, open_slot());
    assert!(receipt.finished_at.is_some());
    assert_eq!(h.store.job("open").await.last_slot, Some(open_slot()));

    // 같은 회차는 다시 나가지 않는다.
    h.clock.set(utc(2026, 11, 16, 0, 0, 30));
    trigger.tick().await.unwrap();
    assert_eq!(h.waker.count(), 1);
}

// 검증: F1, E18
#[tokio::test]
async fn 휴장일에는_깨우지_않는다() {
    let h = Harness::new(before_open());
    h.market.set_calendar(kr_calendar(
        kr_day(date(2026, 11, 13)),
        holiday(date(2026, 11, 16)),
        kr_day(date(2026, 11, 17)),
    ));
    let trigger = h.trigger(config(vec![open_job()], vec![]));
    trigger.tick().await.unwrap();
    h.clock.set(utc(2026, 11, 16, 0, 0, 5));
    trigger.tick().await.unwrap();
    h.clock.set(utc(2026, 11, 16, 3, 0, 0));
    trigger.tick().await.unwrap();
    assert_eq!(h.waker.count(), 0);
}

// 검증: E16
#[tokio::test]
async fn 깨우기_전에_작업을_먼저_옮겨_저장하고_영수증을_연다() {
    let h = Harness::new(before_open());
    h.waker.observe(h.store.clone());
    let trigger = h.trigger(config(vec![open_job()], vec![]));
    trigger.tick().await.unwrap();
    h.clock.set(open_slot());
    trigger.tick().await.unwrap();

    let snapshots = h.waker.snapshots();
    assert_eq!(snapshots.len(), 1);
    let seen = &snapshots[0];
    let job = seen
        .jobs
        .iter()
        .find(|job| job.id == JobId::new("open"))
        .unwrap();
    assert_eq!(job.last_slot, Some(open_slot()));
    let receipt: &Receipt = seen
        .receipts
        .iter()
        .find(|r| r.key.slot == open_slot())
        .unwrap();
    assert_eq!(receipt.status, ReceiptStatus::Running);
    assert_eq!(receipt.delivery, DeliveryState::Started);
}

// 검증: E16
#[tokio::test]
async fn tick마다_저장소에서_작업_상태를_다시_읽는다() {
    let h = Harness::new(before_open());
    let trigger = h.trigger(config(vec![open_job()], vec![]));
    trigger.tick().await.unwrap();
    let mut job = h.store.job("open").await;
    job.disabled = Some(DisableReason::Manual);
    h.store.inner_save(&job).await;

    h.clock.set(open_slot());
    trigger.tick().await.unwrap();
    assert_eq!(h.waker.count(), 0);
}

// 검증: E16
#[tokio::test]
async fn 같은_회차의_영수증이_이미_있으면_깨우지_않는다() {
    let h = Harness::new(before_open());
    let trigger = h.trigger(config(vec![open_job()], vec![]));
    trigger.tick().await.unwrap();
    h.store.inner.insert_receipt(Receipt {
        key: ReceiptKey {
            job: JobId::new("open"),
            slot: open_slot(),
        },
        status: ReceiptStatus::Ok,
        delivery: DeliveryState::Started,
        started_at: open_slot(),
        finished_at: Some(open_slot()),
    });
    h.clock.set(open_slot());
    trigger.tick().await.unwrap();
    assert_eq!(h.waker.count(), 0);
}

// 검증: E16
#[tokio::test]
async fn 같은_작업이_실행_중이면_새_회차를_깨우지_않는다() {
    let h = Harness::new(before_open());
    let trigger = h.trigger(config(vec![open_job()], vec![]));
    trigger.tick().await.unwrap();
    h.store.inner.insert_receipt(Receipt {
        key: ReceiptKey {
            job: JobId::new("open"),
            slot: utc(2026, 11, 13, 0, 0, 0),
        },
        status: ReceiptStatus::Running,
        delivery: DeliveryState::Started,
        started_at: utc(2026, 11, 13, 0, 0, 0),
        finished_at: None,
    });
    h.clock.set(open_slot());
    trigger.tick().await.unwrap();
    assert_eq!(h.waker.count(), 0);
}

// 검증: F1
#[tokio::test]
async fn 전체_정지_중에는_깨우지_않고_기한은_다음_tick에_남는다() {
    let h = Harness::new(before_open());
    let trigger = h.trigger(config(vec![open_job()], vec![]));
    trigger.tick().await.unwrap();
    h.stop.set(true);
    h.clock.set(open_slot());
    let report = trigger.tick().await.unwrap();
    assert_eq!(report.outcome, TickOutcome::Stopped);
    assert_eq!(h.waker.count(), 0);

    h.stop.set(false);
    h.clock.set(utc(2026, 11, 16, 0, 1, 0));
    trigger.tick().await.unwrap();
    assert_eq!(h.waker.count(), 1);
    match &h.waker.wakes()[0].events[0] {
        TriggerEvent::Schedule { slot, .. } => assert_eq!(*slot, open_slot()),
        other => panic!("일정 사건이어야 한다: {other:?}"),
    }
}

// 검증: E16
#[tokio::test]
async fn 저장소에_쓰지_못하면_깨우지_않고_오류다() {
    let h = Harness::new(before_open());
    let trigger = h.trigger(config(vec![open_job()], vec![]));
    trigger.tick().await.unwrap();
    h.store.fail_writes(true);
    h.clock.set(open_slot());
    let result = trigger.tick().await;
    assert!(matches!(result, Err(Error::Store(_))));
    assert_eq!(h.waker.count(), 0);

    h.store.fail_writes(false);
    h.clock.set(utc(2026, 11, 16, 0, 0, 5));
    trigger.tick().await.unwrap();
    assert_eq!(h.waker.count(), 1);
}

// 검증: F2
#[tokio::test]
async fn 같은_tick의_사건은_대상마다_한_번에_묶어_깨운다() {
    let h = Harness::new(before_open());
    let jobs = vec![
        schedule("open-a", ScheduleKind::MarketOpen, Target::Team),
        schedule("open-b", ScheduleKind::MarketOpen, Target::Team),
        schedule(
            "open-news",
            ScheduleKind::MarketOpen,
            Target::Member("news".to_string()),
        ),
    ];
    let trigger = h.trigger(config(jobs, vec![]));
    trigger.tick().await.unwrap();
    h.clock.set(open_slot());
    trigger.tick().await.unwrap();

    let wakes = h.waker.wakes();
    assert_eq!(wakes.len(), 2);
    let team = wakes.iter().find(|w| w.target == Target::Team).unwrap();
    let mut team_jobs: Vec<String> = team
        .events
        .iter()
        .map(|event| match event {
            TriggerEvent::Schedule { job, .. } => job.to_string(),
            other => panic!("일정 사건이어야 한다: {other:?}"),
        })
        .collect();
    team_jobs.sort();
    assert_eq!(team_jobs, vec!["open-a", "open-b"]);
    let news = wakes
        .iter()
        .find(|w| w.target == Target::Member("news".to_string()))
        .unwrap();
    assert_eq!(news.events.len(), 1);
    // 묶어도 영수증은 작업마다 하나다.
    for id in ["open-a", "open-b", "open-news"] {
        let receipt = h.store.receipt(id, open_slot()).await.unwrap();
        assert_eq!(receipt.status, ReceiptStatus::Ok);
    }
}

// 검증: E16
#[tokio::test]
async fn 다른_tick이_돌고_있으면_기다리지_않고_바쁨으로_끝난다() {
    let h = Harness::new(before_open());
    let trigger = Arc::new(h.trigger(config(vec![open_job()], vec![])));
    trigger.tick().await.unwrap();
    h.clock.set(open_slot());
    h.waker.hold(true);

    let running = {
        let trigger = trigger.clone();
        tokio::spawn(async move { trigger.tick().await })
    };
    h.waker.entered().await;
    let second = trigger.tick().await.unwrap();
    assert_eq!(second.outcome, TickOutcome::Busy);
    assert!(second.wakes.is_empty());

    h.waker.release();
    let first = running.await.unwrap().unwrap();
    assert_eq!(first.outcome, TickOutcome::Ran);
    assert_eq!(h.waker.count(), 1);
}

// 검증: F1
#[test]
fn 같은_작업_id가_두_번_있으면_만들지_않는다() {
    let h = Harness::new(before_open());
    let jobs = vec![
        open_job(),
        schedule("open", ScheduleKind::MarketClose, Target::Team),
    ];
    let result = Trigger::new(config(jobs, vec![]), h.ports());
    assert!(matches!(result, Err(Error::DuplicateJob(id)) if id == JobId::new("open")));

    let mixed = Trigger::new(
        config(
            vec![open_job()],
            vec![condition("open", "005930", ConditionKind::Above(d("1")))],
        ),
        h.ports(),
    );
    assert!(matches!(mixed, Err(Error::DuplicateJob(_))));
}

// 검증: F2
#[test]
fn 시장_어댑터가_없는_작업은_만들지_않는다() {
    let h = Harness::new(before_open());
    let us = ScheduleJob {
        id: JobId::new("us-open"),
        market: Market::Us,
        kind: ScheduleKind::MarketOpen,
        target: Target::Team,
    };
    let result = Trigger::new(config(vec![us], vec![]), h.ports());
    assert!(matches!(result, Err(Error::MissingMarket(id)) if id == JobId::new("us-open")));

    // 미국 어댑터를 더하면 만들어진다.
    let mut ports = h.ports();
    let us_adapter: Arc<dyn MarketAdapter> = Arc::new(FakeAdapter::new(Market::Us));
    ports.markets.push(us_adapter);
    let us = ScheduleJob {
        id: JobId::new("us-open"),
        market: Market::Us,
        kind: ScheduleKind::MarketOpen,
        target: Target::Team,
    };
    assert!(Trigger::new(config(vec![us], vec![]), ports).is_ok());
}

// 검증: F2
#[test]
fn 잘못된_시세_조건은_만들지_않는다() {
    let h = Harness::new(before_open());
    let bad = |kind: ConditionKind| ConditionJob {
        id: JobId::new("bad"),
        market: Market::Kr,
        symbol: Symbol::new("005930"),
        kind,
        target: Target::Team,
    };
    for kind in [
        ConditionKind::Change {
            reference: d("0"),
            percent: d("3"),
        },
        ConditionKind::Change {
            reference: d("100"),
            percent: d("0"),
        },
        ConditionKind::Change {
            reference: d("100"),
            percent: d("-1"),
        },
        ConditionKind::Above(d("0")),
        ConditionKind::Below(d("-5")),
    ] {
        let result = Trigger::new(config(vec![], vec![bad(kind.clone())]), h.ports());
        assert!(
            matches!(result, Err(Error::InvalidCondition(_))),
            "{kind:?}는 받지 않아야 한다"
        );
    }

    let mut negative = config(vec![], vec![]);
    negative.rearm_percent = d("-0.1");
    assert!(matches!(
        Trigger::new(negative, h.ports()),
        Err(Error::InvalidConfig(_))
    ));
}
