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
        Self {
            max_age: DEFAULT_STALE_AFTER,
        }
    }
}

/// 시세가 늦었는가.
///
/// `now`에 장이 열려 있고(`calendar.session_at(now)`가 세션을 준다 — 프리 · 애프터 ·
/// 데이마켓도 열린 것으로 본다) `now - quote.received_at`이 `policy.max_age`를 넘으면 참.
/// 장이 닫힌 동안은 늦은 것으로 보지 않는다. 기준은 거래소 시각이 아니라 받은 시각이다.
///
/// 모르면 멈추는 쪽으로 본다(2차):
/// - 달력이 `now`를 덮지 않으면([`MarketCalendar::covers`]가 거짓) 늦은 것이다
///   ("장 닫힘"이 아니라 "달력이 낡음").
/// - 받은 시각이 `now`보다 뒤면(시계가 어긋남) 장이 열렸든 닫혔든 늦은 것이다.
pub fn is_stale(
    quote: &Quote,
    calendar: &MarketCalendar,
    now: DateTime<Utc>,
    policy: &StalePolicy,
) -> bool {
    if calendar.session_at(now).is_none() {
        return false;
    }
    // 받은 시각이 지금보다 뒤면(시계가 어긋남) 나이를 잴 수 없으니 늦었다고 하지 않는다.
    match now.signed_duration_since(quote.received_at).to_std() {
        Ok(age) => age > policy.max_age,
        Err(_) => false,
    }
}
