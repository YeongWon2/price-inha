//! 일정 시각 · 유예 · 오류 지연. 모두 순수 계산이다.

use std::time::Duration;

use chrono::{DateTime, Utc};
use pi_market::MarketCalendar;

use crate::ScheduleKind;

/// 일정 시각을 정규장에서 낼 때 쓰는 간격(설정 값).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScheduleTimes {
    /// 장 시작 전 브리핑이 정규장 시작보다 이만큼 앞선다.
    pub briefing_lead: Duration,
    /// 마감 뒤(복기 · 백업)가 정규장 끝보다 이만큼 뒤다.
    pub after_close_delay: Duration,
}

impl Default for ScheduleTimes {
    /// 브리핑 30분 전, 마감 뒤 30분.
    fn default() -> Self {
        Self {
            briefing_lead: Duration::from_secs(30 * 60),
            after_close_delay: Duration::from_secs(30 * 60),
        }
    }
}

/// 달력의 세 날(이전 영업일 · 기준일 · 다음 영업일)에서 그 일정의 회차 시각들(이른 순).
///
/// 정규장이 없는 날(휴장 · 정규장 없는 부분 휴장)은 회차가 없다.
pub fn schedule_slots(
    calendar: &MarketCalendar,
    kind: ScheduleKind,
    times: &ScheduleTimes,
) -> Vec<DateTime<Utc>> {
    let _ = (calendar, kind, times);
    todo!()
}

/// 유예: 주기의 절반을 120초~7200초로 자른 값(원본: Hermes).
///
/// 늦은 정도가 유예 이하면 그냥 지금 발사하고, 넘으면 놓친 것으로 다룬다.
pub fn grace_for(period: Duration) -> Duration {
    let _ = period;
    todo!()
}

/// 유예의 아래 끝(120초).
pub const MIN_GRACE: Duration = Duration::from_secs(120);

/// 유예의 위 끝(7200초).
pub const MAX_GRACE: Duration = Duration::from_secs(7200);

/// 오류 지연 표(원본: OpenClaw): 30초 · 60초 · 5분 · 15분 · 60분. 마지막 값에 머문다.
pub const ERROR_BACKOFF: [Duration; 5] = [
    Duration::from_secs(30),
    Duration::from_secs(60),
    Duration::from_secs(5 * 60),
    Duration::from_secs(15 * 60),
    Duration::from_secs(60 * 60),
];

/// 연달아 이만큼 실패하면 작업을 끄고 알림 요구를 낸다(10번).
pub const AUTO_DISABLE_AFTER: u32 = 10;

/// 연속 실패 수에 맞는 오류 지연. 0이면 지연 없음(0초), 1이면 30초, … 5 이상이면 60분.
pub fn error_backoff(consecutive_errors: u32) -> Duration {
    let _ = consecutive_errors;
    todo!()
}
