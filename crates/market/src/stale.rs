//! 늦은 시세 (N6).

use std::time::Duration;

use chrono::{DateTime, Utc};

use crate::{MarketCalendar, Quote};

/// 늦은 시세 기준의 기본값: 30초.
pub const DEFAULT_STALE_AFTER: Duration = Duration::from_secs(30);

/// 늦은 시세 기준(설정값).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StalePolicy {
    /// 받은 지 이 시간을 **넘으면** 늦은 것이다(딱 이 시간이면 아직 아니다).
    pub max_age: Duration,
}

impl Default for StalePolicy {
    /// [`DEFAULT_STALE_AFTER`](30초).
    fn default() -> Self {
        todo!()
    }
}

/// 시세가 늦었는가.
///
/// `now`에 장이 열려 있고(`calendar.session_at(now)`가 세션을 준다 — 프리 · 애프터 ·
/// 데이마켓도 열린 것으로 본다) `now - quote.received_at`이 `policy.max_age`를 넘으면 참.
/// 장이 닫힌 동안은 늦은 것으로 보지 않는다. 기준은 거래소 시각이 아니라 받은 시각이다.
pub fn is_stale(
    quote: &Quote,
    calendar: &MarketCalendar,
    now: DateTime<Utc>,
    policy: &StalePolicy,
) -> bool {
    todo!()
}
