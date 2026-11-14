//! 주문을 받을 때의 확인: 살 돈 · 팔 수량 · 장 시간 · 호가 단위 · 상하한가 · 모양, 그리고 묶기.
//! 기대값은 손으로 셈했다(수수료 0.015%, 묶는 돈의 수수료 몫은 원 단위로 올림).

use pi_market::{Currency, SessionKind};
use pi_paper::{
    BrokerError, InvalidReason, OrderGroup, OrderStatus, OrderType, Side, UnfilledTimeout,
};

use crate::support::{
    AAPL, Desk, HYN, SAM, at, buy, cash, config, config_with_krw, d, kr_calendar, market_buy,
    market_sell, monday, sell, sym, us_at, us_buy, utc,
};

// 검증: C10
#[test]
fn 지정가로_사면_값과_수수료만큼_돈이_묶인다() {
    let desk = Desk::standard();
    // 7주 × 70,300 = 492,100. 수수료 492,100 × 0.00015 = 73.815 → 올려서 74. 묶는 돈 492,174.
    let ack = desk.place(&buy(SAM, "7", "70300"), at(0)).unwrap();
    assert_eq!(ack.client_order_id, None);

    assert_eq!(desk.krw(), cash("9507826", "492174", "0"));
    assert_eq!(desk.buying_power(Currency::Krw), d("9507826"));
    assert_eq!(desk.usd(), cash("10000", "0", "0"));

    let order = desk.order(&ack.order_id);
    assert_eq!(order.order_id, ack.order_id);
    assert_eq!(order.symbol, sym(SAM));
    assert_eq!(order.side, Side::Buy);
    assert_eq!(order.order_type, OrderType::Limit);
    assert_eq!(order.status, OrderStatus::Pending);
    assert_eq!(order.price, Some(d("70300")));
    assert_eq!(order.quantity, d("7"));
    assert_eq!(order.currency, Currency::Krw);
    assert_eq!(order.ordered_at, at(0));
    assert_eq!(order.canceled_at, None);
    assert_eq!(order.close_reason, None);
    assert_eq!(order.execution.filled_quantity, d("0"));
    assert_eq!(order.execution.average_filled_price, None);
    assert_eq!(order.execution.filled_amount, d("0"));
    assert_eq!(order.execution.commission, d("0"));
    assert_eq!(order.execution.tax, d("0"));
    assert_eq!(order.execution.filled_at, None);
    assert_eq!(order.execution.settlement_date, None);

    assert_eq!(desk.orders(OrderGroup::Open), vec![order]);
    assert!(
        desk.holdings().is_empty(),
        "접수만으로는 보유가 생기지 않는다"
    );
}

// 검증: C10
#[test]
fn 살_돈이_수수료까지_넣어_1원이라도_모자라면_받지_않는다() {
    // 10주 × 70,000 = 700,000, 수수료 105 → 700,105가 있어야 한다.
    let short = Desk::open(config_with_krw("700104"));
    assert_eq!(
        short.place(&buy(SAM, "10", "70000"), at(0)),
        Err(BrokerError::InsufficientBuyingPower)
    );
    assert_eq!(short.krw(), cash("700104", "0", "0"));
    assert!(short.orders(OrderGroup::Open).is_empty());
    assert!(short.orders(OrderGroup::Closed).is_empty());

    let exact = Desk::open(config_with_krw("700105"));
    exact.place(&buy(SAM, "10", "70000"), at(0)).unwrap();
    assert_eq!(exact.krw(), cash("0", "700105", "0"));
}

// 검증: C10
#[test]
fn 이미_묶인_돈은_다음_주문에_쓸_수_없다() {
    let desk = Desk::open(config_with_krw("1000000"));
    desk.place(&buy(SAM, "10", "70000"), at(0)).unwrap();
    // 남은 돈 299,895. 하이닉스 10주 × 30,000 = 300,000 + 45 는 모자란다.
    assert_eq!(
        desk.place(&buy(HYN, "10", "30000"), at(1)),
        Err(BrokerError::InsufficientBuyingPower)
    );
    // 9주 × 30,000 = 270,000 + 올림(40.5) 41 = 270,041 은 된다.
    desk.place(&buy(HYN, "9", "30000"), at(2)).unwrap();
    assert_eq!(desk.krw(), cash("29854", "970146", "0"));
}

// 검증: C10
#[test]
fn 없는_주식은_팔_수_없다() {
    let desk = Desk::standard();
    assert_eq!(
        desk.place(&sell(SAM, "1", "70000"), at(0)),
        Err(BrokerError::InsufficientSellableQuantity)
    );
    assert_eq!(
        desk.place(&market_sell(SAM, "1"), at(0)),
        Err(BrokerError::InsufficientSellableQuantity)
    );
    assert!(desk.orders(OrderGroup::Open).is_empty());
}

// 검증: C10
#[test]
fn 파는_주문만큼_수량이_묶이고_가진_것보다_많이_팔_수_없다() {
    let desk = Desk::standard();
    desk.own(SAM, "10", "70000", at(0));
    assert_eq!(desk.sellable(SAM), d("10"));

    let first = desk.accepted(&sell(SAM, "6", "71000"), at(1));
    assert_eq!(desk.sellable(SAM), d("4"));
    assert_eq!(
        desk.holding(SAM).unwrap().quantity,
        d("10"),
        "묶여도 보유 수량은 그대로다"
    );

    assert_eq!(
        desk.place(&sell(SAM, "5", "71000"), at(2)),
        Err(BrokerError::InsufficientSellableQuantity)
    );
    desk.place(&sell(SAM, "4", "71000"), at(3)).unwrap();
    assert_eq!(desk.sellable(SAM), d("0"));

    desk.cancel(&first, at(4)).unwrap();
    assert_eq!(desk.sellable(SAM), d("6"), "취소하면 묶인 수량이 풀린다");
}

// 검증: C10
#[test]
fn 사는_주문을_취소하면_묶인_돈이_풀린다() {
    let desk = Desk::standard();
    let order_id = desk.accepted(&buy(SAM, "7", "70300"), at(0));
    assert_eq!(desk.krw(), cash("9507826", "492174", "0"));
    desk.cancel(&order_id, at(5)).unwrap();
    assert_eq!(desk.krw(), cash("10000000", "0", "0"));
}

// 검증: C10
#[test]
fn 장이_닫혀_있으면_받지_않는다() {
    let desk = Desk::standard();
    // 월요일 12:00 UTC(21:00 KST): 애프터마켓도 끝났다.
    let closed = utc(monday(), 12, 0, 0);
    assert_eq!(
        desk.place(&buy(SAM, "1", "70000"), closed),
        Err(BrokerError::OrderHoursClosed)
    );
    assert_eq!(desk.krw(), cash("10000000", "0", "0"));
}

// 검증: C10
#[test]
fn 달력을_모르는_시장의_주문은_받지_않는다() {
    let desk = Desk::bare(config());
    desk.broker.set_reference_price(sym(SAM), d("70000"));
    assert_eq!(
        desk.place(&buy(SAM, "1", "70000"), at(0)),
        Err(BrokerError::OrderHoursClosed)
    );
}

// 검증: C10
#[test]
fn 정규장은_시작_시각에_열리고_종료_시각에_닫힌다() {
    let desk = Desk::standard();
    let open = utc(monday(), 0, 0, 0);
    let last = utc(monday(), 6, 29, 59);
    let close = utc(monday(), 6, 30, 0);
    let before_open = open - chrono::Duration::seconds(1);

    // 기본 테스트 설정은 정규장만 받는다. 프리마켓(시작 1초 전)과 애프터마켓(종료 시각)은 닫힌 것이다.
    assert_eq!(
        desk.place(&buy(SAM, "1", "70000"), before_open),
        Err(BrokerError::OrderHoursClosed)
    );
    desk.place(&buy(SAM, "1", "70000"), open).unwrap();
    desk.place(&buy(SAM, "1", "70000"), last).unwrap();
    assert_eq!(
        desk.place(&buy(SAM, "1", "70000"), close),
        Err(BrokerError::OrderHoursClosed)
    );
}

// 검증: C10
#[test]
fn 세션별로_받는_유형은_설정을_따른다() {
    let mut config = config();
    config.kr.sessions.limit = vec![SessionKind::PreMarket, SessionKind::Regular];
    let desk = Desk::open(config);
    let pre_market = utc(monday(), 0, 0, 0) - chrono::Duration::minutes(30);

    // 프리마켓: 지정가는 받고, 시장가는 그 유형만 못 쓴다.
    desk.place(&buy(SAM, "1", "70000"), pre_market).unwrap();
    assert_eq!(
        desk.place(&market_buy(HYN, "1"), pre_market),
        Err(BrokerError::OrderTypeNotAllowed)
    );
    // 애프터마켓: 어떤 유형도 받지 않으니 닫힌 것이다.
    let after_market = utc(monday(), 7, 0, 0);
    assert_eq!(
        desk.place(&buy(HYN, "1", "30000"), after_market),
        Err(BrokerError::OrderHoursClosed)
    );
}

// 검증: C10
#[test]
fn 호가_단위에_맞지_않는_값은_받지_않고_맞는_단위를_알려_준다() {
    let desk = Desk::standard();
    assert_eq!(
        desk.place(&buy(SAM, "1", "70050"), at(0)),
        Err(BrokerError::InvalidRequest(InvalidReason::TickSize {
            tick: d("100")
        }))
    );
    assert_eq!(
        desk.place(&buy(HYN, "1", "30010"), at(0)),
        Err(BrokerError::InvalidRequest(InvalidReason::TickSize {
            tick: d("50")
        }))
    );
    assert_eq!(
        desk.place(&buy(SAM, "1", "70000.5"), at(0)),
        Err(BrokerError::InvalidRequest(InvalidReason::TickSize {
            tick: d("100")
        }))
    );
    desk.place(&buy(SAM, "1", "70100"), at(0)).unwrap();
    desk.place(&buy(HYN, "1", "30050"), at(0)).unwrap();
}

// 검증: C10
#[test]
fn 호가_단위_표는_설정한_대로_적용된다() {
    let mut config = config();
    config.kr.tick_table = vec![pi_paper::TickBand {
        below: None,
        tick: d("250"),
    }];
    let desk = Desk::open(config);
    assert_eq!(
        desk.place(&buy(SAM, "1", "70100"), at(0)),
        Err(BrokerError::InvalidRequest(InvalidReason::TickSize {
            tick: d("250")
        }))
    );
    desk.place(&buy(SAM, "1", "70250"), at(0)).unwrap();
}

// 검증: C10
#[test]
fn 상한가와_하한가는_받고_그_밖의_값은_받지_않는다() {
    let desk = Desk::standard();
    // 기준가 70,000 × 1.3 = 91,000, × 0.7 = 49,000.
    desk.own(SAM, "2", "70000", at(0));
    desk.place(&sell(SAM, "1", "91000"), at(1)).unwrap();
    assert_eq!(
        desk.place(&sell(SAM, "1", "91100"), at(2)),
        Err(BrokerError::PriceOutOfRange)
    );

    let other = Desk::standard();
    other.place(&buy(SAM, "1", "49000"), at(0)).unwrap();
    assert_eq!(
        other.place(&buy(SAM, "1", "48950"), at(1)),
        Err(BrokerError::PriceOutOfRange)
    );
}

// 검증: C10
#[test]
fn 가격제한폭은_설정한_대로_적용된다() {
    let mut narrow = config();
    narrow.kr.price_limit_rate = Some(d("0.10"));
    let desk = Desk::open(narrow);
    // 기준가 70,000 × 1.1 = 77,000.
    desk.place(&buy(SAM, "1", "77000"), at(0)).unwrap();
    assert_eq!(
        desk.place(&buy(SAM, "1", "77100"), at(1)),
        Err(BrokerError::PriceOutOfRange)
    );

    let mut unlimited = config();
    unlimited.kr.price_limit_rate = None;
    let desk = Desk::open(unlimited);
    desk.place(&buy(SAM, "1", "99000"), at(0)).unwrap();
}

// 검증: C10
#[test]
fn 기준가를_모르는_종목의_지정가는_받지_않는다() {
    let desk = Desk::bare(config());
    desk.broker.set_calendar(kr_calendar(monday()));
    assert_eq!(
        desk.place(&buy(SAM, "1", "70000"), at(0)),
        Err(BrokerError::ReferencePriceUnknown)
    );
    desk.broker.set_reference_price(sym(SAM), d("70000"));
    desk.place(&buy(SAM, "1", "70000"), at(1)).unwrap();
}

// 검증: C10
#[test]
fn 영_이하의_수량과_값_그리고_틀린_모양은_받지_않는다() {
    let desk = Desk::standard();
    desk.own(SAM, "5", "70000", at(0));
    let invalid = |reason| Err(BrokerError::InvalidRequest(reason));

    assert_eq!(
        desk.place(&buy(HYN, "0", "30000"), at(1)),
        invalid(InvalidReason::NonPositiveQuantity)
    );
    assert_eq!(
        desk.place(&sell(SAM, "-1", "70000"), at(1)),
        invalid(InvalidReason::NonPositiveQuantity)
    );
    assert_eq!(
        desk.place(&buy(HYN, "1.5", "30000"), at(1)),
        invalid(InvalidReason::FractionalQuantity)
    );
    assert_eq!(
        desk.place(&buy(HYN, "1", "0"), at(1)),
        invalid(InvalidReason::NonPositivePrice)
    );
    assert_eq!(
        desk.place(&buy(HYN, "1", "-30000"), at(1)),
        invalid(InvalidReason::NonPositivePrice)
    );

    let mut limit_without_price = buy(HYN, "1", "30000");
    limit_without_price.price = None;
    assert_eq!(
        desk.place(&limit_without_price, at(1)),
        invalid(InvalidReason::LimitPriceRequired)
    );
    let mut market_with_price = market_sell(SAM, "1");
    market_with_price.price = Some(d("70000"));
    assert_eq!(
        desk.place(&market_with_price, at(1)),
        invalid(InvalidReason::MarketPriceNotAllowed)
    );

    assert_eq!(desk.orders(OrderGroup::Open), vec![]);
    assert_eq!(desk.sellable(SAM), d("5"));
}

// 검증: C10
#[test]
fn 시장가로_사려면_시세를_한_번은_받았어야_한다() {
    let desk = Desk::standard();
    assert_eq!(
        desk.place(&market_buy(SAM, "5"), at(0)),
        Err(BrokerError::QuoteUnknown)
    );

    // 마지막 시세 70,000 × 1.001 = 70,070 → 호가 단위(100)로 올려 70,100.
    // 5주 × 70,100 = 350,500, 수수료 52.575 → 올려서 53. 묶는 돈 350,553.
    desk.quote(SAM, "70000", at(1));
    let order_id = desk.accepted(&market_buy(SAM, "5"), at(2));
    assert_eq!(desk.krw(), cash("9649447", "350553", "0"));
    let order = desk.order(&order_id);
    assert_eq!(order.order_type, OrderType::Market);
    assert_eq!(order.price, None);
    assert_eq!(order.status, OrderStatus::Pending);
}

// 검증: C10
#[test]
fn 같은_종목에_반대_방향의_열린_주문이_있으면_받지_않는다() {
    let desk = Desk::standard();
    desk.own(SAM, "10", "70000", at(0));
    let pending_buy = desk.accepted(&buy(SAM, "1", "69000"), at(1));
    assert_eq!(
        desk.place(&sell(SAM, "1", "71000"), at(2)),
        Err(BrokerError::OppositePendingOrderExists)
    );
    // 다른 종목은 상관없다. 열린 주문이 없어지면 다시 받는다.
    desk.place(&buy(HYN, "1", "30000"), at(3)).unwrap();
    desk.cancel(&pending_buy, at(4)).unwrap();
    desk.place(&sell(SAM, "1", "71000"), at(5)).unwrap();
}

// 검증: C10
#[test]
fn 미국_주식은_달러로만_셈하고_원화를_쓰지_않는다() {
    let desk = Desk::standard();
    // 10주 × 150.25 = 1,502.50. 수수료 0.1% = 1.5025 → 센트로 올려 1.51. 묶는 돈 1,504.01.
    let order_id = desk.accepted(&us_buy(AAPL, "10", "150.25"), us_at(0));
    assert_eq!(desk.usd(), cash("8495.99", "1504.01", "0"));
    assert_eq!(desk.krw(), cash("10000000", "0", "0"));
    assert_eq!(desk.order(&order_id).currency, Currency::Usd);

    // 원화가 넉넉해도 달러가 모자라면 받지 않는다(환전 없음). 100주 × 150.25 = 15,025.
    assert_eq!(
        desk.place(&us_buy(AAPL, "100", "150.25"), us_at(1)),
        Err(BrokerError::InsufficientBuyingPower)
    );
}

// 검증: C10
#[test]
fn 미국_호가_단위는_1달러를_경계로_바뀌고_가격제한이_없다() {
    let desk = Desk::standard();
    let invalid = |tick: &str| {
        Err(BrokerError::InvalidRequest(InvalidReason::TickSize {
            tick: d(tick),
        }))
    };
    assert_eq!(
        desk.place(&us_buy(AAPL, "1", "150.255"), us_at(0)),
        invalid("0.01")
    );
    assert_eq!(
        desk.place(&us_buy("PENNY", "1", "0.12345"), us_at(0)),
        invalid("0.0001")
    );
    desk.place(&us_buy("PENNY", "1", "0.1234"), us_at(0))
        .unwrap();
    // 기준가를 넣지 않았어도, 값이 아무리 멀어도 받는다.
    desk.place(&us_buy(AAPL, "1", "0.99"), us_at(0)).unwrap();
}

// 검증: C10
#[test]
fn 시장마다_장_시간을_따로_본다() {
    let desk = Desk::standard();
    // 국내 정규장 시각에는 미국 정규장이 닫혀 있고, 그 반대도 같다.
    assert_eq!(
        desk.place(&us_buy(AAPL, "1", "150.25"), at(0)),
        Err(BrokerError::OrderHoursClosed)
    );
    assert_eq!(
        desk.place(&buy(SAM, "1", "70000"), us_at(0)),
        Err(BrokerError::OrderHoursClosed)
    );
    desk.place(
        &buy(SAM, "1", "70000").timeout(UnfilledTimeout::SCALP),
        at(0),
    )
    .unwrap();
}
