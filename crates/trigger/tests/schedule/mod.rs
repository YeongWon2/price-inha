//! 일정 시각 · 유예 · 오류 지연 표.

use std::time::Duration;

use pi_market::{Market, MarketCalendar, Session, SessionKind, TradingDay};
use pi_trigger::{
    AUTO_DISABLE_AFTER, ERROR_BACKOFF, MAX_GRACE, MIN_GRACE, ScheduleKind, ScheduleTimes,
    error_backoff, grace_for, schedule_slots,
};

use crate::support::{date, holiday, kr_calendar, kr_day, kr_week, secs, utc};

fn times() -> ScheduleTimes {
    ScheduleTimes {
        briefing_lead: Duration::from_secs(30 * 60),
        after_close_delay: Duration::from_secs(45 * 60),
    }
}

// 검증: F1
#[test]
fn 장_시작은_세_날의_정규장_시작_시각이다() {
    let slots = schedule_slots(&kr_week(), ScheduleKind::MarketOpen, &times());
    assert_eq!(
        slots,
        vec![
            utc(2026, 11, 13, 0, 0, 0),
            utc(2026, 11, 16, 0, 0, 0),
            utc(2026, 11, 17, 0, 0, 0),
        ]
    );
}

// 검증: F1
#[test]
fn 브리핑_마감_마감뒤는_정규장에서_간격만큼_옮긴_시각이다() {
    let calendar = kr_calendar(
        kr_day(date(2026, 11, 13)),
        kr_day(date(2026, 11, 16)),
        kr_day(date(2026, 11, 17)),
    );
    let briefing = schedule_slots(&calendar, ScheduleKind::PreOpenBriefing, &times());
    assert_eq!(briefing[1], utc(2026, 11, 15, 23, 30, 0));
    let close = schedule_slots(&calendar, ScheduleKind::MarketClose, &times());
    assert_eq!(close[1], utc(2026, 11, 16, 6, 30, 0));
    let after = schedule_slots(&calendar, ScheduleKind::AfterClose, &times());
    assert_eq!(after[1], utc(2026, 11, 16, 7, 15, 0));
    assert_eq!(after.len(), 3);
}

// 검증: F1, E18
#[test]
fn 휴장일에는_회차가_없다() {
    let calendar = kr_calendar(
        kr_day(date(2026, 11, 13)),
        holiday(date(2026, 11, 16)),
        kr_day(date(2026, 11, 17)),
    );
    let slots = schedule_slots(&calendar, ScheduleKind::MarketOpen, &times());
    assert_eq!(
        slots,
        vec![utc(2026, 11, 13, 0, 0, 0), utc(2026, 11, 17, 0, 0, 0)]
    );
}

// 검증: F1
#[test]
fn 정규장이_없는_부분_휴장일에도_회차가_없다() {
    let pre_only = TradingDay {
        date: date(2026, 11, 16),
        sessions: vec![Session {
            kind: SessionKind::PreMarket,
            start: utc(2026, 11, 15, 23, 0, 0),
            end: utc(2026, 11, 16, 0, 0, 0),
        }],
    };
    let calendar = kr_calendar(
        kr_day(date(2026, 11, 13)),
        pre_only,
        kr_day(date(2026, 11, 17)),
    );
    let slots = schedule_slots(&calendar, ScheduleKind::MarketClose, &times());
    assert_eq!(
        slots,
        vec![utc(2026, 11, 13, 6, 30, 0), utc(2026, 11, 17, 6, 30, 0)]
    );
}

// 검증: F1
#[test]
fn 미국_장_시작은_달력이_준_utc_시각_그대로다() {
    // 2026-11-16은 서머타임이 끝난 뒤(EST, UTC-5): 09:30 ET = 14:30 UTC.
    let us_day = |y, m, dd| TradingDay {
        date: date(y, m, dd),
        sessions: vec![Session {
            kind: SessionKind::Regular,
            start: utc(y, m, dd, 14, 30, 0),
            end: utc(y, m, dd, 21, 0, 0),
        }],
    };
    let calendar = MarketCalendar {
        market: Market::Us,
        today: us_day(2026, 11, 16),
        previous_business_day: us_day(2026, 11, 13),
        next_business_day: us_day(2026, 11, 17),
    };
    let open = schedule_slots(&calendar, ScheduleKind::MarketOpen, &times());
    assert_eq!(open[1], utc(2026, 11, 16, 14, 30, 0));
    let briefing = schedule_slots(&calendar, ScheduleKind::PreOpenBriefing, &times());
    assert_eq!(briefing[1], utc(2026, 11, 16, 14, 0, 0));
}

// 검증: E16
#[test]
fn 유예는_주기_절반을_120초에서_7200초로_자른_값이다() {
    assert_eq!(MIN_GRACE, secs(120));
    assert_eq!(MAX_GRACE, secs(7200));
    assert_eq!(grace_for(secs(60)), secs(120));
    assert_eq!(grace_for(Duration::ZERO), secs(120));
    assert_eq!(grace_for(secs(3600)), secs(1800));
    assert_eq!(grace_for(secs(24 * 3600)), secs(7200));
    assert_eq!(grace_for(secs(240)), secs(120));
    assert_eq!(grace_for(secs(242)), secs(121));
}

// 검증: E16
#[test]
fn 오류_지연은_30초_60초_5분_15분_60분이고_마지막에_머문다() {
    assert_eq!(
        ERROR_BACKOFF,
        [secs(30), secs(60), secs(300), secs(900), secs(3600)]
    );
    assert_eq!(error_backoff(0), Duration::ZERO);
    assert_eq!(error_backoff(1), secs(30));
    assert_eq!(error_backoff(2), secs(60));
    assert_eq!(error_backoff(3), secs(300));
    assert_eq!(error_backoff(4), secs(900));
    assert_eq!(error_backoff(5), secs(3600));
    assert_eq!(error_backoff(9), secs(3600));
    assert_eq!(error_backoff(u32::MAX), secs(3600));
    assert_eq!(AUTO_DISABLE_AFTER, 10);
}
