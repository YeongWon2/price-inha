//! 시세 조건 · 다시 걸기(결정 21) · 늦은 시세(N6).

use pi_market::{Market, MarketError, Symbol};
use pi_trigger::{
    ConditionKind, ConditionMemory, Direction, JobId, ReceiptStatus, Target, TriggerEvent,
    WakeReason,
};

use crate::support::{Harness, condition, config, d, quote, secs, utc};

fn above_100() -> pi_trigger::ConditionJob {
    condition("samsung-100", "005930", ConditionKind::Above(d("100")))
}

fn in_session() -> chrono::DateTime<chrono::Utc> {
    utc(2026, 11, 16, 1, 0, 0)
}

/// 지금 받은 시세를 정하고 tick 한 번.
async fn tick_with_price(h: &Harness, trigger: &pi_trigger::Trigger, price: &str) {
    h.market.set_quote(quote("005930", price, h.clock_now()));
    trigger.tick().await.unwrap();
}

trait Now {
    fn clock_now(&self) -> chrono::DateTime<chrono::Utc>;
}

impl Now for Harness {
    fn clock_now(&self) -> chrono::DateTime<chrono::Utc> {
        use pi_market::Clock;
        self.clock.now()
    }
}

// 검증: F2, E18
#[tokio::test]
async fn 장이_닫혀_있으면_시세를_받지_않고_깨우지_않는다() {
    let h = Harness::new(utc(2026, 11, 16, 8, 0, 0));
    // 아침에 받은 낡은 시세라도 장이 닫혔으면 늦은 것으로 보지 않는다.
    h.market
        .set_quote(quote("005930", "150", utc(2026, 11, 16, 6, 0, 0)));
    let trigger = h.trigger(config(vec![], vec![above_100()]));
    trigger.tick().await.unwrap();
    h.clock.advance(secs(60));
    trigger.tick().await.unwrap();
    assert_eq!(h.market.quote_calls(), 0);
    assert_eq!(h.waker.count(), 0);
}

// 검증: F2
#[tokio::test]
async fn 조건에_닿으면_담당을_깨우고_계속_맞아도_다시_깨우지_않는다() {
    let h = Harness::new(in_session());
    let trigger = h.trigger(config(vec![], vec![above_100()]));
    tick_with_price(&h, &trigger, "100").await;

    let wakes = h.waker.wakes();
    assert_eq!(wakes.len(), 1);
    assert_eq!(wakes[0].target, Target::Member("price".to_string()));
    assert_eq!(wakes[0].reason, WakeReason::Price);
    assert_eq!(
        wakes[0].events,
        vec![TriggerEvent::Price {
            job: JobId::new("samsung-100"),
            symbol: Symbol::new("005930"),
            condition: ConditionKind::Above(d("100")),
            direction: Direction::Up,
            price: d("100"),
            at: in_session(),
        }]
    );
    let receipt = h.store.receipt("samsung-100", in_session()).await.unwrap();
    assert_eq!(receipt.status, ReceiptStatus::Ok);
    assert_eq!(
        h.store.job("samsung-100").await.condition,
        Some(ConditionMemory::Fired {
            notified: d("100"),
            direction: Direction::Up,
        })
    );

    h.clock.advance(secs(10));
    tick_with_price(&h, &trigger, "101").await;
    h.clock.advance(secs(10));
    tick_with_price(&h, &trigger, "100").await;
    assert_eq!(h.waker.count(), 1);
}

// 검증: F2
#[tokio::test]
async fn 알린_값에서_반대로_0_5퍼센트_벗어났다가_돌아오면_다시_깨운다() {
    let h = Harness::new(in_session());
    let trigger = h.trigger(config(vec![], vec![above_100()]));
    tick_with_price(&h, &trigger, "100").await;
    assert_eq!(h.waker.count(), 1);

    for price in ["99.6", "100.2"] {
        h.clock.advance(secs(10));
        tick_with_price(&h, &trigger, price).await;
    }
    assert_eq!(h.waker.count(), 1, "0.4%만 벗어났으면 다시 걸리지 않는다");

    h.clock.advance(secs(10));
    tick_with_price(&h, &trigger, "99.5").await;
    assert_eq!(h.waker.count(), 1, "다시 걸 때는 깨우지 않는다");
    assert_eq!(
        h.store.job("samsung-100").await.condition,
        Some(ConditionMemory::Armed)
    );

    h.clock.advance(secs(10));
    tick_with_price(&h, &trigger, "100").await;
    assert_eq!(h.waker.count(), 2);
}

// 검증: F2
#[tokio::test]
async fn 깨우기가_실패하면_다시_걸기_상태를_저장하지_않고_물러선_뒤_다시_깨운다() {
    let h = Harness::new(in_session());
    let trigger = h.trigger(config(vec![], vec![above_100()]));
    h.waker.fail(true);
    tick_with_price(&h, &trigger, "100").await;
    assert_eq!(h.waker.count(), 1);
    let job = h.store.job("samsung-100").await;
    assert!(matches!(job.condition, None | Some(ConditionMemory::Armed)));
    assert_eq!(job.consecutive_errors, 1);
    assert_eq!(
        job.retry_at,
        Some(in_session() + chrono::Duration::seconds(30))
    );
    let receipt = h.store.receipt("samsung-100", in_session()).await.unwrap();
    assert_eq!(receipt.status, ReceiptStatus::Error);

    h.waker.fail(false);
    h.clock.advance(secs(10));
    tick_with_price(&h, &trigger, "100").await;
    assert_eq!(h.waker.count(), 1, "물러선 동안은 깨우지 않는다");

    h.clock.advance(secs(20));
    tick_with_price(&h, &trigger, "100").await;
    assert_eq!(h.waker.count(), 2);
    let job = h.store.job("samsung-100").await;
    assert_eq!(job.consecutive_errors, 0);
    assert_eq!(job.retry_at, None);
    assert!(matches!(job.condition, Some(ConditionMemory::Fired { .. })));
}

// 검증: N6, E14
#[tokio::test]
async fn 시세가_30초_넘게_늦으면_팀을_이상으로_한_번_깨운다() {
    let h = Harness::new(in_session());
    let trigger = h.trigger(config(vec![], vec![above_100()]));
    // 딱 30초는 아직 늦지 않았다.
    h.market.set_quote(quote(
        "005930",
        "50",
        in_session() - chrono::Duration::seconds(30),
    ));
    trigger.tick().await.unwrap();
    assert_eq!(h.waker.count(), 0);

    h.clock.advance(secs(1));
    trigger.tick().await.unwrap();
    let wakes = h.waker.wakes();
    assert_eq!(wakes.len(), 1);
    assert_eq!(wakes[0].target, Target::Team);
    assert_eq!(wakes[0].reason, WakeReason::Anomaly);
    assert_eq!(
        wakes[0].events,
        vec![TriggerEvent::StaleQuote {
            market: Market::Kr,
            symbol: Symbol::new("005930"),
            last_received: Some(in_session() - chrono::Duration::seconds(30)),
        }]
    );

    // 늦은 동안은 다시 깨우지 않는다.
    h.clock.advance(secs(10));
    trigger.tick().await.unwrap();
    assert_eq!(h.waker.count(), 1);

    // 새 시세가 오면 풀리고, 다시 늦으면 다시 깨운다.
    h.clock.advance(secs(10));
    tick_with_price(&h, &trigger, "50").await;
    assert_eq!(h.waker.count(), 1);
    h.clock.advance(secs(31));
    trigger.tick().await.unwrap();
    assert_eq!(h.waker.count(), 2);
    assert_eq!(h.waker.wakes()[1].reason, WakeReason::Anomaly);
}

// 검증: N6, E14
#[tokio::test]
async fn 시세를_한_번도_받지_못한_채_30초가_넘으면_이상으로_깨운다() {
    let h = Harness::new(in_session());
    h.market.fail_quotes(Some(MarketError::Connection {
        detail: "연결 안 됨".to_string(),
    }));
    let trigger = h.trigger(config(vec![], vec![above_100()]));
    trigger.tick().await.unwrap();
    h.clock.advance(secs(30));
    trigger.tick().await.unwrap();
    assert_eq!(h.waker.count(), 0);

    h.clock.advance(secs(1));
    trigger.tick().await.unwrap();
    let wakes = h.waker.wakes();
    assert_eq!(wakes.len(), 1);
    assert_eq!(wakes[0].reason, WakeReason::Anomaly);
    assert_eq!(
        wakes[0].events,
        vec![TriggerEvent::StaleQuote {
            market: Market::Kr,
            symbol: Symbol::new("005930"),
            last_received: None,
        }]
    );
}
