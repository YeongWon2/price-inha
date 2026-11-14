//! 체결과 돈 계산. 기대값은 손으로 셈해 끝자리까지 비교한다.
//! 테스트 설정: 수수료 0.015%, 거래세 0.20%, 원 미만 버림, 시장가 미끄러짐 0.1%.

use pi_market::Currency;
use pi_paper::{
    CloseReason, Fill, LimitFillPrice, MaxFill, OrderEventKind, OrderStatus, RoundingMode, Side,
    UnfilledTimeout,
};

use crate::support::{
    AAPL, Desk, HYN, SAM, at, buy, cash, config, config_with_krw, d, day, kinds, market_buy,
    market_sell, monday, quote_received, sell, sym, us_at, us_buy, us_sell, utc,
};

// 검증: C10
#[test]
fn 지정가_사기는_시세가_지정가_이하일_때_체결된다() {
    let desk = Desk::standard();
    let order_id = desk.accepted(&buy(SAM, "7", "70300"), at(0));

    // 시세가 지정가보다 높으면 체결되지 않는다.
    assert!(desk.quote(SAM, "70400", at(1)).is_empty());
    assert_eq!(desk.order(&order_id).status, OrderStatus::Pending);
    assert!(desk.broker.fills().is_empty());

    // 시세 = 지정가. 7주 × 70,300 = 492,100. 수수료 73.815 → 버려서 73.
    let events = desk.quote(SAM, "70300", at(2));
    assert_eq!(kinds(&events), vec![OrderEventKind::Fill]);

    let order = desk.order(&order_id);
    assert_eq!(order.status, OrderStatus::Filled);
    assert_eq!(order.execution.filled_quantity, d("7"));
    assert_eq!(order.execution.average_filled_price, Some(d("70300")));
    assert_eq!(order.execution.filled_amount, d("492100"));
    assert_eq!(order.execution.commission, d("73"));
    assert_eq!(order.execution.tax, d("0"));
    assert_eq!(order.execution.filled_at, Some(at(2)));
    assert_eq!(order.execution.settlement_date, Some(day(2026, 11, 18)));
    assert_eq!(order.close_reason, None);
    assert_eq!(events[0].order, order);

    // 10,000,000 − 492,100 − 73 = 9,507,827. 묶였던 돈 가운데 남은 1원도 풀린다.
    assert_eq!(desk.krw(), cash("9507827", "0", "0"));

    let holding = desk.holding(SAM).unwrap();
    assert_eq!(holding.symbol, sym(SAM));
    assert_eq!(holding.market, pi_market::Market::Kr);
    assert_eq!(holding.currency, Currency::Krw);
    assert_eq!(holding.quantity, d("7"));
    assert_eq!(holding.average_purchase_price, d("70300"));
    assert_eq!(desk.sellable(SAM), d("7"));

    assert_eq!(
        desk.broker.fills(),
        vec![Fill {
            order_id,
            market: pi_market::Market::Kr,
            symbol: sym(SAM),
            side: Side::Buy,
            quantity: d("7"),
            price: d("70300"),
            amount: d("492100"),
            commission: d("73"),
            tax: d("0"),
            currency: Currency::Krw,
            filled_at: at(2),
            settlement_date: day(2026, 11, 18),
        }]
    );
}

// 검증: C10
#[test]
fn 지정가는_시세가_더_유리해도_지정가로_체결한다() {
    let desk = Desk::standard();
    let order_id = desk.accepted(&buy(SAM, "10", "70000"), at(0));
    desk.quote(SAM, "69500", at(1));
    let order = desk.order(&order_id);
    assert_eq!(order.status, OrderStatus::Filled);
    assert_eq!(order.execution.average_filled_price, Some(d("70000")));
    // 10,000,000 − 700,000 − 105 = 9,299,895.
    assert_eq!(desk.krw(), cash("9299895", "0", "0"));

    // 팔기도 같다: 지정가 71,100, 시세 72,000 → 71,100에 체결.
    let sell_id = desk.accepted(&sell(SAM, "3", "71100"), at(2));
    desk.quote(SAM, "72000", at(3));
    assert_eq!(
        desk.order(&sell_id).execution.average_filled_price,
        Some(d("71100"))
    );
}

// 검증: C10, E29
#[test]
fn 체결_값_규칙을_시세로_바꾸면_시세로_체결하고_평균을_낸다() {
    let mut config = config();
    config.kr.limit_fill_price = LimitFillPrice::Quote;
    config.kr.max_fill = MaxFill::PerQuote(d("5"));
    let desk = Desk::open(config);
    let order_id = desk.accepted(&buy(SAM, "10", "70000"), at(0));

    // 첫 시세 69,900: 5주. 금액 349,500, 수수료 52.425 → 52.
    let events = desk.quote(SAM, "69900", at(1));
    assert_eq!(kinds(&events), vec![OrderEventKind::PartialFill]);
    let order = desk.order(&order_id);
    assert_eq!(order.status, OrderStatus::PartialFilled);
    assert_eq!(order.execution.filled_quantity, d("5"));
    assert_eq!(order.execution.average_filled_price, Some(d("69900")));
    // 남은 5주가 묶는 돈: 350,000 + 올림(52.5) 53 = 350,053.
    // 전체 10,000,000 − 349,552 = 9,650,448. 쓸 수 있는 돈 9,650,448 − 350,053 = 9,300,395.
    assert_eq!(desk.krw(), cash("9300395", "350053", "0"));

    // 둘째 시세 69,700: 5주. 금액 348,500, 수수료 52.275 → 52.
    let events = desk.quote(SAM, "69700", at(2));
    assert_eq!(kinds(&events), vec![OrderEventKind::Fill]);
    let order = desk.order(&order_id);
    assert_eq!(order.status, OrderStatus::Filled);
    assert_eq!(order.execution.filled_quantity, d("10"));
    assert_eq!(order.execution.average_filled_price, Some(d("69800")));
    assert_eq!(order.execution.filled_amount, d("698000"));
    assert_eq!(order.execution.commission, d("104"));
    assert_eq!(order.execution.filled_at, Some(at(2)));
    assert_eq!(desk.krw(), cash("9301896", "0", "0"));
    assert_eq!(
        desk.holding(SAM).unwrap().average_purchase_price,
        d("69800")
    );
    assert_eq!(desk.broker.fills().len(), 2);
}

// 검증: C10
#[test]
fn 지정가_팔기는_시세가_지정가_이상일_때_체결되고_수수료와_거래세를_뗀다() {
    let desk = Desk::standard();
    desk.own(SAM, "10", "70000", at(0));
    assert_eq!(desk.krw(), cash("9299895", "0", "0"));

    let order_id = desk.accepted(&sell(SAM, "3", "71100"), at(1));
    assert!(desk.quote(SAM, "71000", at(2)).is_empty());
    assert_eq!(desk.order(&order_id).status, OrderStatus::Pending);

    // 3주 × 71,100 = 213,300. 수수료 31.995 → 31. 거래세 426.6 → 426. 받는 돈 212,843.
    desk.quote(SAM, "71100", at(3));
    let order = desk.order(&order_id);
    assert_eq!(order.status, OrderStatus::Filled);
    assert_eq!(order.execution.filled_amount, d("213300"));
    assert_eq!(order.execution.commission, d("31"));
    assert_eq!(order.execution.tax, d("426"));

    // 판 돈은 결제일까지 쓸 수 없는 칸에 있다.
    assert_eq!(desk.krw(), cash("9299895", "0", "212843"));
    assert_eq!(desk.krw().total(), d("9512738"));

    // 판 만큼만 줄고, 평균 산 값은 그대로다.
    let holding = desk.holding(SAM).unwrap();
    assert_eq!(holding.quantity, d("7"));
    assert_eq!(holding.average_purchase_price, d("70000"));
    assert_eq!(desk.sellable(SAM), d("7"));

    let fill = desk.broker.fills().pop().unwrap();
    assert_eq!(fill.side, Side::Sell);
    assert_eq!(fill.tax, d("426"));
    assert_eq!(fill.commission, d("31"));
}

// 검증: C10
#[test]
fn 다_팔면_보유_목록에서_사라진다() {
    let desk = Desk::standard();
    desk.own(SAM, "10", "70000", at(0));
    desk.own(HYN, "2", "30000", at(0));
    desk.accepted(&sell(SAM, "10", "70000"), at(1));
    desk.quote(SAM, "70000", at(2));
    assert!(desk.holding(SAM).is_none());
    assert_eq!(desk.sellable(SAM), d("0"));
    let left = desk.holdings();
    assert_eq!(left.len(), 1);
    assert_eq!(left[0].symbol, sym(HYN));
}

// 검증: C10
#[test]
fn 여러_번_나눠_사면_평균_산_값은_수량_가중_평균이다() {
    let desk = Desk::standard();
    desk.own(SAM, "3", "70000", at(0));
    desk.own(SAM, "2", "70100", at(1));
    // (3 × 70,000 + 2 × 70,100) ÷ 5 = 70,040.
    let holding = desk.holding(SAM).unwrap();
    assert_eq!(holding.quantity, d("5"));
    assert_eq!(holding.average_purchase_price, d("70040"));
}

// 검증: C10
// 반올림 방향의 기본값은 확인하지 못했다. 설정한 방향이 그대로 적용되는지만 본다.
#[test]
fn 수수료와_세금의_반올림은_설정한_방향을_따른다() {
    // (방향, 68,000 × 1의 수수료 10.2, 70,300 × 7의 수수료 73.815, 71,100 × 3의 수수료 31.995, 거래세 426.6)
    for (mode, small, large, sell_commission, sell_tax) in [
        (RoundingMode::Down, "10", "73", "31", "426"),
        (RoundingMode::HalfUp, "10", "74", "32", "427"),
        (RoundingMode::Up, "11", "74", "32", "427"),
    ] {
        let mut config = config();
        config.kr.rounding.mode = mode;
        let desk = Desk::open(config);

        let first = desk.accepted(&buy(SAM, "1", "68000"), at(0));
        desk.quote(SAM, "68000", at(1));
        assert_eq!(
            desk.order(&first).execution.commission,
            d(small),
            "{mode:?}"
        );

        let second = desk.accepted(&buy(SAM, "7", "70300"), at(2));
        desk.quote(SAM, "70300", at(3));
        assert_eq!(
            desk.order(&second).execution.commission,
            d(large),
            "{mode:?}"
        );

        let third = desk.accepted(&sell(SAM, "3", "71100"), at(4));
        desk.quote(SAM, "71100", at(5));
        let execution = desk.order(&third).execution;
        assert_eq!(execution.commission, d(sell_commission), "{mode:?}");
        assert_eq!(execution.tax, d(sell_tax), "{mode:?}");

        // 돈은 뗀 만큼만 줄어든다: 처음 돈 − 산 금액 − 수수료들 + 판 금액 − 수수료 − 세금.
        let expected = d("10000000") - d("68000") - d(small) - d("492100") - d(large) + d("213300")
            - d(sell_commission)
            - d(sell_tax);
        assert_eq!(desk.krw().total(), expected, "{mode:?}");
    }
}

// 검증: C10
// 수수료율 · 세율의 기본값은 확인하지 못했다. 설정한 비율이 그대로 적용되는지만 본다.
#[test]
fn 수수료율과_세율은_설정한_값이_그대로_적용된다() {
    let mut config = config();
    config.kr.commission_rate = d("0.001");
    config.kr.sell_tax_rate = d("0.005");
    let desk = Desk::open(config);

    // 사기 10 × 70,000 = 700,000 → 수수료 700. 묶는 돈 700,700.
    let buy_id = desk.accepted(&buy(SAM, "10", "70000"), at(0));
    assert_eq!(desk.krw(), cash("9299300", "700700", "0"));
    desk.quote(SAM, "70000", at(1));
    assert_eq!(desk.order(&buy_id).execution.commission, d("700"));

    // 팔기 10 × 71,000 = 710,000 → 수수료 710, 세금 3,550.
    let sell_id = desk.accepted(&sell(SAM, "10", "71000"), at(2));
    desk.quote(SAM, "71000", at(3));
    let execution = desk.order(&sell_id).execution;
    assert_eq!(execution.commission, d("710"));
    assert_eq!(execution.tax, d("3550"));
    assert_eq!(desk.krw(), cash("9299300", "0", "705740"));
}

// 검증: C10
#[test]
fn 시장가_사기는_다음_시세에_미끄러짐을_더해_체결한다() {
    let desk = Desk::standard();
    desk.quote(SAM, "70000", at(0));
    let order_id = desk.accepted(&market_buy(SAM, "5"), at(1));
    // 접수 전에 받은 시세로는 체결되지 않는다. 다음 시세를 기다린다.
    assert_eq!(desk.order(&order_id).status, OrderStatus::Pending);
    assert!(desk.broker.fills().is_empty());

    // 다음 시세 70,200 × 1.001 = 70,270.2 → 불리한 쪽 호가 단위로 올려 70,300.
    // 5주 × 70,300 = 351,500. 수수료 52.725 → 52.
    let events = desk.quote(SAM, "70200", at(2));
    assert_eq!(kinds(&events), vec![OrderEventKind::Fill]);
    let order = desk.order(&order_id);
    assert_eq!(order.status, OrderStatus::Filled);
    assert_eq!(order.execution.average_filled_price, Some(d("70300")));
    assert_eq!(order.execution.filled_amount, d("351500"));
    assert_eq!(order.execution.commission, d("52"));
    assert_eq!(desk.krw(), cash("9648448", "0", "0"));
}

// 검증: C10
#[test]
fn 시장가_팔기는_미끄러짐을_빼고_체결한다() {
    let desk = Desk::standard();
    desk.own(SAM, "10", "70000", at(0));
    let order_id = desk.accepted(&market_sell(SAM, "4"), at(1));
    assert_eq!(desk.sellable(SAM), d("6"));

    // 시세 70,000 × 0.999 = 69,930 → 불리한 쪽 호가 단위로 내려 69,900.
    // 4주 × 69,900 = 279,600. 수수료 41.94 → 41. 거래세 559.2 → 559. 받는 돈 279,000.
    desk.quote(SAM, "70000", at(2));
    let order = desk.order(&order_id);
    assert_eq!(order.status, OrderStatus::Filled);
    assert_eq!(order.execution.average_filled_price, Some(d("69900")));
    assert_eq!(order.execution.commission, d("41"));
    assert_eq!(order.execution.tax, d("559"));
    assert_eq!(desk.krw(), cash("9299895", "0", "279000"));
    assert_eq!(desk.holding(SAM).unwrap().quantity, d("6"));
}

// 검증: C10
// 미끄러짐의 기본값은 확인하지 못했다. 설정한 값이 그대로 적용되는지만 본다.
#[test]
fn 미끄러짐은_설정한_값이_그대로_적용된다() {
    let mut config = config();
    config.kr.market_slippage_rate = d("0");
    let desk = Desk::open(config);
    desk.quote(SAM, "70000", at(0));
    let order_id = desk.accepted(&market_buy(SAM, "1"), at(1));
    desk.quote(SAM, "70200", at(2));
    assert_eq!(
        desk.order(&order_id).execution.average_filled_price,
        Some(d("70200"))
    );

    let mut config = crate::support::config();
    config.kr.market_slippage_rate = d("0.01");
    let desk = Desk::open(config);
    desk.quote(SAM, "70000", at(0));
    let order_id = desk.accepted(&market_buy(SAM, "1"), at(1));
    // 70,000 × 1.01 = 70,700 (호가 단위에 딱 맞는다).
    desk.quote(SAM, "70000", at(2));
    assert_eq!(
        desk.order(&order_id).execution.average_filled_price,
        Some(d("70700"))
    );
}

// 검증: C10
#[test]
fn 시장가로_사는데_시세가_올라_돈이_모자라면_살_수_있는_만큼만_체결하고_거부한다() {
    let desk = Desk::open(config_with_krw("400000"));
    desk.quote(SAM, "70000", at(0));
    // 묶는 돈 350,553 (5주 × 70,100 + 53).
    let order_id = desk.accepted(&market_buy(SAM, "5"), at(1));
    assert_eq!(desk.krw(), cash("49447", "350553", "0"));

    // 다음 시세 85,000 × 1.001 = 85,085 → 85,100.
    // 5주는 425,500 + 63 = 425,563 으로 가진 돈 400,000을 넘는다.
    // 4주는 340,400 + 51(51.06 버림) = 340,451 로 살 수 있다.
    let events = desk.quote(SAM, "85000", at(2));
    assert_eq!(
        kinds(&events),
        vec![OrderEventKind::PartialFill, OrderEventKind::Rejected]
    );
    let order = desk.order(&order_id);
    assert_eq!(order.status, OrderStatus::Rejected);
    assert_eq!(
        order.close_reason,
        Some(CloseReason::InsufficientBuyingPower)
    );
    assert_eq!(order.execution.filled_quantity, d("4"));
    assert_eq!(order.execution.average_filled_price, Some(d("85100")));
    assert_eq!(order.execution.commission, d("51"));
    // 400,000 − 340,451 = 59,549. 묶인 돈은 남지 않는다.
    assert_eq!(desk.krw(), cash("59549", "0", "0"));
    assert_eq!(desk.holding(SAM).unwrap().quantity, d("4"));
}

// 검증: C10
#[test]
fn 시장가로_사는데_한_주도_살_수_없으면_체결_없이_거부한다() {
    let desk = Desk::open(config_with_krw("75000"));
    desk.quote(SAM, "70000", at(0));
    // 묶는 돈 70,100 + 올림(10.515) 11 = 70,111.
    let order_id = desk.accepted(&market_buy(SAM, "1"), at(1));
    assert_eq!(desk.krw(), cash("4889", "70111", "0"));

    // 다음 시세 80,000 → 80,080 → 80,100. 한 주에 80,100 + 12 = 80,112 로 모자란다.
    let events = desk.quote(SAM, "80000", at(2));
    assert_eq!(kinds(&events), vec![OrderEventKind::Rejected]);
    let order = desk.order(&order_id);
    assert_eq!(order.status, OrderStatus::Rejected);
    assert_eq!(order.execution.filled_quantity, d("0"));
    assert_eq!(desk.krw(), cash("75000", "0", "0"));
    assert!(desk.holdings().is_empty());
    assert!(desk.broker.fills().is_empty());
}

// 검증: C10, E29
#[test]
fn 한_시세에서_체결되는_수량은_설정한_최대까지다() {
    let mut config = config();
    config.kr.max_fill = MaxFill::PerQuote(d("3"));
    let desk = Desk::open(config);
    let order_id = desk.accepted(&buy(SAM, "10", "70000"), at(0));

    // 3주: 210,000, 수수료 31.5 → 31. 남은 7주가 묶는 돈 490,000 + 올림(73.5) 74 = 490,074.
    let events = desk.quote(SAM, "70000", at(1));
    assert_eq!(kinds(&events), vec![OrderEventKind::PartialFill]);
    let order = desk.order(&order_id);
    assert_eq!(order.status, OrderStatus::PartialFilled);
    assert_eq!(order.execution.filled_quantity, d("3"));
    assert_eq!(order.execution.commission, d("31"));
    assert_eq!(desk.krw(), cash("9299895", "490074", "0"));
    assert_eq!(
        desk.holding(SAM).unwrap().quantity,
        d("3"),
        "체결된 만큼만 보유다"
    );

    desk.quote(SAM, "70000", at(2));
    desk.quote(SAM, "70000", at(3));
    assert_eq!(desk.order(&order_id).execution.filled_quantity, d("9"));
    assert_eq!(desk.order(&order_id).status, OrderStatus::PartialFilled);

    // 2차 6번: 수수료는 주문 단위로 누적해 뗀다. 누적 금액 210,000 → 420,000 → 630,000 → 700,000의
    // 수수료는 31.5 → 63 → 94.5 → 105, 버려서 31 → 63 → 94 → 105. 체결마다 뗀 것은 31 · 32 · 31 · 11.
    // 뗀 합 105는 10주가 한 번에 체결됐을 때(700,000 × 0.015% = 105)와 같다.
    let events = desk.quote(SAM, "70000", at(4));
    assert_eq!(kinds(&events), vec![OrderEventKind::Fill]);
    let order = desk.order(&order_id);
    assert_eq!(order.status, OrderStatus::Filled);
    assert_eq!(order.execution.filled_quantity, d("10"));
    assert_eq!(order.execution.commission, d("105"));
    assert_eq!(desk.krw(), cash("9299895", "0", "0"));
    let taken: Vec<_> = desk
        .broker
        .fills()
        .iter()
        .map(|fill| fill.commission)
        .collect();
    assert_eq!(taken, vec![d("31"), d("32"), d("31"), d("11")]);
    assert_eq!(desk.broker.fills().len(), 4);

    // 다 체결된 주문은 더 체결되지 않는다.
    assert!(desk.quote(SAM, "70000", at(5)).is_empty());
    assert_eq!(desk.broker.fills().len(), 4);
}

// 검증: C10
#[test]
fn 같은_종목의_주문은_접수한_순서대로_저마다_체결된다() {
    let mut config = config();
    config.kr.max_fill = MaxFill::PerQuote(d("3"));
    let desk = Desk::open(config);
    let first = desk.accepted(&buy(SAM, "2", "70000"), at(0));
    let second = desk.accepted(&buy(SAM, "5", "70100"), at(1));
    let untouched = desk.accepted(&buy(SAM, "1", "69000"), at(2));

    let events = desk.quote(SAM, "70000", at(3));
    assert_eq!(
        kinds(&events),
        vec![OrderEventKind::Fill, OrderEventKind::PartialFill]
    );
    let fills = desk.broker.fills();
    assert_eq!(fills.len(), 2);
    assert_eq!(fills[0].order_id, first);
    assert_eq!(fills[0].quantity, d("2"));
    assert_eq!(fills[1].order_id, second);
    assert_eq!(fills[1].quantity, d("3"));
    assert_eq!(fills[1].price, d("70100"));
    assert_eq!(desk.order(&untouched).status, OrderStatus::Pending);
}

// 검증: N6
#[test]
fn 늦은_시세로는_체결하지_않는다() {
    let desk = Desk::standard();
    let order_id = desk.accepted(&buy(SAM, "1", "70000"), at(0));

    // 받은 지 31초 된 시세: 늦었다(30초를 넘었다).
    let late = quote_received(SAM, "70000", Currency::Krw, at(9));
    assert!(desk.on_quote(&late, at(40)).is_empty());
    assert_eq!(desk.order(&order_id).status, OrderStatus::Pending);
    assert!(desk.broker.fills().is_empty());
    assert_eq!(desk.krw(), cash("9929989", "70011", "0"));

    // 받은 지 딱 30초 된 시세: 아직 늦지 않았다.
    let fresh = quote_received(SAM, "70000", Currency::Krw, at(11));
    assert_eq!(
        kinds(&desk.on_quote(&fresh, at(41))),
        vec![OrderEventKind::Fill]
    );
    assert_eq!(desk.order(&order_id).status, OrderStatus::Filled);
}

// 검증: N6
#[test]
fn 늦은_시세_기준은_설정을_따른다() {
    let mut config = config();
    config.stale.max_age = std::time::Duration::from_secs(5);
    let desk = Desk::open(config);
    let order_id = desk.accepted(&buy(SAM, "1", "70000"), at(0));
    let six_seconds_old = quote_received(SAM, "70000", Currency::Krw, at(4));
    assert!(desk.on_quote(&six_seconds_old, at(10)).is_empty());
    assert_eq!(desk.order(&order_id).status, OrderStatus::Pending);
}

// 검증: C10, E12
// 2차 8번: 주문은 장 마감을 넘기지 않는다. 기다리는 시간이 사흘이어도 접수한 세션(정규장)이
// 끝나면 취소되므로, 그 뒤 세션의 시세로는 체결될 주문이 남아 있지 않다.
#[test]
fn 장이_닫힌_시각이나_받지_않는_세션의_시세로는_체결하지_않는다() {
    let desk = Desk::standard();
    let long = UnfilledTimeout::After(std::time::Duration::from_secs(3 * 24 * 3600));
    let order_id = desk.accepted(&buy(SAM, "1", "70000").timeout(long), at(0));

    // 애프터마켓(테스트 설정은 정규장만 받는다)의 시세: 체결 없이, 정규장 끝에 마감이 된 주문이 취소된다.
    let events = desk.quote(SAM, "70000", utc(monday(), 7, 0, 0));
    assert_eq!(kinds(&events), vec![OrderEventKind::Canceled]);
    // 완전히 닫힌 시각의 시세: 아무 일도 없다.
    assert!(desk.quote(SAM, "70000", utc(monday(), 12, 0, 0)).is_empty());

    let order = desk.order(&order_id);
    assert_eq!(order.status, OrderStatus::Canceled);
    assert_eq!(order.close_reason, Some(CloseReason::UnfilledTimeout));
    assert_eq!(order.execution.filled_quantity, d("0"));
    assert!(desk.broker.fills().is_empty());
    assert_eq!(desk.krw(), cash("10000000", "0", "0"));
}

// 검증: C10
#[test]
fn 다른_종목이나_다른_통화의_시세로는_체결하지_않는다() {
    let desk = Desk::standard();
    let order_id = desk.accepted(&buy(SAM, "1", "70000"), at(0));

    assert!(desk.quote(HYN, "30000", at(1)).is_empty());
    let dollars = quote_received(SAM, "70000", Currency::Usd, at(2));
    assert!(desk.on_quote(&dollars, at(2)).is_empty());

    assert_eq!(desk.order(&order_id).status, OrderStatus::Pending);
    assert!(desk.broker.fills().is_empty());
}

// 검증: C10
#[test]
fn 미국_주식은_달러와_센트로_셈한다() {
    let desk = Desk::standard();
    let buy_id = desk.accepted(&us_buy(AAPL, "10", "150.25"), us_at(0));

    // 10주 × 150.25 = 1,502.50. 수수료 0.1% = 1.5025 → 센트 미만 버려 1.50.
    desk.us_quote(AAPL, "150.20", us_at(1));
    let order = desk.order(&buy_id);
    assert_eq!(order.status, OrderStatus::Filled);
    assert_eq!(order.execution.filled_amount, d("1502.50"));
    assert_eq!(order.execution.commission, d("1.50"));
    assert_eq!(order.execution.tax, d("0"));
    // 미국은 T+1: 월요일 체결 → 화요일.
    assert_eq!(order.execution.settlement_date, Some(day(2026, 11, 17)));
    assert_eq!(desk.usd(), cash("8496.00", "0", "0"));

    let holding = desk.holding(AAPL).unwrap();
    assert_eq!(holding.market, pi_market::Market::Us);
    assert_eq!(holding.currency, Currency::Usd);
    assert_eq!(holding.average_purchase_price, d("150.25"));

    // 팔기 10주 × 151.00 = 1,510.00. 수수료 1.51. 팔 때 세금 0.01% = 0.151 → 0.15. 받는 돈 1,508.34.
    let sell_id = desk.accepted(&us_sell(AAPL, "10", "151.00"), us_at(2));
    desk.us_quote(AAPL, "151.10", us_at(3));
    let execution = desk.order(&sell_id).execution;
    assert_eq!(execution.commission, d("1.51"));
    assert_eq!(execution.tax, d("0.15"));
    assert_eq!(desk.usd(), cash("8496.00", "0", "1508.34"));

    // 원화는 한 푼도 움직이지 않는다.
    assert_eq!(desk.krw(), cash("10000000", "0", "0"));
}
