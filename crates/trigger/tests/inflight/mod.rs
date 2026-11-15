//! 2차 1 · 2: 저장소 쓰기가 중간에 실패해도 회차를 잃지 않는다. 깨우기는 띄워 두고 tick 잠금을 바로 놓는다.
//!
//! 오래 걸리는 깨우기는 가짜 대상이 붙잡아 둔다. tokio 시간을 멈춰 두고(`start_paused`) 기다림에 상한을 걸어,
//! tick이 깨우기를 기다리면 실제로 잠들지 않고 바로 실패한다.

use std::collections::BTreeSet;
use std::future::Future;
use std::time::Duration;

use pi_market::{Clock, Market, Symbol};
use pi_trigger::{
    ConditionJob, ConditionKind, ConditionMemory, DeliveryState, Error, Finished, JobId, MAX_TIMER,
    ReceiptStatus, ScheduleKind, Target, TickOutcome, TickReport, Trigger, TriggerEvent,
    WakeReason,
};

use crate::support::{
    Harness, condition, config, d, kr_next_week_day, quote, schedule, secs, settled_tick, utc,
};

fn before_open() -> chrono::DateTime<chrono::Utc> {
    utc(2026, 11, 15, 23, 59, 0)
}

fn open_slot() -> chrono::DateTime<chrono::Utc> {
    utc(2026, 11, 16, 0, 0, 0)
}

/// 기다림의 상한(tokio 시간이 멈춰 있어 실제로는 잠들지 않는다).
const LIMIT: Duration = Duration::from_secs(5);

/// 깨우기를 기다리지 않고 tick이 끝나야 한다.
async fn quick_tick(trigger: &Trigger) -> Result<TickReport, Error> {
    tokio::time::timeout(LIMIT, trigger.tick())
        .await
        .expect("tick이 깨운 실행이 끝나기를 기다렸다(잠금을 쥐고 있다)")
}

/// 조건이 참이 될 때까지 다른 작업에 차례를 넘긴다.
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

/// 깨운 사건들에 나온 작업 id들.
fn woken_jobs(h: &Harness) -> BTreeSet<String> {
    h.waker
        .wakes()
        .iter()
        .flat_map(|wake| wake.events.iter())
        .filter_map(|event| match event {
            TriggerEvent::Schedule { job, .. } | TriggerEvent::Price { job, .. } => {
                Some(job.to_string())
            }
            _ => None,
        })
        .collect()
}

fn three_opens() -> Vec<pi_trigger::ScheduleJob> {
    vec![
        schedule("open-a", ScheduleKind::MarketOpen, Target::Team),
        schedule("open-b", ScheduleKind::MarketOpen, Target::Team),
        schedule(
            "open-news",
            ScheduleKind::MarketOpen,
            Target::Member("news".to_string()),
        ),
    ]
}

// 검증: E16
#[tokio::test]
async fn 두번째_단위_쓰기부터_실패하면_앞_작업은_깨우고_실패한_작업만_빠져_회차를_잃지_않는다() {
    let h = Harness::new(before_open());
    let trigger = h.trigger(config(three_opens(), vec![]));
    settled_tick(&trigger).await;
    let mut before = Vec::new();
    for id in ["open-a", "open-b", "open-news"] {
        before.push((id, h.store.job(id).await));
    }

    h.store.fail_units_from(2);
    h.clock.set(open_slot());
    let result = quick_tick(&trigger).await;
    assert!(
        matches!(result, Err(Error::Store(_))),
        "빠진 작업이 있으면 저장소 오류를 알린다"
    );
    trigger.settle().await.unwrap();

    assert_eq!(h.store.units(), 3, "작업마다 한 단위로 한 번씩 써 본다");
    let woken = woken_jobs(&h);
    assert_eq!(woken.len(), 1, "첫 단위만 적혔다: {woken:?}");
    for (id, saved) in &before {
        let now = h.store.job(id).await;
        let receipt = h.store.receipt(id, open_slot()).await;
        if woken.contains(*id) {
            assert_eq!(receipt.unwrap().status, ReceiptStatus::Ok);
            assert_eq!(now.last_slot, Some(open_slot()));
        } else {
            assert!(receipt.is_none(), "{id}: 실패한 작업은 영수증이 없다");
            assert_eq!(&now, saved, "{id}: 실패한 작업의 상태는 그대로다");
        }
    }
    assert!(h.store.running().await.is_empty());

    // 저장소가 돌아오면 빠졌던 작업이 같은 회차로 나간다.
    h.store.fail_units_from(0);
    h.clock.set(utc(2026, 11, 16, 0, 0, 5));
    settled_tick(&trigger).await;
    assert_eq!(woken_jobs(&h).len(), 3);
    for id in ["open-a", "open-b", "open-news"] {
        let receipt = h.store.receipt(id, open_slot()).await.unwrap();
        assert_eq!(receipt.status, ReceiptStatus::Ok, "{id}");
    }
    let slots: Vec<_> = h
        .waker
        .wakes()
        .iter()
        .flat_map(|wake| wake.events.clone())
        .filter_map(|event| match event {
            TriggerEvent::Schedule { slot, .. } => Some(slot),
            _ => None,
        })
        .collect();
    assert_eq!(slots.len(), 3, "작업마다 한 번만 나갔다");
    assert!(slots.iter().all(|slot| *slot == open_slot()));
    assert!(h.store.running().await.is_empty());
}

// 검증: E16
#[tokio::test]
async fn 연_영수증을_깨우지_못했으면_실패로_닫아_작업이_막히지_않는다() {
    let h = Harness::new(before_open());
    let trigger = h.trigger(config(
        vec![schedule("open", ScheduleKind::MarketOpen, Target::Team)],
        vec![],
    ));
    settled_tick(&trigger).await;

    h.store.fail_delivery_marks(true);
    h.clock.set(open_slot());
    let _ = quick_tick(&trigger).await;
    trigger.settle().await.unwrap();
    assert!(
        h.store.running().await.is_empty(),
        "실행 중으로 남은 영수증이 없어야 한다"
    );
    if h.waker.count() == 0 {
        let receipt = h.store.receipt("open", open_slot()).await.unwrap();
        assert_eq!(receipt.status, ReceiptStatus::Error);
        assert!(receipt.finished_at.is_some());
    }

    // 다음 회차는 막히지 않는다.
    h.store.fail_delivery_marks(false);
    let count = h.waker.count();
    h.market.set_calendar(kr_next_week_day());
    h.clock.set(utc(2026, 11, 17, 0, 0, 0));
    settled_tick(&trigger).await;
    assert_eq!(h.waker.count(), count + 1);
    let next = h
        .store
        .receipt("open", utc(2026, 11, 17, 0, 0, 0))
        .await
        .unwrap();
    assert_eq!(next.status, ReceiptStatus::Ok);
}

// 검증: N6, E14, F2
#[tokio::test(start_paused = true)]
async fn 오래_걸리는_깨우기가_도는_동안에도_늦은_시세는_30초_넘으면_이상으로_깨우고_시세_조건도_계속된다()
 {
    let t0 = utc(2026, 11, 16, 1, 0, 0);
    let h = Harness::new(t0);
    let hynix = ConditionJob {
        target: Target::Member("other".to_string()),
        ..condition("hynix-100", "000660", ConditionKind::Above(d("100")))
    };
    let samsung = condition("samsung-100", "005930", ConditionKind::Above(d("100")));
    let trigger = h.trigger(config(vec![], vec![samsung, hynix]));
    h.waker.hold(true);
    h.waker
        .hold_target(Some(Target::Member("price".to_string())));

    // 삼성이 조건에 닿아 담당 price를 깨운다. 그 실행은 오래 걸린다.
    h.market.set_quote(quote("005930", "100", t0));
    h.market.set_quote(quote("000660", "50", t0));
    let first = quick_tick(&trigger).await.unwrap();
    assert_eq!(first.outcome, TickOutcome::Ran);
    assert_eq!(first.wakes.len(), 1);
    assert_eq!(first.wakes[0].target, Target::Member("price".to_string()));
    tokio::time::timeout(LIMIT, h.waker.entered())
        .await
        .expect("깨우기가 띄워져야 한다");
    assert_eq!(trigger.in_flight(), 1);
    let running = h.store.receipt("samsung-100", t0).await.unwrap();
    assert_eq!(running.status, ReceiptStatus::Running);
    assert_eq!(running.delivery, DeliveryState::Started);

    // 10초 뒤: 다른 대상의 시세 조건은 그대로 깨운다. 시계 확인도 시세 간격으로 계속된다.
    h.clock.advance(secs(10));
    let t1 = h.clock.now();
    h.market.set_quote(quote("000660", "150", t1));
    let second = quick_tick(&trigger).await.unwrap();
    assert_eq!(second.outcome, TickOutcome::Ran, "바쁨이 아니어야 한다");
    assert_eq!(second.next_delay, secs(10));
    assert!(second.next_delay <= MAX_TIMER);
    assert_eq!(second.wakes.len(), 1);
    assert_eq!(second.wakes[0].target, Target::Member("other".to_string()));
    until(
        "다른 대상의 깨우기가 끝나 영수증이 닫힌다",
        || async {
            h.store
                .receipt("hynix-100", t1)
                .await
                .is_some_and(|receipt| receipt.status == ReceiptStatus::Ok)
        },
    )
    .await;

    // 삼성 시세가 31초째 오지 않는다: 오래 걸리는 깨우기가 끝나지 않았어도 팀을 이상으로 깨운다.
    h.clock.set(t0 + chrono::Duration::seconds(31));
    let t2 = h.clock.now();
    h.market.set_quote(quote("000660", "150", t2));
    let third = quick_tick(&trigger).await.unwrap();
    assert_eq!(third.outcome, TickOutcome::Ran);
    let anomaly = third
        .wakes
        .iter()
        .find(|wake| wake.target == Target::Team)
        .expect("늦은 시세로 팀을 깨워야 한다");
    assert_eq!(anomaly.reason, WakeReason::Anomaly);
    assert_eq!(
        anomaly.events,
        vec![TriggerEvent::StaleQuote {
            market: Market::Kr,
            symbol: Symbol::new("005930"),
            last_received: Some(t0),
        }]
    );
    until("팀 깨우기가 대상에 닿는다", || async {
        h.waker
            .wakes()
            .iter()
            .any(|wake| wake.target == Target::Team)
    })
    .await;
    assert_eq!(
        h.store.receipt("samsung-100", t0).await.unwrap().status,
        ReceiptStatus::Running,
        "오래 걸리는 깨우기는 아직 돈다"
    );
    let price_wakes = h
        .waker
        .wakes()
        .iter()
        .filter(|wake| wake.target == Target::Member("price".to_string()))
        .count();
    assert_eq!(price_wakes, 1, "실행 중 영수증이 같은 작업의 중복을 막는다");

    // 끝나면 영수증을 닫고 다시 걸기 상태를 적는다.
    h.waker.release();
    let settled = tokio::time::timeout(LIMIT, trigger.settle())
        .await
        .expect("풀어 준 깨우기는 끝나야 한다")
        .unwrap();
    assert_eq!(settled.finished.len(), 3);
    assert!(settled.finished.iter().all(|finished| finished.ok));
    assert_eq!(trigger.in_flight(), 0);
    assert_eq!(
        h.store.receipt("samsung-100", t0).await.unwrap().status,
        ReceiptStatus::Ok
    );
    assert_eq!(
        h.store.job("samsung-100").await.condition,
        Some(ConditionMemory::Fired {
            notified: d("100"),
            direction: pi_trigger::Direction::Up,
        })
    );
}

// 검증: E16, F2
#[tokio::test(start_paused = true)]
async fn 오래_걸리는_깨우기가_도는_동안에도_tick은_돌고_다른_대상은_깨우고_같은_회차는_겹치지_않는다()
 {
    let h = Harness::new(before_open());
    let trigger = h.trigger(config(
        vec![
            schedule("open", ScheduleKind::MarketOpen, Target::Team),
            schedule(
                "open-news",
                ScheduleKind::MarketOpen,
                Target::Member("news".to_string()),
            ),
        ],
        vec![],
    ));
    quick_tick(&trigger).await.unwrap();
    h.waker.hold(true);
    h.waker.hold_target(Some(Target::Team));

    h.clock.set(open_slot());
    let fired = quick_tick(&trigger).await.unwrap();
    assert_eq!(fired.wakes.len(), 2);
    tokio::time::timeout(LIMIT, h.waker.entered())
        .await
        .expect("팀 깨우기가 띄워져야 한다");
    until(
        "뉴스 담당 깨우기는 팀을 기다리지 않고 끝난다",
        || async {
            h.store
                .receipt("open-news", open_slot())
                .await
                .is_some_and(|receipt| receipt.status == ReceiptStatus::Ok)
        },
    )
    .await;
    let team = h.store.receipt("open", open_slot()).await.unwrap();
    assert_eq!(team.status, ReceiptStatus::Running);
    assert_eq!(team.delivery, DeliveryState::Started);
    assert_eq!(trigger.in_flight(), 1);

    // 팀 실행이 도는 동안 30초 뒤 tick: 바쁨이 아니고, 같은 회차를 다시 보내지 않는다.
    h.clock.advance(secs(30));
    let during = quick_tick(&trigger).await.unwrap();
    assert_eq!(during.outcome, TickOutcome::Ran);
    assert!(during.wakes.is_empty());
    assert!(during.next_delay <= MAX_TIMER);
    assert_eq!(h.waker.count(), 2);

    h.waker.release();
    tokio::time::timeout(LIMIT, trigger.settle())
        .await
        .expect("풀어 준 깨우기는 끝나야 한다")
        .unwrap();
    assert_eq!(trigger.in_flight(), 0);
    let team = h.store.receipt("open", open_slot()).await.unwrap();
    assert_eq!(team.status, ReceiptStatus::Ok);
    assert!(team.finished_at.is_some());
    assert_eq!(
        h.store.job("open").await.last_slot,
        Some(open_slot()),
        "상태는 깨우기 전에 이미 옮겼다"
    );
}

// 검증: E16
#[tokio::test]
async fn 끝난_깨우기의_결과는_settle이_한_번만_낸다() {
    let h = Harness::new(before_open());
    let trigger = h.trigger(config(
        vec![schedule("open", ScheduleKind::MarketOpen, Target::Team)],
        vec![],
    ));
    settled_tick(&trigger).await;
    h.waker.fail(true);
    h.clock.set(open_slot());
    let report = quick_tick(&trigger).await.unwrap();
    assert_eq!(report.wakes.len(), 1);

    let first = trigger.settle().await.unwrap();
    assert_eq!(
        first.finished,
        vec![Finished {
            wake: report.wakes[0].clone(),
            ok: false,
        }]
    );
    assert!(first.alerts.is_empty());
    let second = trigger.settle().await.unwrap();
    assert!(second.finished.is_empty());
    assert!(second.alerts.is_empty());

    let receipt = h.store.receipt("open", open_slot()).await.unwrap();
    assert_eq!(receipt.status, ReceiptStatus::Error);
    let state = h.store.job("open").await;
    assert_eq!(state.consecutive_errors, 1);
    assert_eq!(state.id, JobId::new("open"));
}
