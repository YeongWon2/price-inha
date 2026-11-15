//! 시계. 알게 된 시각은 이것으로만 낸다(ARCH-CLOCK-2). 실제 시계는 조립하는 곳이 넣는다.

use chrono::{DateTime, Utc};

/// 시계.
pub trait Clock: Send + Sync {
    /// 지금 시각.
    fn now(&self) -> DateTime<Utc>;
}
