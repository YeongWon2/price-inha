//! 시간. 테스트가 시각과 기다림을 정할 수 있게 trait으로 둔다.

use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, Utc};

/// 시계. 토큰 만료 판단 · 시세를 받은 시각 · 429 뒤의 기다림이 모두 이것을 쓴다.
#[async_trait]
pub trait Clock: Send + Sync {
    /// 지금 시각.
    fn now(&self) -> DateTime<Utc>;
    /// 그만큼 기다린다. 토스 어댑터는 `Retry-After` 기다림을 여기로만 한다.
    async fn sleep(&self, duration: Duration);
}

/// 실제 시계. `now`는 시스템 시각, `sleep`은 `tokio::time::sleep`.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

#[async_trait]
impl Clock for SystemClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }

    async fn sleep(&self, duration: Duration) {
        tokio::time::sleep(duration).await;
    }
}
