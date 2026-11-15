//! 일정 작업 하나의 기한 판정과 다음에 볼 시각. 모두 순수 계산이다.

use std::time::Duration;

use chrono::{DateTime, Datelike, NaiveDate, Utc, Weekday};
use pi_market::{Market, MarketCalendar, SessionKind};

use crate::{JobState, ScheduleJob, ScheduleTimes, TriggerEvent, grace_for, schedule_slots};

/// 일정의 주기(하루). 유예는 이것의 절반을 자른 값이다(2시간).
const DAY: Duration = Duration::from_secs(24 * 60 * 60);

/// 일정 작업 하나를 이번 tick에 어떻게 할지.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SchedulePlan {
    /// 넘길 사건들(없으면 발사하지 않는다).
    pub events: Vec<TriggerEvent>,
    /// 영수증 키의 예정 시각(발사할 때만).
    pub receipt_slot: Option<DateTime<Utc>>,
    /// 옮긴 뒤의 작업 상태.
    pub state: JobState,
    /// 상태가 바뀌었는가(저장해야 하는가).
    pub changed: bool,
}

/// 그 시각에 정규장이 열려 있는가.
pub(crate) fn regular_open(calendar: &MarketCalendar, now: DateTime<Utc>) -> bool {
    calendar
        .session_at(now)
        .is_some_and(|session| session.kind == SessionKind::Regular)
}

/// 기한 판정(원본: Hermes tick 4단계, 장 밖은 결정 20).
///
/// - 꺼졌거나 물러선 동안이면 아무것도 하지 않는다(기한은 남는다).
/// - `last_slot`보다 뒤이고 지금 이하인 회차가 기한이다. 가장 늦은 회차가 유예 안이거나 정규장이
///   열려 있으면 그것 하나를 발사하고 앞의 것은 `missed`로 센다. 아니면 `pending_catch_up`에 적는다.
/// - 미뤄 둔 것이 있고 정규장이 열려 있으면 [`TriggerEvent::CatchUp`] 하나로 함께 넘긴다.
pub(crate) fn plan_schedule(
    job: &ScheduleJob,
    state: &JobState,
    calendar: &MarketCalendar,
    times: &ScheduleTimes,
    now: DateTime<Utc>,
) -> SchedulePlan {
    let mut plan = SchedulePlan {
        events: Vec::new(),
        receipt_slot: None,
        state: state.clone(),
        changed: false,
    };
    if state.disabled.is_some() || state.retry_at.is_some_and(|retry| now < retry) {
        return plan;
    }
    let Some(last) = state.last_slot else {
        plan.state.last_slot = Some(now);
        plan.changed = true;
        return plan;
    };
    let due: Vec<DateTime<Utc>> = schedule_slots(calendar, job.kind, times)
        .into_iter()
        .filter(|slot| *slot > last && *slot <= now)
        .collect();
    let open = regular_open(calendar, now);
    let mut schedule_event = None;
    if let Some(&latest) = due.last() {
        let late = now
            .signed_duration_since(latest)
            .to_std()
            .unwrap_or_default();
        plan.state.last_slot = Some(latest);
        plan.changed = true;
        if late <= grace_for(DAY) || open {
            schedule_event = Some(TriggerEvent::Schedule {
                job: job.id.clone(),
                kind: job.kind,
                market: job.market,
                slot: latest,
                missed: u32::try_from(due.len() - 1).unwrap_or(u32::MAX),
            });
            plan.receipt_slot = Some(latest);
        } else {
            plan.state.pending_catch_up.extend(due);
            plan.state.pending_catch_up.sort();
            plan.state.pending_catch_up.dedup();
        }
    }
    if open && !plan.state.pending_catch_up.is_empty() {
        let slots = std::mem::take(&mut plan.state.pending_catch_up);
        plan.receipt_slot = plan.receipt_slot.max(slots.last().copied());
        plan.events.push(TriggerEvent::CatchUp {
            job: job.id.clone(),
            kind: job.kind,
            market: job.market,
            slots,
        });
        plan.changed = true;
    }
    plan.events.extend(schedule_event);
    plan
}

/// 이 일정 작업을 다음에 볼 시각. 볼 것이 없으면(꺼짐 · 달력에 회차 없음) `None`.
///
/// 다음 회차 · 미뤄 둔 것을 넘길 다음 정규장 시작 가운데 이른 것이고, 물러선 동안이면 `retry_at`까지 미룬다.
/// 지난 시각일 수 있다(아직 처리하지 못한 기한).
pub(crate) fn schedule_wait(
    job: &ScheduleJob,
    state: &JobState,
    calendar: &MarketCalendar,
    times: &ScheduleTimes,
    now: DateTime<Utc>,
) -> Option<DateTime<Utc>> {
    if state.disabled.is_some() {
        return None;
    }
    let last = state.last_slot.unwrap_or(now);
    let next_slot = schedule_slots(calendar, job.kind, times)
        .into_iter()
        .find(|slot| *slot > last);
    let release = if state.pending_catch_up.is_empty() {
        None
    } else if regular_open(calendar, now) {
        Some(now)
    } else {
        next_regular_start(calendar, now)
    };
    let earliest = [next_slot, release].into_iter().flatten().min()?;
    match state.retry_at {
        Some(retry) if retry > earliest => Some(retry),
        _ => Some(earliest),
    }
}

/// 지금보다 뒤에 시작하는 첫 정규장 시작.
fn next_regular_start(calendar: &MarketCalendar, now: DateTime<Utc>) -> Option<DateTime<Utc>> {
    [
        &calendar.previous_business_day,
        &calendar.today,
        &calendar.next_business_day,
    ]
    .into_iter()
    .filter_map(|day| day.session(SessionKind::Regular))
    .map(|session| session.start)
    .filter(|start| *start > now)
    .min()
}

/// 지금이 그 시장의 몇월 며칠인가(달력을 받을 기준일). 국내는 KST(UTC+9), 미국은 미 동부 시각.
pub(crate) fn market_date(market: Market, now: DateTime<Utc>) -> NaiveDate {
    let hours = match market {
        Market::Kr => 9,
        Market::Us => {
            if us_daylight_saving(now) {
                -4
            } else {
                -5
            }
        }
        _ => 0,
    };
    (now + chrono::Duration::hours(hours)).date_naive()
}

/// 미 동부가 서머타임인가: 3월 둘째 일요일 02:00 EST(07:00 UTC)부터 11월 첫째 일요일 02:00 EDT(06:00 UTC) 전까지.
fn us_daylight_saving(now: DateTime<Utc>) -> bool {
    let year = now.year();
    let start = NaiveDate::from_weekday_of_month_opt(year, 3, Weekday::Sun, 2)
        .and_then(|day| day.and_hms_opt(7, 0, 0));
    let end = NaiveDate::from_weekday_of_month_opt(year, 11, Weekday::Sun, 1)
        .and_then(|day| day.and_hms_opt(6, 0, 0));
    match (start, end) {
        (Some(start), Some(end)) => {
            let at = now.naive_utc();
            start <= at && at < end
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn at(y: i32, m: u32, d: u32, h: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(y, m, d, h, 0, 0)
            .single()
            .unwrap_or_default()
    }

    #[test]
    fn 시장_날짜는_현지_날짜다() {
        // 2026-11-15 23:00 UTC = 11-16 08:00 KST
        assert_eq!(
            market_date(Market::Kr, at(2026, 11, 15, 23)),
            NaiveDate::from_ymd_opt(2026, 11, 16).unwrap_or_default()
        );
        // 2026-11-16 03:00 UTC = 11-15 22:00 EST
        assert_eq!(
            market_date(Market::Us, at(2026, 11, 16, 3)),
            NaiveDate::from_ymd_opt(2026, 11, 15).unwrap_or_default()
        );
        // 2026-07-01 03:30 UTC → EDT(UTC-4) 6-30 23시
        assert_eq!(
            market_date(Market::Us, at(2026, 7, 1, 3)),
            NaiveDate::from_ymd_opt(2026, 6, 30).unwrap_or_default()
        );
        assert_eq!(
            market_date(Market::Us, at(2026, 7, 1, 4)),
            NaiveDate::from_ymd_opt(2026, 7, 1).unwrap_or_default()
        );
    }
}
