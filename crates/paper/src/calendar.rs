//! 달력을 읽는 판단: 달력이 그 시각을 덮는가, 그 시각이 어느 영업일인가, 결제 예정일은 언제인가.

use chrono::{DateTime, Datelike, NaiveDate, Utc, Weekday};
use pi_market::{MarketCalendar, TradingDay};

/// 달력의 세 날(이전 영업일 · 기준일 · 다음 영업일)을 날짜 순서로.
fn days(calendar: &MarketCalendar) -> [&TradingDay; 3] {
    [
        &calendar.previous_business_day,
        &calendar.today,
        &calendar.next_business_day,
    ]
}

/// 달력이 그 시각을 덮는가: `이전 영업일의 첫 세션 시작 <= at < 다음 영업일의 마지막 세션 끝`.
///
/// 세션이 없는 날(휴장으로 적힌 날)이 끼어 있으면 세션이 있는 가장 앞 · 가장 뒤의 날로 잰다.
/// 세션이 하나도 없는 달력은 어느 시각도 덮지 않는다.
pub(crate) fn covers(calendar: &MarketCalendar, at: DateTime<Utc>) -> bool {
    let sessions = || {
        days(calendar)
            .into_iter()
            .flat_map(|day| day.sessions.iter())
    };
    let start = sessions().map(|session| session.start).min();
    let end = sessions().map(|session| session.end).max();
    match (start, end) {
        (Some(start), Some(end)) => start <= at && at < end,
        _ => false,
    }
}

/// 그 시각의 영업일: 달력에서 `now`가 든 세션의 날. 열린 세션이 없으면 `None`.
pub(crate) fn trade_date(calendar: &MarketCalendar, now: DateTime<Utc>) -> Option<NaiveDate> {
    days(calendar)
        .into_iter()
        .find(|day| {
            day.sessions
                .iter()
                .any(|session| session.start <= now && now < session.end)
        })
        .map(|day| day.date)
}

/// 결제 예정일 = 체결한 영업일 + `business_days` 영업일. 영업일은 평일(월~금)로만 센다.
pub(crate) fn settlement_date(
    calendar: &MarketCalendar,
    now: DateTime<Utc>,
    business_days: u32,
) -> Option<NaiveDate> {
    let mut date = trade_date(calendar, now)?;
    let mut left = business_days;
    while left > 0 {
        date = date.succ_opt()?;
        if !matches!(date.weekday(), Weekday::Sat | Weekday::Sun) {
            left -= 1;
        }
    }
    Some(date)
}

#[cfg(test)]
mod tests {
    use super::{covers, settlement_date, trade_date};
    use chrono::{DateTime, NaiveDate, TimeZone, Utc};
    use pi_market::{Market, MarketCalendar, Session, SessionKind, TradingDay};

    fn date(day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 11, day).unwrap()
    }

    fn at(day: u32, hour: u32) -> DateTime<Utc> {
        Utc.from_utc_datetime(&date(day).and_hms_opt(hour, 0, 0).unwrap())
    }

    /// 그 날 01시~06시에 정규장이 열리는 날. `open`이 거짓이면 휴장이다.
    fn trading_day(day: u32, open: bool) -> TradingDay {
        let sessions = if open {
            vec![Session {
                kind: SessionKind::Regular,
                start: at(day, 1),
                end: at(day, 6),
            }]
        } else {
            Vec::new()
        };
        TradingDay {
            date: date(day),
            sessions,
        }
    }

    /// 금(13일) · 월(16일) · 화(17일).
    fn calendar(previous_open: bool) -> MarketCalendar {
        MarketCalendar {
            market: Market::Kr,
            today: trading_day(16, true),
            previous_business_day: trading_day(13, previous_open),
            next_business_day: trading_day(17, true),
        }
    }

    #[test]
    fn 달력은_이전_영업일의_시작부터_다음_영업일의_끝_전까지_덮는다() {
        let calendar = calendar(true);
        assert!(!covers(&calendar, at(13, 0)));
        assert!(covers(&calendar, at(13, 1)));
        assert!(
            covers(&calendar, at(15, 12)),
            "주말도 덮는다(장은 닫혀 있다)"
        );
        assert!(covers(&calendar, at(17, 5)));
        assert!(!covers(&calendar, at(17, 6)), "끝 시각은 덮지 않는다");
    }

    #[test]
    fn 세션이_없는_날은_건너뛰고_세션이_하나도_없으면_덮지_않는다() {
        let calendar = calendar(false);
        assert!(!covers(&calendar, at(13, 1)));
        assert!(covers(&calendar, at(16, 1)));
        let mut empty = calendar;
        empty.today = trading_day(16, false);
        empty.next_business_day = trading_day(17, false);
        assert!(!covers(&empty, at(16, 1)));
    }

    #[test]
    fn 영업일과_결제_예정일은_지금이_든_세션의_날에서_센다() {
        let calendar = calendar(true);
        assert_eq!(trade_date(&calendar, at(16, 2)), Some(date(16)));
        assert_eq!(trade_date(&calendar, at(16, 12)), None, "닫힌 시각");
        // 금요일 체결 + 2영업일 = 화요일(주말을 건너뛴다).
        assert_eq!(settlement_date(&calendar, at(13, 2), 2), Some(date(17)));
        assert_eq!(settlement_date(&calendar, at(16, 2), 0), Some(date(16)));
    }
}
