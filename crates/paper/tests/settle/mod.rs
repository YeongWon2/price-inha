//! 결제일: 판 돈을 언제 다시 쓸 수 있는가.
//! 출처: docs/research/stock-investing-principles.md 5.1(국내 T+2) · 5.2(미국 T+1).
//! 판 돈을 결제 전에 다시 쓸 수 있는지는 확인하지 못해 설정으로 두고, 두 쪽을 다 본다.

use chrono::{DateTime, NaiveDate, Utc};
use pi_market::Currency;
use pi_paper::{BrokerError, ProceedsUsable};

use crate::support::{
    AAPL, Desk, SAM, at, buy, cash, config, d, day, kr_calendar, monday, sell, sym, us_buy,
    us_calendar, us_sell, utc,
};

/// 10주를 70,000에 사서 71,000에 다 판다. 판 돈 710,000 − 106(106.5 버림) − 1,420 = 708,474.
fn buy_then_sell_all(desk: &Desk, now: DateTime<Utc>) {
    desk.own(SAM, "10", "70000", now);
    desk.accepted(&sell(SAM, "10", "71000"), now);
    desk.quote(SAM, "71000", now);
}

// 검증: C10
#[test]
fn 국내에서_판_돈은_이틀_뒤_영업일에_쓸_수_있다() {
    let desk = Desk::standard();
    buy_then_sell_all(&desk, at(0));

    let fills = desk.broker.fills();
    assert_eq!(fills[1].settlement_date, day(2026, 11, 18));
    assert_eq!(desk.krw(), cash("9299895", "0", "708474"));
    assert_eq!(desk.buying_power(Currency::Krw), d("9299895"));

    // 결제 전인 돈으로는 살 수 없다: 9,300,000 + 수수료는 쓸 수 있는 돈을 넘는다.
    assert_eq!(
        desk.place(&buy(SAM, "133", "70000"), at(1)),
        Err(BrokerError::InsufficientBuyingPower)
    );

    // 화요일(T+1): 아직이다.
    desk.broker.set_calendar(kr_calendar(day(2026, 11, 17)));
    assert_eq!(desk.krw(), cash("9299895", "0", "708474"));

    // 수요일(T+2): 쓸 수 있는 돈으로 옮겨진다.
    desk.broker.set_calendar(kr_calendar(day(2026, 11, 18)));
    assert_eq!(desk.krw(), cash("10008369", "0", "0"));
    assert_eq!(desk.buying_power(Currency::Krw), d("10008369"));
}

// 검증: C10
#[test]
fn 결제일을_지나서_달력을_넣어도_풀린다() {
    let desk = Desk::standard();
    buy_then_sell_all(&desk, at(0));
    desk.broker.set_calendar(kr_calendar(day(2026, 11, 20)));
    assert_eq!(desk.krw(), cash("10008369", "0", "0"));
}

fn friday() -> NaiveDate {
    day(2026, 11, 13)
}

// 검증: C10
#[test]
fn 금요일에_체결하면_주말을_건너_화요일에_결제된다() {
    let desk = Desk::bare(config());
    desk.broker.set_calendar(kr_calendar(friday()));
    desk.broker.set_reference_price(sym(SAM), d("70000"));
    buy_then_sell_all(&desk, utc(friday(), 1, 0, 0));

    let fills = desk.broker.fills();
    assert_eq!(fills[0].settlement_date, day(2026, 11, 17), "산 체결");
    assert_eq!(fills[1].settlement_date, day(2026, 11, 17), "판 체결");

    desk.broker.set_calendar(kr_calendar(monday()));
    assert_eq!(desk.krw().unsettled, d("708474"));
    desk.broker.set_calendar(kr_calendar(day(2026, 11, 17)));
    assert_eq!(desk.krw(), cash("10008369", "0", "0"));
}

// 검증: C10
#[test]
fn 산_주식은_결제_전에도_바로_팔_수_있고_산_돈은_바로_나간다() {
    let desk = Desk::standard();
    desk.own(SAM, "10", "70000", at(0));
    assert_eq!(desk.krw(), cash("9299895", "0", "0"));
    assert_eq!(desk.sellable(SAM), d("10"));
    desk.place(&sell(SAM, "10", "71000"), at(1)).unwrap();
}

// 검증: C10
#[test]
fn 판_돈을_바로_쓰게_설정하면_체결되자마자_쓸_수_있다() {
    let mut config = config();
    config.kr.settlement.proceeds_usable = ProceedsUsable::Immediately;
    let desk = Desk::open(config);
    buy_then_sell_all(&desk, at(0));

    assert_eq!(desk.krw(), cash("10008369", "0", "0"));
    // 결제 예정일 자체는 그대로 적힌다.
    assert_eq!(desk.broker.fills()[1].settlement_date, day(2026, 11, 18));
}

// 검증: C10
#[test]
fn 결제까지의_영업일_수는_설정을_따른다() {
    let mut config = config();
    config.kr.settlement.business_days = 3;
    let desk = Desk::open(config);
    buy_then_sell_all(&desk, at(0));
    assert_eq!(desk.broker.fills()[1].settlement_date, day(2026, 11, 19));
    desk.broker.set_calendar(kr_calendar(day(2026, 11, 18)));
    assert_eq!(desk.krw().unsettled, d("708474"));
    desk.broker.set_calendar(kr_calendar(day(2026, 11, 19)));
    assert_eq!(desk.krw().unsettled, d("0"));
}

// 검증: C10
#[test]
fn 미국에서_판_돈은_다음_영업일에_쓸_수_있고_시장마다_따로_결제된다() {
    let desk = Desk::bare(config());
    desk.broker.set_calendar(us_calendar(friday()));
    let now = utc(friday(), 15, 0, 0);
    desk.accepted(&us_buy(AAPL, "10", "150.25"), now);
    desk.us_quote(AAPL, "150.25", now);
    desk.accepted(&us_sell(AAPL, "10", "151.00"), now);
    desk.us_quote(AAPL, "151.00", now);

    // 금요일 체결 → 월요일 결제(T+1).
    let fills = desk.broker.fills();
    assert_eq!(fills[1].settlement_date, monday());
    assert_eq!(desk.usd(), cash("8496.00", "0", "1508.34"));

    // 국내 달력이 월요일이 돼도 달러는 풀리지 않는다. 미국 달력이 월요일이 돼야 풀린다.
    desk.broker.set_calendar(kr_calendar(monday()));
    assert_eq!(desk.usd().unsettled, d("1508.34"));
    desk.broker.set_calendar(us_calendar(monday()));
    assert_eq!(desk.usd(), cash("10004.34", "0", "0"));
}
