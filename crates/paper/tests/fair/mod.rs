//! 2차 1~4번: 모의 체결이 실제보다 유리하게 나오지 않게 한다.
//! 주문 뒤에 온 새 시세로만 체결하고, 같은 시세를 두 번 쓰지 않고, 앞날의 시세와 늦은 시세를 쓰지 않는다.

use pi_market::Currency;
use pi_paper::{BrokerError, MaxFill, OrderEventKind, OrderGroup, OrderStatus};

use crate::support::{
    Desk, HYN, SAM, at, buy, cash, config, d, kinds, market_buy, quote_full, quote_received,
};

fn per_quote(most: &str) -> pi_paper::PaperConfig {
    let mut config = config();
    config.kr.max_fill = MaxFill::PerQuote(d(most));
    config
}

// 검증: C10, N6
// 1차 검토의 실험: 시장가 사기를 10초에 접수하고, 받은 시각이 0초인 시세를 11초에 주면 10주가 전부 체결됐다.
#[test]
fn 주문_전에_받은_시세를_다시_줘도_시장가는_체결되지_않는다() {
    let desk = Desk::standard();
    let old = quote_received(SAM, "70000", Currency::Krw, at(0));
    desk.on_quote(&old, at(0));
    // 묶는 돈: 70,000 × 1.001 → 70,100. 10주 × 70,100 = 701,000, 수수료 105.15 → 올려서 106.
    let order_id = desk.accepted(&market_buy(SAM, "10"), at(10));

    let events = desk.on_quote(&old, at(11));
    assert!(events.is_empty(), "체결 0건이어야 한다: {events:?}");
    let order = desk.order(&order_id);
    assert_eq!(order.status, OrderStatus::Pending);
    assert_eq!(order.execution.filled_quantity, d("0"));
    assert!(desk.broker.fills().is_empty());
    assert!(desk.holdings().is_empty());
    assert_eq!(desk.krw(), cash("9298894", "701106", "0"));
}

// 검증: C10
#[test]
fn 받은_시각이_접수_시각보다_앞이거나_같은_시세로는_체결하지_않는다() {
    let desk = Desk::standard();
    desk.quote(SAM, "70000", at(0));
    let order_id = desk.accepted(&market_buy(SAM, "10"), at(10));

    // 접수 5초 전에 받아 둔 다른 값의 시세를 접수 뒤에 준다.
    let before = quote_full(SAM, "70200", Currency::Krw, None, at(5));
    assert!(desk.on_quote(&before, at(11)).is_empty());
    // 받은 시각이 접수 시각과 딱 같은 시세.
    let same = quote_full(SAM, "70300", Currency::Krw, None, at(10));
    assert!(desk.on_quote(&same, at(12)).is_empty());
    assert_eq!(desk.order(&order_id).status, OrderStatus::Pending);
    assert!(desk.broker.fills().is_empty());

    // 접수 1초 뒤에 받은 시세: 70,400 × 1.001 = 70,470.4 → 70,500에 10주.
    let after = quote_full(SAM, "70400", Currency::Krw, None, at(11));
    assert_eq!(
        kinds(&desk.on_quote(&after, at(13))),
        vec![OrderEventKind::Fill]
    );
    let order = desk.order(&order_id);
    assert_eq!(order.execution.filled_quantity, d("10"));
    assert_eq!(order.execution.average_filled_price, Some(d("70500")));
}

// 검증: C10
// 1차 검토의 실험: 지정가를 20초에 접수하고, 거래소 시각이 50분 전인 체결가를 방금 받았다고 주면 체결됐다.
#[test]
fn 거래소_시각이_접수_시각보다_앞이거나_같은_시세로는_체결하지_않는다() {
    let desk = Desk::standard();
    let order_id = desk.accepted(&buy(HYN, "10", "30000"), at(20));

    // 방금 받았지만 50분 전에 체결된 값(거래가 없는 종목의 마지막 체결가).
    let old_trade = quote_full(HYN, "30000", Currency::Krw, Some(at(-3000)), at(21));
    assert!(desk.on_quote(&old_trade, at(21)).is_empty());
    // 거래소 시각이 접수 시각과 딱 같은 값.
    let same_time = quote_full(HYN, "29950", Currency::Krw, Some(at(20)), at(22));
    assert!(desk.on_quote(&same_time, at(22)).is_empty());

    assert_eq!(desk.order(&order_id).status, OrderStatus::Pending);
    assert!(desk.broker.fills().is_empty());
    assert!(desk.holdings().is_empty());

    // 접수 뒤에 체결된 값이면 체결한다.
    let new_trade = quote_full(HYN, "30000", Currency::Krw, Some(at(22)), at(23));
    assert_eq!(
        kinds(&desk.on_quote(&new_trade, at(23))),
        vec![OrderEventKind::Fill]
    );
    assert_eq!(desk.order(&order_id).execution.filled_quantity, d("10"));
}

// 검증: C10
#[test]
fn 거래소_시각이_없는_시세는_받은_시각이_접수_뒤면_체결한다() {
    let desk = Desk::standard();
    let order_id = desk.accepted(&buy(SAM, "1", "70000"), at(10));
    let quote = quote_full(SAM, "69900", Currency::Krw, None, at(11));
    assert_eq!(
        kinds(&desk.on_quote(&quote, at(11))),
        vec![OrderEventKind::Fill]
    );
    assert_eq!(desk.order(&order_id).status, OrderStatus::Filled);
}

// 검증: C10, E29
// 1차 검토의 실험: 한 시세 최대 3주 설정에서 같은 시세를 네 번 주면 3 → 6 → 9 → 10주로 체결됐다.
#[test]
fn 같은_시세를_네_번_줘도_한_번만_체결된다() {
    let desk = Desk::open(per_quote("3"));
    let order_id = desk.accepted(&buy(SAM, "10", "70000"), at(0));
    let quote = quote_full(SAM, "70000", Currency::Krw, None, at(1));

    assert_eq!(
        kinds(&desk.on_quote(&quote, at(1))),
        vec![OrderEventKind::PartialFill]
    );
    for again in 2..=4 {
        let events = desk.on_quote(&quote, at(1));
        assert!(events.is_empty(), "{again}번째: {events:?}");
        assert_eq!(
            desk.order(&order_id).execution.filled_quantity,
            d("3"),
            "{again}번째에도 3주에서 멈춰야 한다"
        );
    }
    assert_eq!(desk.broker.fills().len(), 1);
    assert_eq!(desk.holding(SAM).unwrap().quantity, d("3"));

    // 값이 다른 새 시세가 오면 다시 3주까지 체결한다.
    let next = quote_full(SAM, "69900", Currency::Krw, None, at(2));
    desk.on_quote(&next, at(2));
    assert_eq!(desk.order(&order_id).execution.filled_quantity, d("6"));
}

// 검증: C10, E29
// 주기적으로 조회하는 조립에서는 거래가 없는 종목의 마지막 체결가(같은 거래소 시각 · 같은 값)가
// 받은 시각만 바뀐 채 되풀이 들어온다. "한 시세 최대 수량"이 조회 횟수만큼 풀리면 안 된다.
#[test]
fn 주기적으로_조회해_같은_체결가가_되풀이_들어와도_다시_체결하지_않는다() {
    let desk = Desk::open(per_quote("3"));
    let order_id = desk.accepted(&buy(SAM, "10", "70000"), at(0));

    let trade = |received: i64| quote_full(SAM, "70000", Currency::Krw, Some(at(1)), at(received));
    assert_eq!(
        kinds(&desk.on_quote(&trade(1), at(1))),
        vec![OrderEventKind::PartialFill]
    );
    for received in [6, 11, 16] {
        assert!(desk.on_quote(&trade(received), at(received)).is_empty());
    }
    assert_eq!(desk.order(&order_id).execution.filled_quantity, d("3"));
    assert_eq!(desk.broker.fills().len(), 1);

    // 값은 같아도 거래소 시각이 새로운 체결이면 새 시세다.
    let new_trade = quote_full(SAM, "70000", Currency::Krw, Some(at(17)), at(17));
    assert_eq!(
        kinds(&desk.on_quote(&new_trade, at(17))),
        vec![OrderEventKind::PartialFill]
    );
    assert_eq!(desk.order(&order_id).execution.filled_quantity, d("6"));
}

// 검증: C10
// 지시서는 "같은 종목 · 같은 거래소 시각 · 같은 값"이라고만 한다. 거래소 시각이 없는 시세끼리는
// 값이 같으면 새 체결인지 가릴 수 없어, 보수적으로 같은 시세로 본다(체결하지 않는다).
#[test]
fn 거래소_시각이_없는_시세는_값이_같으면_같은_시세로_본다() {
    let desk = Desk::open(per_quote("3"));
    let untimed =
        |price: &str, received: i64| quote_full(SAM, price, Currency::Krw, None, at(received));
    // 주문이 없을 때 먼저 받은 시세.
    desk.on_quote(&untimed("70000", 0), at(0));
    let order_id = desk.accepted(&buy(SAM, "10", "70000"), at(1));

    // 받은 시각만 새로운 같은 값: 이미 처리한 시세다.
    assert!(desk.on_quote(&untimed("70000", 2), at(2)).is_empty());
    assert!(desk.on_quote(&untimed("70000", 3), at(3)).is_empty());
    assert_eq!(desk.order(&order_id).status, OrderStatus::Pending);

    // 값이 달라지면 새 시세다.
    desk.on_quote(&untimed("69900", 4), at(4));
    assert_eq!(desk.order(&order_id).execution.filled_quantity, d("3"));
    assert!(desk.on_quote(&untimed("69900", 5), at(5)).is_empty());
    assert_eq!(desk.order(&order_id).execution.filled_quantity, d("3"));
}

// 검증: C10, V6
#[test]
fn 되살린_뒤에도_이미_처리한_시세로는_다시_체결하지_않는다() {
    let before = Desk::open(per_quote("3"));
    let order_id = before.accepted(&buy(SAM, "10", "70000"), at(0));
    let trade = |received: i64| quote_full(SAM, "70000", Currency::Krw, Some(at(1)), at(received));
    before.on_quote(&trade(1), at(1));
    assert_eq!(before.order(&order_id).execution.filled_quantity, d("3"));

    let after = before.reopen(per_quote("3"));
    assert!(after.on_quote(&trade(2), at(2)).is_empty());
    assert_eq!(after.order(&order_id).execution.filled_quantity, d("3"));
    assert_eq!(after.broker.fills().len(), 1);

    let new_trade = quote_full(SAM, "70000", Currency::Krw, Some(at(3)), at(3));
    after.on_quote(&new_trade, at(3));
    assert_eq!(after.order(&order_id).execution.filled_quantity, d("6"));
}

// 검증: C10, N6
// 1차 검토의 실험: 받은 시각이 앞날인 시세로도 체결됐다.
#[test]
fn 받은_시각이_지금보다_뒤인_시세로는_체결하지_않는다() {
    let desk = Desk::standard();
    let order_id = desk.accepted(&buy(SAM, "1", "70000"), at(0));

    // 지금은 2초인데 받은 시각이 4초라고 적힌 시세.
    let ahead = quote_full(SAM, "70000", Currency::Krw, Some(at(1)), at(4));
    assert!(desk.on_quote(&ahead, at(2)).is_empty());
    assert_eq!(desk.order(&order_id).status, OrderStatus::Pending);
    assert!(desk.broker.fills().is_empty());
    assert_eq!(desk.krw(), cash("9929989", "70011", "0"));

    // 그 시각이 되어 다시 오면 체결한다(앞서 준 것은 처리하지도 기억하지도 않았다).
    assert_eq!(
        kinds(&desk.on_quote(&ahead, at(4))),
        vec![OrderEventKind::Fill]
    );
}

// 검증: C10, N6
#[test]
fn 받은_시각이_지금보다_뒤인_시세는_시장가가_묶는_돈에도_쓰지_않는다() {
    let desk = Desk::standard();
    let ahead = quote_full(SAM, "50000", Currency::Krw, Some(at(0)), at(100));
    desk.on_quote(&ahead, at(0));
    assert_eq!(
        desk.place(&market_buy(SAM, "1"), at(1)),
        Err(BrokerError::QuoteUnknown)
    );
    assert_eq!(desk.krw(), cash("10000000", "0", "0"));
}

// 검증: N6, C10
// 1차 검토의 실험: 10분 늦은 50,000원 시세만 받은 뒤 시장가가 접수됐다(묶인 돈 50,108원).
#[test]
fn 늦은_시세만_받았으면_시장가를_받지_않는다() {
    let desk = Desk::standard();
    let late = quote_received(SAM, "50000", Currency::Krw, at(-600));
    assert!(desk.on_quote(&late, at(0)).is_empty());

    assert_eq!(
        desk.place(&market_buy(SAM, "1"), at(1)),
        Err(BrokerError::QuoteUnknown)
    );
    assert_eq!(desk.krw(), cash("10000000", "0", "0"));
    assert!(desk.orders(OrderGroup::Open).is_empty());
    assert!(desk.orders(OrderGroup::Closed).is_empty());

    // 늦지 않은 시세를 받으면 그 값으로 묶는다: 70,000 × 1.001 → 70,100, 수수료 10.515 → 11.
    desk.quote(SAM, "70000", at(2));
    desk.accepted(&market_buy(SAM, "1"), at(3));
    assert_eq!(desk.krw(), cash("9929889", "70111", "0"));
}

// 검증: N6
#[test]
fn 늦은_시세는_앞서_기억한_시세를_덮어쓰지_않는다() {
    let desk = Desk::standard();
    desk.quote(SAM, "70000", at(0));
    // 10분 늦은 50,000원 시세가 뒤늦게 들어온다.
    let late = quote_received(SAM, "50000", Currency::Krw, at(-600));
    desk.on_quote(&late, at(1));

    // 묶는 돈은 50,000원이 아니라 70,000원 시세로 셈한다: 70,100 + 11.
    desk.accepted(&market_buy(SAM, "1"), at(2));
    assert_eq!(desk.krw(), cash("9929889", "70111", "0"));
}

// 검증: N6
#[test]
fn 기억한_시세가_주문을_받을_때_늦었으면_시장가를_받지_않는다() {
    // 받은 지 31초: 늦었다(30초를 넘었다).
    let desk = Desk::standard();
    desk.quote(SAM, "70000", at(0));
    assert_eq!(
        desk.place(&market_buy(SAM, "1"), at(31)),
        Err(BrokerError::QuoteUnknown)
    );
    assert_eq!(desk.krw(), cash("10000000", "0", "0"));

    // 받은 지 딱 30초: 아직 늦지 않았다.
    let desk = Desk::standard();
    desk.quote(SAM, "70000", at(0));
    desk.accepted(&market_buy(SAM, "1"), at(30));
    assert_eq!(desk.krw(), cash("9929889", "70111", "0"));

    // 기준은 설정을 따른다: 5초로 줄이면 6초 된 시세는 못 쓴다.
    let mut strict = config();
    strict.stale.max_age = std::time::Duration::from_secs(5);
    let desk = Desk::open(strict);
    desk.quote(SAM, "70000", at(0));
    assert_eq!(
        desk.place(&market_buy(SAM, "1"), at(6)),
        Err(BrokerError::QuoteUnknown)
    );
    desk.quote(SAM, "70000", at(7));
    desk.accepted(&market_buy(SAM, "1"), at(12));
}
