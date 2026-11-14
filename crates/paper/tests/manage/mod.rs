//! 취소 · 정정 · 주문 보기 · 체결 알림. 주문을 내는 쪽은 `Broker` trait만으로 이 모두를 한다.
//! 출처: 토스 명세의 주문 정정 · 취소 · 목록 · 상세 API와 실시간 본인 주문 이벤트.

use pi_market::Currency;
use pi_paper::{
    Broker, BrokerError, CloseReason, InvalidReason, MaxFill, ModifyRequest, OrderEventKind,
    OrderGroup, OrderId, OrderStatus, OrderType,
};

use crate::support::{
    AAPL, Desk, HYN, SAM, at, buy, cash, config, d, kinds, sell, sym, us_at, us_buy,
};

// 검증: C10
#[test]
fn 취소하면_새_식별자를_돌려주고_원주문은_취소됨이_된다() {
    let desk = Desk::standard();
    let order_id = desk.accepted(&buy(SAM, "10", "70000"), at(0));
    let mut events = desk.as_broker().subscribe();

    let cancel_id = desk.cancel(&order_id, at(5)).unwrap();
    assert_ne!(
        cancel_id, order_id,
        "명세: 취소로 새로 발급된 식별자는 원주문과 다르다"
    );

    let order = desk.order(&order_id);
    assert_eq!(order.status, OrderStatus::Canceled);
    assert_eq!(order.close_reason, Some(CloseReason::CancelRequested));
    assert_eq!(order.canceled_at, Some(at(5)));
    assert_eq!(desk.krw(), cash("10000000", "0", "0"));

    let event = events.try_recv().unwrap();
    assert_eq!(event.kind, OrderEventKind::Canceled);
    assert_eq!(event.order, order);
    assert!(events.try_recv().is_err());

    // 취소된 주문은 시세가 와도 체결되지 않는다.
    assert!(desk.quote(SAM, "70000", at(6)).is_empty());
    assert!(desk.holdings().is_empty());
}

// 검증: C10
#[test]
fn 끝난_주문과_없는_주문은_취소할_수_없다() {
    let desk = Desk::standard();
    let filled = desk.accepted(&buy(SAM, "1", "70000"), at(0));
    desk.quote(SAM, "70000", at(1));
    assert_eq!(desk.cancel(&filled, at(2)), Err(BrokerError::AlreadyFilled));

    let canceled = desk.accepted(&buy(SAM, "1", "70000"), at(3));
    desk.cancel(&canceled, at(4)).unwrap();
    assert_eq!(
        desk.cancel(&canceled, at(5)),
        Err(BrokerError::AlreadyCanceled)
    );

    assert_eq!(
        desk.cancel(&OrderId::new("no-such-order"), at(6)),
        Err(BrokerError::OrderNotFound)
    );
}

// 검증: C10, E29
#[test]
fn 일부_체결된_주문을_취소하면_체결된_만큼은_남는다() {
    let mut config = config();
    config.kr.max_fill = MaxFill::PerQuote(d("3"));
    let desk = Desk::open(config);
    let order_id = desk.accepted(&buy(SAM, "10", "70000"), at(0));
    desk.quote(SAM, "70000", at(1));
    desk.cancel(&order_id, at(2)).unwrap();

    let order = desk.order(&order_id);
    assert_eq!(order.status, OrderStatus::Canceled);
    assert_eq!(order.execution.filled_quantity, d("3"));
    assert_eq!(desk.holding(SAM).unwrap().quantity, d("3"));
    assert_eq!(desk.krw(), cash("9789969", "0", "0"));
}

// 검증: C10
#[test]
fn 정정하면_원주문은_정정됨이_되고_새_주문이_자리를_잇는다() {
    let desk = Desk::standard();
    let original_id = desk.accepted(&buy(SAM, "10", "70000"), at(0));
    assert_eq!(desk.krw(), cash("9299895", "700105", "0"));
    let mut events = desk.as_broker().subscribe();

    // 값을 69,500으로. 10주 × 69,500 = 695,000, 수수료 104.25 → 올려서 105. 묶는 돈 695,105.
    let new_id = desk
        .modify(&original_id, Some("10"), Some("69500"), "10", at(5))
        .unwrap();
    assert_ne!(new_id, original_id);

    let original = desk.order(&original_id);
    assert_eq!(original.status, OrderStatus::Replaced);
    assert_eq!(original.close_reason, Some(CloseReason::Replaced));
    assert_eq!(original.price, Some(d("70000")));

    let replacement = desk.order(&new_id);
    assert_eq!(replacement.status, OrderStatus::Pending);
    assert_eq!(replacement.symbol, sym(SAM));
    assert_eq!(replacement.side, original.side);
    assert_eq!(replacement.price, Some(d("69500")));
    assert_eq!(replacement.quantity, d("10"));
    assert_eq!(replacement.ordered_at, at(5));

    assert_eq!(desk.krw(), cash("9304895", "695105", "0"));
    assert_eq!(desk.orders(OrderGroup::Open), vec![replacement.clone()]);
    assert_eq!(desk.orders(OrderGroup::Closed), vec![original.clone()]);

    // 알림: 원주문이 정정됨, 새 주문이 접수.
    let first = events.try_recv().unwrap();
    assert_eq!(first.kind, OrderEventKind::Replaced);
    assert_eq!(first.order, original);
    let second = events.try_recv().unwrap();
    assert_eq!(second.kind, OrderEventKind::Pending);
    assert_eq!(second.order, replacement);

    // 옛 값(70,000)의 시세로는 체결되지 않고, 새 값의 시세로 새 주문이 체결된다.
    assert!(desk.quote(SAM, "70000", at(6)).is_empty());
    desk.quote(SAM, "69500", at(7));
    assert_eq!(desk.order(&new_id).status, OrderStatus::Filled);
    assert_eq!(desk.order(&original_id).execution.filled_quantity, d("0"));
    // 10,000,000 − 695,000 − 104 = 9,304,896.
    assert_eq!(desk.krw(), cash("9304896", "0", "0"));
}

// 검증: C10
#[test]
fn 정정으로_수량을_바꾸면_묶는_돈도_다시_셈한다() {
    let desk = Desk::standard();
    let original_id = desk.accepted(&buy(SAM, "10", "70000"), at(0));
    // 4주 × 70,000 = 280,000 + 42 = 280,042.
    let new_id = desk
        .modify(&original_id, Some("4"), Some("70000"), "4", at(1))
        .unwrap();
    assert_eq!(desk.order(&new_id).quantity, d("4"));
    assert_eq!(desk.krw(), cash("9719958", "280042", "0"));
}

// 검증: C10
#[test]
fn 정정한_주문이_살_돈을_넘으면_받지_않고_원주문은_그대로다() {
    let desk = Desk::open(crate::support::config_with_krw("1000000"));
    let original_id = desk.accepted(&buy(SAM, "10", "70000"), at(0));

    // 15주 × 70,000 = 1,050,000 은 가진 돈(원주문이 묶은 것을 풀어도 1,000,000)을 넘는다.
    assert_eq!(
        desk.modify(&original_id, Some("15"), Some("70000"), "15", at(1)),
        Err(BrokerError::InsufficientBuyingPower)
    );
    assert_eq!(desk.order(&original_id).status, OrderStatus::Pending);
    assert_eq!(desk.krw(), cash("299895", "700105", "0"));
    assert_eq!(desk.orders(OrderGroup::Open).len(), 1);

    // 14주 × 70,000 = 980,000 + 147 = 980,147 은 원주문이 묶은 것을 풀면 된다.
    desk.modify(&original_id, Some("14"), Some("70000"), "14", at(2))
        .unwrap();
    assert_eq!(desk.krw(), cash("19853", "980147", "0"));
}

// 검증: C10, E33
#[test]
fn 정정도_접수와_같은_확인을_거친다() {
    let desk = Desk::standard();
    let original_id = desk.accepted(&buy(SAM, "10", "70000"), at(0));

    // 표시 없음.
    let without_permit = ModifyRequest {
        order_type: OrderType::Limit,
        quantity: Some(d("10")),
        price: Some(d("69500")),
        confirm_high_value: false,
        permit: None,
    };
    assert_eq!(
        desk.modify_raw(&original_id, without_permit, at(1)),
        Err(BrokerError::CheckRequired)
    );
    // 표시가 승인한 수량(10)과 정정 수량(20)이 다름.
    assert_eq!(
        desk.modify(&original_id, Some("20"), Some("69500"), "10", at(1)),
        Err(BrokerError::CheckMismatch)
    );
    // 국내는 수량이 꼭 있어야 한다(명세).
    assert_eq!(
        desk.modify(&original_id, None, Some("69500"), "10", at(1)),
        Err(BrokerError::InvalidRequest(
            InvalidReason::ModifyQuantityRequired
        ))
    );
    // 호가 단위.
    assert_eq!(
        desk.modify(&original_id, Some("10"), Some("69550"), "10", at(1)),
        Err(BrokerError::InvalidRequest(InvalidReason::TickSize {
            tick: d("100")
        }))
    );
    // 하한가(49,000) 아래.
    assert_eq!(
        desk.modify(&original_id, Some("10"), Some("48950"), "10", at(1)),
        Err(BrokerError::PriceOutOfRange)
    );

    // 받지 않은 정정은 아무것도 바꾸지 않는다.
    let original = desk.order(&original_id);
    assert_eq!(original.status, OrderStatus::Pending);
    assert_eq!(original.price, Some(d("70000")));
    assert_eq!(desk.krw(), cash("9299895", "700105", "0"));
    assert_eq!(desk.orders(OrderGroup::Open).len(), 1);
    assert!(desk.orders(OrderGroup::Closed).is_empty());
}

// 검증: C10
#[test]
fn 끝난_주문과_없는_주문은_정정할_수_없다() {
    let desk = Desk::standard();
    let filled = desk.accepted(&buy(SAM, "1", "70000"), at(0));
    desk.quote(SAM, "70000", at(1));
    assert_eq!(
        desk.modify(&filled, Some("1"), Some("69500"), "1", at(2)),
        Err(BrokerError::AlreadyFilled)
    );

    let canceled = desk.accepted(&buy(HYN, "1", "30000"), at(3));
    desk.cancel(&canceled, at(4)).unwrap();
    assert_eq!(
        desk.modify(&canceled, Some("1"), Some("29950"), "1", at(5)),
        Err(BrokerError::AlreadyCanceled)
    );

    let replaced = desk.accepted(&buy(HYN, "1", "30000"), at(6));
    desk.modify(&replaced, Some("1"), Some("29950"), "1", at(7))
        .unwrap();
    assert_eq!(
        desk.modify(&replaced, Some("1"), Some("29900"), "1", at(8)),
        Err(BrokerError::AlreadyModified)
    );
    assert_eq!(
        desk.cancel(&replaced, at(8)),
        Err(BrokerError::AlreadyModified)
    );

    let missing = ModifyRequest {
        order_type: OrderType::Limit,
        quantity: Some(d("1")),
        price: Some(d("70000")),
        confirm_high_value: false,
        permit: None,
    };
    assert_eq!(
        desk.modify_raw(&OrderId::new("no-such-order"), missing, at(9)),
        Err(BrokerError::OrderNotFound)
    );
}

// 검증: C10
// 출처: 토스 명세의 주문 정정 요청(미국 주식은 수량을 줄 수 없고 값만 바꾼다).
#[test]
fn 미국_주식_정정은_값만_바꿀_수_있다() {
    let desk = Desk::standard();
    let original_id = desk.accepted(&us_buy(AAPL, "10", "150.25"), us_at(0));

    assert_eq!(
        desk.modify(&original_id, Some("5"), Some("150.00"), "5", us_at(1)),
        Err(BrokerError::UsModifyQuantityNotSupported)
    );
    assert_eq!(desk.order(&original_id).status, OrderStatus::Pending);

    // 수량 없이 값만: 새 주문의 수량은 원주문의 남은 수량(10) 그대로다.
    // 10주 × 150.00 = 1,500.00, 수수료 1.50. 묶는 돈 1,501.50.
    let new_id = desk
        .modify(&original_id, None, Some("150.00"), "10", us_at(2))
        .unwrap();
    let replacement = desk.order(&new_id);
    assert_eq!(replacement.quantity, d("10"));
    assert_eq!(replacement.price, Some(d("150.00")));
    assert_eq!(replacement.currency, Currency::Usd);
    assert_eq!(desk.order(&original_id).status, OrderStatus::Replaced);
    assert_eq!(desk.usd(), cash("8498.50", "1501.50", "0"));
}

// 검증: C10
#[test]
fn 주문_목록은_진행_중과_끝난_것을_나누고_종목으로_거른다() {
    let desk = Desk::standard();
    let filled = desk.accepted(&buy(SAM, "1", "70000"), at(0));
    desk.quote(SAM, "70000", at(1));
    let canceled = desk.accepted(&buy(HYN, "1", "30000"), at(2));
    desk.cancel(&canceled, at(3)).unwrap();
    let open_sam = desk.accepted(&sell(SAM, "1", "71000"), at(4));
    let open_hyn = desk.accepted(&buy(HYN, "2", "29950"), at(5));

    let ids = |orders: Vec<pi_paper::Order>| -> Vec<OrderId> {
        orders.into_iter().map(|order| order.order_id).collect()
    };
    let broker = desk.as_broker();
    let list = |group, code: Option<&str>| {
        let symbol = code.map(sym);
        ids(desk
            .run(broker.orders(group, symbol.as_ref(), at(5)))
            .unwrap())
    };

    // 접수한 순서대로.
    assert_eq!(
        list(OrderGroup::Open, None),
        vec![open_sam.clone(), open_hyn.clone()]
    );
    assert_eq!(
        list(OrderGroup::Closed, None),
        vec![filled.clone(), canceled.clone()]
    );
    assert_eq!(list(OrderGroup::Open, Some(SAM)), vec![open_sam]);
    assert_eq!(list(OrderGroup::Open, Some(HYN)), vec![open_hyn]);
    assert_eq!(list(OrderGroup::Closed, Some(SAM)), vec![filled]);
    assert_eq!(list(OrderGroup::Closed, Some(HYN)), vec![canceled]);
    assert!(list(OrderGroup::Open, Some(AAPL)).is_empty());
}

// 검증: C10
#[test]
fn 없는_주문의_상세는_찾을_수_없다고_알려_준다() {
    let desk = Desk::standard();
    assert_eq!(
        desk.run(
            desk.as_broker()
                .order(&OrderId::new("no-such-order"), at(0))
        ),
        Err(BrokerError::OrderNotFound)
    );
}

// 검증: C10
#[test]
fn 체결_알림은_상태가_바뀔_때마다_빠짐없이_순서대로_온다() {
    let mut config = config();
    config.kr.max_fill = MaxFill::PerQuote(d("3"));
    let desk = Desk::open(config);
    let mut first = desk.as_broker().subscribe();
    let mut second = desk.as_broker().subscribe();

    let order_id = desk.accepted(&buy(SAM, "5", "70000"), at(0));
    let pending = first.try_recv().unwrap();
    assert_eq!(pending.kind, OrderEventKind::Pending);
    assert_eq!(pending.order.order_id, order_id);
    assert_eq!(pending.order.status, OrderStatus::Pending);

    let returned_partial = desk.quote(SAM, "70000", at(1));
    let returned_fill = desk.quote(SAM, "70000", at(2));
    assert_eq!(kinds(&returned_partial), vec![OrderEventKind::PartialFill]);
    assert_eq!(kinds(&returned_fill), vec![OrderEventKind::Fill]);

    // 구독자는 시세 처리의 반환값과 같은 이벤트를 받는다.
    let partial = first.try_recv().unwrap();
    assert_eq!(partial, returned_partial[0]);
    assert_eq!(partial.order.status, OrderStatus::PartialFilled);
    assert_eq!(partial.order.execution.filled_quantity, d("3"));
    let fill = first.try_recv().unwrap();
    assert_eq!(fill, returned_fill[0]);
    assert_eq!(fill.order.status, OrderStatus::Filled);
    assert!(first.try_recv().is_err(), "더 온 것이 없다");

    // 둘째 구독자도 같은 것을 같은 순서로 받는다.
    let seen: Vec<OrderEventKind> = std::iter::from_fn(|| second.try_recv().ok())
        .map(|event| event.kind)
        .collect();
    assert_eq!(
        seen,
        vec![
            OrderEventKind::Pending,
            OrderEventKind::PartialFill,
            OrderEventKind::Fill
        ]
    );

    // 받지 않은 주문은 알림이 없다.
    let mut third = desk.as_broker().subscribe();
    assert!(desk.place(&buy(SAM, "1", "70050"), at(3)).is_err());
    assert!(third.try_recv().is_err());
}

/// 주문을 내는 쪽의 코드: 증권사 trait만 안다.
async fn buy_and_report(broker: &dyn Broker, desk: &Desk) -> (OrderStatus, rust_decimal::Decimal) {
    let req = buy(SAM, "2", "70000");
    let permit = desk.authority.approve(req.approved());
    let ack = broker
        .place_order(req.request(Some(permit)), at(0))
        .await
        .unwrap();
    let order = broker.order(&ack.order_id, at(0)).await.unwrap();
    let power = broker.buying_power(Currency::Krw, at(0)).await.unwrap();
    (order.status, power)
}

// 검증: C10
#[test]
fn 주문을_내는_쪽은_증권사_trait_하나로_모의계좌를_쓴다() {
    let desk = Desk::standard();
    let broker: &dyn Broker = &desk.broker;
    let (status, power) = desk.run(buy_and_report(broker, &desk));
    assert_eq!(status, OrderStatus::Pending);
    // 2주 × 70,000 = 140,000 + 21.
    assert_eq!(power, d("9859979"));
}
