//! 시험이 정하는 시계와 한국 시각 도우미(round26의 것을 본떴다).

use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, FixedOffset, NaiveDate, TimeZone, Utc};
use pi_market::Clock;

/// 시험이 정하는 시계. 자는 것은 짧게만 잔다.
pub struct TestClock(AtomicI64);

impl TestClock {
    pub fn at(time: DateTime<Utc>) -> Arc<Self> {
        Arc::new(Self(AtomicI64::new(time.timestamp_millis())))
    }

    pub fn set(&self, time: DateTime<Utc>) {
        self.0.store(time.timestamp_millis(), Ordering::SeqCst);
    }
}

#[async_trait]
impl Clock for TestClock {
    fn now(&self) -> DateTime<Utc> {
        Utc.timestamp_millis_opt(self.0.load(Ordering::SeqCst))
            .unwrap()
    }

    async fn sleep(&self, duration: Duration) {
        tokio::time::sleep(duration.min(Duration::from_millis(10))).await;
    }
}

/// 2026-11-17 화요일.
pub fn tuesday() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 11, 17).unwrap()
}

/// 그 날 · 한국 시각(시 · 분 · 초)의 UTC 시각.
pub fn kst(date: NaiveDate, hour: u32, minute: u32, second: u32) -> DateTime<Utc> {
    let zone = FixedOffset::east_opt(9 * 3600).unwrap();
    zone.from_local_datetime(&date.and_hms_opt(hour, minute, second).unwrap())
        .unwrap()
        .with_timezone(&Utc)
}

/// 그 시각에서 `seconds`초 뒤.
pub fn plus(at: DateTime<Utc>, seconds: i64) -> DateTime<Utc> {
    at + chrono::Duration::seconds(seconds)
}
