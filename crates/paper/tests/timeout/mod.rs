//! 미체결 처리 (E12 · E29 · N4): 정한 시간이 되면 남은 주문을 취소하고 이유를 남긴다.

use pi_market::Currency;
use pi_paper::{
    BrokerError, CloseReason, MaxFill, Order, OrderEventKind, OrderGroup, OrderStatus,
    UnfilledTimeout,
};

use crate::support::{Desk, HYN, SAM, at, buy, cash, config, d, kinds, monday, sell, utc};

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

// ---------------------------------------------------------------------------------------------
// 3차 5번: 마감 취소 시각은 마감 시각이다. 조회가 먼저 알아챘든 시간 알림이 알아챘든 같다.
// ---------------------------------------------------------------------------------------------

/// 0초에 단타(1분)로 낸 삼성 1주 사기가 있는 계좌. 마감은 60초다.
fn scalp_desk() -> (Desk, pi_paper::OrderId) {
    let desk = Desk::standard();
    let order_id = desk.accepted(
        &buy(SAM, "1", "70000").timeout(UnfilledTimeout::SCALP),
        at(0),
    );
    (desk, order_id)
}

/// 마감이 지난 100초에 저마다 다른 호출로 처음 알아채게 한 뒤의 주문.
fn noticed_by(how: &str) -> Order {
    let (desk, order_id) = scalp_desk();
    let late = at(100);
    match how {
        "시간 알림" => {
            desk.on_time(late);
        }
        "주문 상세" => {
            desk.order_at(&order_id, late);
        }
        "주문 목록" => {
            desk.orders_at(OrderGroup::Closed, late);
        }
        "현금" => {
            desk.krw_at(late);
        }
        "매수 가능 금액" => {
            desk.buying_power_at(Currency::Krw, late);
        }
        "보유" => {
            desk.holdings_at(late);
        }
        "다른 종목의 시세" => {
            desk.quote(HYN, "30000", late);
        }
        "다른 주문 내기" => {
            desk.accepted(&buy(HYN, "1", "30000"), late);
        }
        "취소" => {
            assert_eq!(
                desk.cancel(&order_id, late),
                Err(BrokerError::AlreadyCanceled)
            );
        }
        _ => unreachable!(),
    }
    desk.order_at(&order_id, late)
}

// 검증: E12, E29, N4
// 2차 검토의 참고: 마감(60초)이 지난 뒤 처음 부른 호출의 시각이 취소 시각이 되어, 무엇이 먼저
// 알아챘느냐에 따라 같은 주문의 취소 시각이 달랐다.
#[test]
fn 마감이_지난_주문은_어느_호출이_알아채도_취소_시각이_마감_시각이다() {
    let ways = [
        "시간 알림",
        "주문 상세",
        "주문 목록",
        "현금",
        "매수 가능 금액",
        "보유",
        "다른 종목의 시세",
        "다른 주문 내기",
        "취소",
    ];
    let reference = noticed_by(ways[0]);
    assert_eq!(reference.status, OrderStatus::Canceled);
    assert_eq!(reference.close_reason, Some(CloseReason::UnfilledTimeout));
    assert_eq!(reference.canceled_at, Some(at(60)));
    for how in ways {
        let order = noticed_by(how);
        assert_eq!(order, reference, "{how}가 알아챈 경우");
    }
}

// 검증: E12, N4
#[test]
fn 조회가_먼저_알아챈_마감_취소도_구독으로_마감_시각이_적힌_알림이_온다() {
    let (desk, order_id) = scalp_desk();
    let mut events = desk.as_broker().subscribe();
    let order = desk.order_at(&order_id, at(100));
    assert_eq!(order.canceled_at, Some(at(60)));

    let event = events.try_recv().unwrap();
    assert_eq!(event.kind, OrderEventKind::Canceled);
    assert_eq!(event.order, order);
}

// 검증: E12, E29, N4
#[test]
fn 일부_체결된_주문도_장_마감_주문도_취소_시각은_마감_시각이다() {
    // 일부 체결: 10주 가운데 3주가 체결된 단타 주문을 마감 뒤 2분에 처음 조회한다.
    let mut per_quote = config();
    per_quote.kr.max_fill = MaxFill::PerQuote(d("3"));
    let desk = Desk::open(per_quote);
    let order_id = desk.accepted(
        &buy(SAM, "10", "70000").timeout(UnfilledTimeout::SCALP),
        at(0),
    );
    desk.quote(SAM, "70000", at(1));
    let order = desk.order_at(&order_id, at(180));
    assert_eq!(order.status, OrderStatus::Canceled);
    assert_eq!(order.execution.filled_quantity, d("3"));
    assert_eq!(order.canceled_at, Some(at(60)));

    // 장 마감: 중장기 주문을 정규장이 끝나고(06:30 UTC) 한 시간 뒤에 처음 조회한다.
    let desk = Desk::standard();
    let order_id = desk.accepted(
        &buy(SAM, "1", "70000").timeout(UnfilledTimeout::LONG_TERM),
        at(0),
    );
    let order = desk.order_at(&order_id, utc(monday(), 7, 30, 0));
    assert_eq!(order.close_reason, Some(CloseReason::UnfilledTimeout));
    assert_eq!(order.canceled_at, Some(utc(monday(), 6, 30, 0)));
}
