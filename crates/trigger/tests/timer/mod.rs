//! 60초 상한 타이머, 벽시계 · 단조 시계 비교(시계가 뒤로 감 · 절전에서 깸), 되풀이.

use pi_trigger::{
    CLOCK_JUMP_TOLERANCE, ConditionKind, DeliveryState, JobId, MAX_TIMER, MIN_REFIRE_GAP, Receipt,
    ReceiptKey, ReceiptStatus, ScheduleKind, Target,
};
use tokio::sync::watch;

use crate::support::{Harness, condition, config, d, quote, schedule, secs, utc};

fn open_job() -> pi_trigger::ScheduleJob {
    schedule("open", ScheduleKind::MarketOpen, Target::Team)
}

// 검증: E16
#[tokio::test]
async fn 타이머는_다음_회차까지이고_길어야_60초다() {
    assert_eq!(MAX_TIMER, secs(60));
    assert_eq!(MIN_REFIRE_GAP, secs(2));
    assert_eq!(CLOCK_JUMP_TOLERANCE, secs(2));

    let h = Harness::new(utc(2026, 11, 15, 23, 50, 0));
    let trigger = h.trigger(config(vec![open_job()], vec![]));
    let far = trigger.tick().await.unwrap();
    assert_eq!(far.next_delay, secs(60));

    h.clock.set(utc(2026, 11, 15, 23, 59, 40));
    let near = trigger.tick().await.unwrap();
    assert_eq!(near.next_delay, secs(20));

    h.clock.set(utc(2026, 11, 16, 0, 0, 0));
    let fired = trigger.tick().await.unwrap();
    assert_eq!(fired.wakes.len(), 1);
    assert_eq!(fired.next_delay, secs(60));
}

// 검증: E16
#[tokio::test]
async fn 기한이_지났는데_발사하지_못했으면_2초_뒤에_다시_본다() {
    let h = Harness::new(utc(2026, 11, 15, 23, 59, 0));
    let trigger = h.trigger(config(vec![open_job()], vec![]));
    trigger.tick().await.unwrap();
    h.stop.set(true);
    h.clock.set(utc(2026, 11, 16, 0, 0, 0));
    let report = trigger.tick().await.unwrap();
    assert_eq!(report.next_delay, secs(2));
}

// 검증: N6
#[tokio::test]
async fn 시세_조건이_있고_장이_열렸으면_시세_간격마다_본다() {
    let h = Harness::new(utc(2026, 11, 16, 1, 0, 0));
    h.market
        .set_quote(quote("005930", "50", utc(2026, 11, 16, 1, 0, 0)));
    let trigger = h.trigger(config(
        vec![],
        vec![condition("c", "005930", ConditionKind::Above(d("100")))],
    ));
    let open = trigger.tick().await.unwrap();
    assert_eq!(open.next_delay, secs(10));

    h.clock.set(utc(2026, 11, 16, 8, 0, 0));
    let closed = trigger.tick().await.unwrap();
    assert_eq!(closed.next_delay, secs(60));
}

// 검증: E16
#[tokio::test]
async fn 시계가_뒤로_가도_같은_회차를_두_번_발사하지_않는다() {
    let h = Harness::new(utc(2026, 11, 15, 23, 59, 0));
    let trigger = h.trigger(config(vec![open_job()], vec![]));
    let first = trigger.tick().await.unwrap();
    assert!(!first.clock_jumped);
    h.clock.set(utc(2026, 11, 16, 0, 0, 0));
    let fired = trigger.tick().await.unwrap();
    assert!(!fired.clock_jumped);
    assert_eq!(h.waker.count(), 1);

    // 벽시계가 30초 뒤로 갔다(단조 시계는 30초 앞으로).
    h.clock.jump_wall(utc(2026, 11, 15, 23, 59, 30), secs(30));
    let back = trigger.tick().await.unwrap();
    assert!(back.clock_jumped);
    assert_eq!(h.waker.count(), 1);

    // 다시 회차 시각을 지나도 이미 처리한 회차다.
    h.clock.jump_wall(utc(2026, 11, 16, 0, 0, 10), secs(40));
    let again = trigger.tick().await.unwrap();
    assert!(!again.clock_jumped);
    assert_eq!(h.waker.count(), 1);
}

// 검증: E16
#[tokio::test]
async fn 절전에서_깨어_벽시계만_뛰면_알아차리고_놓친_회차를_다룬다() {
    let h = Harness::new(utc(2026, 11, 15, 22, 0, 0));
    let trigger = h.trigger(config(vec![open_job()], vec![]));
    trigger.tick().await.unwrap();

    // 노트북이 잠들었다가 09:30 KST에 깼다. 단조 시계는 60초만 흘렀다.
    h.clock.jump_wall(utc(2026, 11, 16, 0, 30, 0), secs(60));
    let woke = trigger.tick().await.unwrap();
    assert!(woke.clock_jumped);
    assert_eq!(h.waker.count(), 1, "유예(2시간) 안이라 지금 발사한다");

    h.clock.advance(secs(60));
    let normal = trigger.tick().await.unwrap();
    assert!(!normal.clock_jumped);
    assert_eq!(h.waker.count(), 1);
}

// 검증: E16
#[tokio::test]
async fn 되풀이는_다시_켠_뒤_처리부터_하고_시계로_기다리며_멈추라면_끝난다() {
    let h = Harness::new(utc(2026, 11, 15, 23, 59, 0));
    let old_slot = utc(2026, 11, 13, 0, 0, 0);
    h.store.inner.insert_receipt(Receipt {
        key: ReceiptKey {
            job: JobId::new("open"),
            slot: old_slot,
        },
        status: ReceiptStatus::Running,
        delivery: DeliveryState::Started,
        started_at: old_slot,
        finished_at: None,
    });
    let trigger = h.trigger(config(vec![open_job()], vec![]));
    let (stop_tx, stop_rx) = watch::channel(false);
    h.clock.stop_after(3, stop_tx);

    trigger.run(stop_rx).await.unwrap();

    let sleeps = h.clock.sleeps();
    assert_eq!(sleeps.len(), 3);
    assert!(sleeps.iter().all(|s| *s <= secs(60) && *s > secs(0)));
    assert_eq!(
        h.store.receipt("open", old_slot).await.unwrap().status,
        ReceiptStatus::Interrupted
    );
    // 23:59 → (60초) → 00:00에 장 시작을 발사한다.
    assert_eq!(h.waker.count(), 1);
    assert_eq!(
        h.store
            .receipt("open", utc(2026, 11, 16, 0, 0, 0))
            .await
            .unwrap()
            .status,
        ReceiptStatus::Ok
    );
}
