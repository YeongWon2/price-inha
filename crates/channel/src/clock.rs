//! 시계 포트. 이 모듈은 스스로 지금 시각을 읽지 않는다(ARCH-CLOCK-2). 조립하는 곳이 넣는다.

use chrono::{DateTime, Utc};

/// 시계. 판정([`crate::Gatekeeper::judge`]) · 승인에 넘길 `now`를 부르는 쪽이 이것으로 얻는다.
pub trait Clock: Send + Sync {
    /// 지금 시각.
    fn now(&self) -> DateTime<Utc>;
}
