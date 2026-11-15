//! 실패하면 물러서고, 10번이면 끄고 알림 요구를 낸다.

use pi_market::Clock;
use pi_trigger::{
    Alert, AlertReason, ConditionKind, DisableReason, JobId, ReceiptStatus, ScheduleKind, Target,
};

use crate::support::{
    Harness, condition, config, d, kr_next_week_day, quote, schedule, secs, settled_tick, utc,
};

// 검증: E16, F2
#[tokio::test]
async fn 연달아_실패하면_30초_60초_5분_15분_60분으로_물러서고_10번이면_끈다() {
    let start = utc(2026, 11, 16, 1, 0, 0);
    let h = Harness::new(start);
    let job = condition("samsung-100", "005930", ConditionKind::Above(d("100")));
    let trigger = h.trigger(config(vec![], vec![job]));
    h.waker.fail(true);

    let mut delays = Vec::new();
    let mut last_alerts = Vec::new();
    for attempt in 1..=10u32 {
        let now = h.clock.now();
        h.market.set_quote(quote("005930", "100", now));
        let report = settled_tick(&trigger).await;
        assert_eq!(h.waker.count(), attempt as usize, "{attempt}번째 시도");
        let state = h.store.job("samsung-100").await;
        assert_eq!(state.consecutive_errors, attempt);
        if attempt < 10 {
            assert!(report.alerts.is_empty());
            assert_eq!(state.disabled, None);
            let retry_at = state.retry_at.unwrap();
            delays.push((retry_at - now).to_std().unwrap());
            // 물러선 동안 한 번 더 보아도 깨우지 않는다.
            h.clock.advance(secs(5));
            h.market.set_quote(quote("005930", "100", h.clock.now()));
            settled_tick(&trigger).await;
            assert_eq!(h.waker.count(), attempt as usize);
            h.clock.set(retry_at);
        } else {
            last_alerts = report.alerts.clone();
            assert_eq!(state.disabled, Some(DisableReason::ConsecutiveFailures));
        }
    }
    assert_eq!(
        delays,
        vec![
            secs(30),
            secs(60),
            secs(300),
            secs(900),
            secs(3600),
            secs(3600),
            secs(3600),
            secs(3600),
            secs(3600),
        ]
    );
    assert_eq!(
        last_alerts,
        vec![Alert {
            job: JobId::new("samsung-100"),
            reason: AlertReason::AutoDisabled {
                consecutive_errors: 10,
            },
        }]
    );

    // 꺼진 작업은 성공할 수 있어도 깨우지 않는다.
    h.waker.fail(false);
    h.clock.advance(secs(60));
    h.market.set_quote(quote("005930", "100", h.clock.now()));
    let report = settled_tick(&trigger).await;
    assert_eq!(h.waker.count(), 10);
    assert!(report.alerts.is_empty(), "알림 요구는 끌 때 한 번만 낸다");
}

// 검증: E16
#[tokio::test]
async fn 일정이_실패하면_같은_회차를_다시_돌리지_않고_다음_회차에_성공하면_처음으로() {
    let open_slot = utc(2026, 11, 16, 0, 0, 0);
    let h = Harness::new(utc(2026, 11, 15, 23, 59, 0));
    let trigger = h.trigger(config(
        vec![schedule("open", ScheduleKind::MarketOpen, Target::Team)],
        vec![],
    ));
    settled_tick(&trigger).await;
    h.waker.fail(true);
    h.clock.set(open_slot);
    settled_tick(&trigger).await;
    assert_eq!(h.waker.count(), 1);
    let state = h.store.job("open").await;
    assert_eq!(state.consecutive_errors, 1);
    assert_eq!(state.last_slot, Some(open_slot));
    assert_eq!(
        h.store.receipt("open", open_slot).await.unwrap().status,
        ReceiptStatus::Error
    );

    h.waker.fail(false);
    for at in [utc(2026, 11, 16, 0, 0, 30), utc(2026, 11, 16, 0, 1, 30)] {
        h.clock.set(at);
        settled_tick(&trigger).await;
    }
    assert_eq!(h.waker.count(), 1);

    h.market.set_calendar(kr_next_week_day());
    h.clock.set(utc(2026, 11, 17, 0, 0, 0));
    settled_tick(&trigger).await;
    assert_eq!(h.waker.count(), 2);
    let state = h.store.job("open").await;
    assert_eq!(state.consecutive_errors, 0);
    assert_eq!(state.retry_at, None);
}
