//! 미체결 처리 (E12 · E29 · N4): 정한 시간이 되면 남은 주문을 취소하고 이유를 남긴다.

use pi_paper::{
    BrokerError, CloseReason, MaxFill, OrderEventKind, OrderGroup, OrderStatus, UnfilledTimeout,
};

use crate::support::{Desk, SAM, at, buy, cash, config, d, kinds, monday, sell, utc};

// 검증: E12, N4
#[test]
fn 단타_주문은_1분이_되면_취소되고_이유가_남는다() {
    let desk = Desk::standard();
    let order_id = desk.accepted(
        &buy(SAM, "10", "70000").timeout(UnfilledTimeout::SCALP),
        at(0),
    );

    assert!(desk.on_time(at(59)).is_empty());
    assert_eq!(desk.order(&order_id).status, OrderStatus::Pending);
    assert_eq!(desk.krw(), cash("9299895", "700105", "0"));

    let events = desk.on_time(at(60));
    assert_eq!(kinds(&events), vec![OrderEventKind::Canceled]);
    let order = desk.order(&order_id);
    assert_eq!(events[0].order, order);
    assert_eq!(order.status, OrderStatus::Canceled);
    assert_eq!(order.close_reason, Some(CloseReason::UnfilledTimeout));
    assert_eq!(order.canceled_at, Some(at(60)));
    assert_eq!(order.execution.filled_quantity, d("0"));

    // 묶인 돈이 풀리고 보유는 생기지 않는다.
    assert_eq!(desk.krw(), cash("10000000", "0", "0"));
    assert!(desk.holdings().is_empty());
    assert!(desk.orders(OrderGroup::Open).is_empty());
    assert_eq!(desk.orders(OrderGroup::Closed), vec![order]);

    // 한 번 취소된 주문은 다시 취소되지 않는다.
    assert!(desk.on_time(at(61)).is_empty());
}

// 검증: E12, N4
#[test]
fn 스윙_주문은_10분이_되면_취소된다() {
    let desk = Desk::standard();
    let order_id = desk.accepted(
        &buy(SAM, "1", "70000").timeout(UnfilledTimeout::SWING),
        at(0),
    );
    assert!(desk.on_time(at(599)).is_empty());
    assert_eq!(desk.order(&order_id).status, OrderStatus::Pending);
    assert_eq!(
        kinds(&desk.on_time(at(600))),
        vec![OrderEventKind::Canceled]
    );
    assert_eq!(
        desk.order(&order_id).close_reason,
        Some(CloseReason::UnfilledTimeout)
    );
}

// 검증: E12, N4
#[test]
fn 중장기_주문은_장_마감에_취소된다() {
    let desk = Desk::standard();
    let order_id = desk.accepted(
        &buy(SAM, "1", "70000").timeout(UnfilledTimeout::LONG_TERM),
        at(0),
    );
    // 정규장은 06:30 UTC(15:30 KST)에 끝난다.
    assert!(desk.on_time(utc(monday(), 6, 29, 59)).is_empty());
    assert_eq!(desk.order(&order_id).status, OrderStatus::Pending);

    let close = utc(monday(), 6, 30, 0);
    assert_eq!(kinds(&desk.on_time(close)), vec![OrderEventKind::Canceled]);
    let order = desk.order(&order_id);
    assert_eq!(order.status, OrderStatus::Canceled);
    assert_eq!(order.close_reason, Some(CloseReason::UnfilledTimeout));
    assert_eq!(order.canceled_at, Some(close));
}

// 검증: N4
#[test]
fn 취소_시간은_주문마다_준_값을_따른다() {
    let desk = Desk::standard();
    let custom = UnfilledTimeout::After(std::time::Duration::from_secs(5));
    let short = desk.accepted(&buy(SAM, "1", "70000").timeout(custom), at(0));
    let long = desk.accepted(
        &buy(SAM, "1", "70000").timeout(UnfilledTimeout::SWING),
        at(0),
    );

    assert!(desk.on_time(at(4)).is_empty());
    let events = desk.on_time(at(5));
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].order.order_id, short);
    assert_eq!(desk.order(&long).status, OrderStatus::Pending);
}

// 검증: E29, E12
#[test]
fn 일부만_체결되면_남은_부분만_취소하고_체결된_만큼은_보유로_남는다() {
    let mut config = config();
    config.kr.max_fill = MaxFill::PerQuote(d("3"));
    let desk = Desk::open(config);
    let order_id = desk.accepted(
        &buy(SAM, "10", "70000").timeout(UnfilledTimeout::SCALP),
        at(0),
    );
    desk.quote(SAM, "70000", at(10));
    assert_eq!(desk.order(&order_id).status, OrderStatus::PartialFilled);

    let events = desk.on_time(at(60));
    assert_eq!(kinds(&events), vec![OrderEventKind::Canceled]);
    let order = desk.order(&order_id);
    assert_eq!(order.status, OrderStatus::Canceled);
    assert_eq!(order.close_reason, Some(CloseReason::UnfilledTimeout));
    assert_eq!(order.quantity, d("10"));
    assert_eq!(order.execution.filled_quantity, d("3"));
    assert_eq!(order.execution.filled_amount, d("210000"));
    assert_eq!(order.execution.commission, d("31"));

    // 체결된 3주만 보유다(이 수량으로 손절값을 건다). 남은 7주에 묶였던 돈은 풀린다.
    let holding = desk.holding(SAM).unwrap();
    assert_eq!(holding.quantity, d("3"));
    assert_eq!(holding.average_purchase_price, d("70000"));
    assert_eq!(desk.sellable(SAM), d("3"));
    assert_eq!(desk.krw(), cash("9789969", "0", "0"));
    assert_eq!(desk.broker.fills().len(), 1);
}

// 검증: E12
#[test]
fn 마감_시각에_온_시세로는_체결하지_않고_취소한다() {
    let desk = Desk::standard();
    let order_id = desk.accepted(
        &buy(SAM, "1", "70000").timeout(UnfilledTimeout::SCALP),
        at(0),
    );
    let events = desk.quote(SAM, "70000", at(60));
    assert_eq!(kinds(&events), vec![OrderEventKind::Canceled]);
    assert_eq!(desk.order(&order_id).status, OrderStatus::Canceled);
    assert!(desk.broker.fills().is_empty());
    assert_eq!(desk.krw(), cash("10000000", "0", "0"));
}

// 검증: E12
#[test]
fn 마감_전에_온_시세로는_체결한다() {
    let desk = Desk::standard();
    let order_id = desk.accepted(
        &buy(SAM, "1", "70000").timeout(UnfilledTimeout::SCALP),
        at(0),
    );
    desk.quote(SAM, "70000", at(59));
    assert_eq!(desk.order(&order_id).status, OrderStatus::Filled);
    // 다 체결된 주문은 마감이 지나도 그대로다.
    assert!(desk.on_time(at(60)).is_empty());
    assert_eq!(desk.order(&order_id).status, OrderStatus::Filled);
    assert_eq!(desk.order(&order_id).close_reason, None);
}

// 검증: E12
#[test]
fn 파는_주문이_취소되면_묶인_수량이_풀린다() {
    let desk = Desk::standard();
    desk.own(SAM, "10", "70000", at(0));
    let order_id = desk.accepted(
        &sell(SAM, "10", "75000").timeout(UnfilledTimeout::SCALP),
        at(0),
    );
    assert_eq!(desk.sellable(SAM), d("0"));
    desk.on_time(at(60));
    assert_eq!(desk.order(&order_id).status, OrderStatus::Canceled);
    assert_eq!(desk.sellable(SAM), d("10"));
    assert_eq!(desk.holding(SAM).unwrap().quantity, d("10"));
}

// 검증: E12
#[test]
fn 시간이_지나_취소된_주문은_다시_취소할_수_없다() {
    let desk = Desk::standard();
    let order_id = desk.accepted(
        &buy(SAM, "1", "70000").timeout(UnfilledTimeout::SCALP),
        at(0),
    );
    desk.on_time(at(60));
    assert_eq!(
        desk.cancel(&order_id, at(61)),
        Err(BrokerError::AlreadyCanceled)
    );
}
