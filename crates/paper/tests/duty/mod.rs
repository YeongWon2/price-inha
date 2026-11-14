//! 2차 13번: 조립하는 쪽의 의무를 드러낸다. 달력이 지금을 덮지 않거나 기준가가 오늘 것이 아니면
//! 조용히 돌지 않고, 이유가 "장 닫힘"과 구분된다.

use pi_paper::{BrokerError, OrderEventKind, OrderGroup, OrderStatus};

use crate::support::{
    Desk, HYN, SAM, at, buy, cash, config, d, day, kinds, kr_calendar, monday, sym, utc,
};

// 검증: C10
// 월요일 달력은 이전 영업일(금요일)의 첫 세션 시작부터 다음 영업일(화요일)의 마지막 세션 끝까지 덮는다.
// 테스트 달력: 프리마켓은 그 날 00:00 UTC의 1시간 전에 시작하고, 애프터마켓은 11:00 UTC에 끝난다.
#[test]
fn 달력이_지금을_덮지_않으면_주문을_받지_않고_장_닫힘과_구분된다() {
    let desk = Desk::standard();
    let order = buy(SAM, "1", "70000");

    // 목요일 장중 시각: 월요일 달력으로는 알 수 없는 날이다.
    assert_eq!(
        desk.place(&order, utc(day(2026, 11, 19), 1, 0, 0)),
        Err(BrokerError::CalendarStale)
    );
    // 월요일 밤: 달력이 덮는 시각이고 장이 닫혀 있다.
    assert_eq!(
        desk.place(&order, utc(monday(), 12, 0, 0)),
        Err(BrokerError::OrderHoursClosed)
    );

    // 뒤쪽 경계: 화요일 애프터마켓이 끝나는 시각부터는 덮지 않는다.
    let tuesday = day(2026, 11, 17);
    assert_eq!(
        desk.place(&order, utc(tuesday, 10, 59, 59)),
        Err(BrokerError::OrderHoursClosed),
        "덮는다. 애프터마켓은 이 설정이 받지 않는 세션이다"
    );
    assert_eq!(
        desk.place(&order, utc(tuesday, 11, 0, 0)),
        Err(BrokerError::CalendarStale)
    );

    // 앞쪽 경계: 금요일 프리마켓이 시작하는 시각(목요일 23:00 UTC)부터 덮는다.
    let thursday = day(2026, 11, 12);
    assert_eq!(
        desk.place(&order, utc(thursday, 22, 59, 59)),
        Err(BrokerError::CalendarStale)
    );
    assert_eq!(
        desk.place(&order, utc(thursday, 23, 0, 0)),
        Err(BrokerError::OrderHoursClosed)
    );

    desk.set_clock(at(0));
    assert!(desk.orders(OrderGroup::Open).is_empty());
    assert!(desk.orders(OrderGroup::Closed).is_empty());
    assert_eq!(desk.krw(), cash("10000000", "0", "0"));
}

// 검증: C10
#[test]
fn 달력이_지금을_덮지_않으면_체결하지_않는다() {
    let desk = Desk::standard();
    let order_id = desk.accepted(&buy(SAM, "1", "70000"), at(0));

    // 다음 주 목요일의 달력으로 바뀌었다(수요일~금요일만 덮는다). 지금은 여전히 월요일이다.
    desk.broker.set_calendar(kr_calendar(day(2026, 11, 26)));
    assert!(desk.quote(SAM, "70000", at(1)).is_empty());
    assert_eq!(desk.order(&order_id).status, OrderStatus::Pending);
    assert!(desk.broker.fills().is_empty());
    assert_eq!(
        desk.place(&buy(HYN, "1", "30000"), at(2)),
        Err(BrokerError::CalendarStale)
    );

    // 맞는 달력을 다시 넣으면 다음 시세에 체결된다.
    desk.broker.set_calendar(kr_calendar(monday()));
    assert_eq!(
        kinds(&desk.quote(SAM, "70000", at(3))),
        vec![OrderEventKind::Fill]
    );
}

// 검증: C10
#[test]
fn 달력이_지금을_덮지_않으면_정정도_받지_않는다() {
    let desk = Desk::standard();
    let order_id = desk.accepted(&buy(SAM, "1", "70000"), at(0));
    desk.broker.set_calendar(kr_calendar(day(2026, 11, 26)));
    assert_eq!(
        desk.modify(&order_id, Some("1"), Some("69900"), "1", at(1)),
        Err(BrokerError::CalendarStale)
    );
    assert_eq!(desk.order(&order_id).status, OrderStatus::Pending);
    assert_eq!(desk.orders(OrderGroup::Open).len(), 1);
}

// 검증: C10
#[test]
fn 기준가는_넣은_날짜가_오늘일_때만_쓴다() {
    let desk = Desk::bare(config());
    desk.broker.set_calendar(kr_calendar(monday()));

    // 지난 금요일 날짜로 넣은 기준가뿐이다: 오늘(월요일) 것이 아니라 모르는 것이다.
    desk.broker
        .set_reference_price(sym(SAM), d("70000"), day(2026, 11, 13));
    assert_eq!(
        desk.place(&buy(SAM, "1", "70000"), at(0)),
        Err(BrokerError::ReferencePriceUnknown)
    );
    // 내일 날짜로 미리 넣은 것도 오늘 것이 아니다.
    desk.broker
        .set_reference_price(sym(SAM), d("70000"), day(2026, 11, 17));
    assert_eq!(
        desk.place(&buy(SAM, "1", "70000"), at(1)),
        Err(BrokerError::ReferencePriceUnknown)
    );

    desk.broker
        .set_reference_price(sym(SAM), d("70000"), monday());
    desk.accepted(&buy(SAM, "1", "70000"), at(2));
}

// 검증: C10
#[test]
fn 날이_바뀌면_어제_넣은_기준가는_모르는_것이_된다() {
    let desk = Desk::standard();
    let tuesday = day(2026, 11, 17);
    let now = |seconds: i64| utc(tuesday, 1, 0, 0) + chrono::Duration::seconds(seconds);
    desk.broker.set_calendar(kr_calendar(tuesday));

    // 기준가는 월요일 것뿐이다.
    assert_eq!(
        desk.place(&buy(SAM, "1", "70000"), now(0)),
        Err(BrokerError::ReferencePriceUnknown)
    );

    // 화요일 기준가 71,000을 넣는다. 상한가 71,000 × 1.3 = 92,300.
    // (월요일 기준가 70,000이었다면 상한가는 91,000이라 92,300은 받지 않았을 값이다.)
    desk.broker
        .set_reference_price(sym(SAM), d("71000"), tuesday);
    desk.own(SAM, "1", "71000", now(1));
    desk.accepted(&crate::support::sell(SAM, "1", "92300"), now(3));
    assert_eq!(
        desk.place(&crate::support::sell(SAM, "1", "92400"), now(4)),
        Err(BrokerError::PriceOutOfRange)
    );
}
