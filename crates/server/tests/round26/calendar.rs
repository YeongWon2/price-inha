//! 1번: 시연 달력(N19 · C33). 매일이 거래일이고 그 시장 시간대(국내 한국 시각, 미국 뉴욕 시각)의
//! 00:05 이상 23:55 미만이 정규장 하나다. 자정 앞뒤 10분은 장 밖이다.

use chrono::{FixedOffset, NaiveDate};
use pi_market::{Market, MarketCalendar, SessionKind, TradingDay};
use price_inha::demo_calendar;

use crate::common::{at_zone, kst, new_york_november, saturday, seoul, sunday, tuesday};

/// 그 날이 그 시간대의 00:05~23:55 정규장 하나인지 본다.
fn one_regular_day(day: &TradingDay, zone: FixedOffset, date: NaiveDate) {
    assert_eq!(day.date, date, "날짜가 맞다");
    assert_eq!(day.sessions.len(), 1, "{date}: 정규장 하나다: {day:?}");
    let session = &day.sessions[0];
    assert_eq!(session.kind, SessionKind::Regular, "{date}: 정규장이다");
    assert_eq!(
        session.start,
        at_zone(zone, date, 0, 5),
        "{date}: 시장 시간대 00:05에 연다"
    );
    assert_eq!(
        session.end,
        at_zone(zone, date, 23, 55),
        "{date}: 시장 시간대 23:55에 닫는다"
    );
}

/// 그 날의 달력이 매일 거래일 · 앞뒤 거래일은 하루 전 · 뒤인지 본다.
fn every_day_open(calendar: &MarketCalendar, market: Market, zone: FixedOffset, date: NaiveDate) {
    assert_eq!(calendar.market, market, "물은 시장의 달력이다");
    let before = date.pred_opt().unwrap();
    let after = date.succ_opt().unwrap();
    one_regular_day(&calendar.today, zone, date);
    one_regular_day(&calendar.previous_business_day, zone, before);
    one_regular_day(&calendar.next_business_day, zone, after);
}

// 검증: V54 N19 C33 D14
#[test]
fn 국내_시연_달력은_주말을_포함해_매일_한국_시각_00시_05분부터_23시_55분까지_정규장이다() {
    for date in [tuesday(), saturday(), sunday()] {
        let calendar = demo_calendar(Market::Kr, date);
        every_day_open(&calendar, Market::Kr, seoul(), date);
    }
}

// 검증: V54 N19 C33 D14
#[test]
fn 미국_시연_달력은_뉴욕_시각_00시_05분부터_23시_55분까지_정규장이다() {
    for date in [saturday(), sunday()] {
        let calendar = demo_calendar(Market::Us, date);
        every_day_open(&calendar, Market::Us, new_york_november(), date);
    }
}

// 검증: V54 N19 C33
#[test]
fn 시연_달력에서_자정_앞뒤_10분은_장_밖이고_그_밖은_장_안이다() {
    let calendar = demo_calendar(Market::Kr, saturday());
    let next = sunday();
    let closed = [
        kst(saturday(), 0, 0),
        kst(saturday(), 0, 4),
        kst(saturday(), 23, 55),
        kst(saturday(), 23, 59),
        kst(next, 0, 0),
        kst(next, 0, 4),
    ];
    for at in closed {
        assert!(
            calendar.session_at(at).is_none(),
            "자정 앞뒤 10분은 장 밖이다: {at}"
        );
    }
    let open = [
        kst(saturday(), 0, 5),
        kst(saturday(), 3, 0),
        kst(saturday(), 14, 0),
        kst(saturday(), 22, 30),
        kst(saturday(), 23, 54),
        kst(next, 0, 5),
    ];
    for at in open {
        assert!(
            calendar.session_at(at).is_some(),
            "주말 · 밤이어도 00:05~23:55는 장 안이다: {at}"
        );
    }
}
