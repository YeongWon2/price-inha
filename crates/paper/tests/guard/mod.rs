//! 문지기: 점검 통과 표시(E33), 계좌 종류(E25 · V29), 장애 흉내(E13 · E30), 멱등성 키.

use pi_paper::{
    AccountKind, ApprovedOrder, BrokerError, Fault, IDEMPOTENCY_WINDOW, OrderGroup, OrderStatus,
    OrderType, Side, UnfilledTimeout,
};

use crate::support::{Desk, HYN, SAM, at, buy, cash, config, d, sell, sym};

fn nothing_happened(desk: &Desk) {
    assert!(desk.orders(OrderGroup::Open).is_empty());
    assert!(desk.orders(OrderGroup::Closed).is_empty());
    assert_eq!(desk.krw(), cash("10000000", "0", "0"));
    assert!(desk.broker.fills().is_empty());
}

// 검증: E33
#[test]
fn 점검_통과_표시가_없는_주문은_받지_않는다() {
    let desk = Desk::standard();
    let request = buy(SAM, "1", "70000").request(None);
    assert_eq!(
        desk.place_raw(request, at(0)),
        Err(BrokerError::CheckRequired)
    );
    nothing_happened(&desk);
}

// 검증: E33
#[test]
fn 다른_계좌의_권한이_낸_표시는_받지_않는다() {
    let desk = Desk::standard();
    let stranger = Desk::standard();
    let req = buy(SAM, "1", "70000");
    let foreign = stranger.authority.approve(req.approved());
    assert_eq!(
        desk.place_raw(req.request(Some(foreign)), at(0)),
        Err(BrokerError::CheckInvalid)
    );
    nothing_happened(&desk);
}

// 검증: E33
#[test]
fn 표시가_승인한_내용과_다른_주문은_받지_않는다() {
    let desk = Desk::standard();
    let approved = buy(SAM, "1", "70000");
    let mismatch = |change: fn(&mut crate::support::Req)| {
        let mut req = approved.clone();
        change(&mut req);
        let permit = desk.authority.approve(approved.approved());
        desk.place_raw(req.request(Some(permit)), at(0))
    };

    // 1주를 승인받은 표시로 10주를 낼 수 없다.
    assert_eq!(
        mismatch(|req| req.quantity = d("10")),
        Err(BrokerError::CheckMismatch)
    );
    assert_eq!(
        mismatch(|req| req.price = Some(d("70100"))),
        Err(BrokerError::CheckMismatch)
    );
    assert_eq!(
        mismatch(|req| req.symbol = sym(HYN)),
        Err(BrokerError::CheckMismatch)
    );
    assert_eq!(
        mismatch(|req| req.side = Side::Sell),
        Err(BrokerError::CheckMismatch)
    );
    assert_eq!(
        mismatch(|req| {
            req.order_type = OrderType::Market;
            req.price = None;
        }),
        Err(BrokerError::CheckMismatch)
    );
    nothing_happened(&desk);

    // 내용이 모두 같으면 받는다.
    assert!(mismatch(|_| {}).is_ok());
}

// 검증: E33
#[test]
fn 표시는_승인한_내용을_그대로_담는다() {
    let desk = Desk::standard();
    let approved = ApprovedOrder {
        market: pi_market::Market::Kr,
        symbol: sym(SAM),
        side: Side::Buy,
        order_type: OrderType::Limit,
        quantity: d("3"),
        price: Some(d("70000")),
        account_kind: AccountKind::Paper,
    };
    let permit = desk.authority.approve(approved.clone());
    assert_eq!(permit.approved(), &approved);
}

// 검증: E25, V29
#[test]
fn 실전_계좌로_가려는_주문은_모의계좌가_받지_않는다() {
    let desk = Desk::standard();
    assert_eq!(desk.as_broker().account_kind(), AccountKind::Paper);

    // 표시까지 실전으로 승인받았어도, 이 증권사 구현은 모의계좌라서 받지 않는다.
    let mut live = buy(SAM, "1", "70000");
    live.account_kind = AccountKind::Live;
    assert_eq!(
        desk.place(&live, at(0)),
        Err(BrokerError::AccountKindMismatch {
            order: AccountKind::Live,
            broker: AccountKind::Paper,
        })
    );
    nothing_happened(&desk);
}

// 검증: E25
#[test]
fn 계좌_종류는_표시보다_먼저_본다() {
    let desk = Desk::standard();
    let mut live = buy(SAM, "1", "70000");
    live.account_kind = AccountKind::Live;
    // 표시가 없어도 이유는 "계좌 종류가 다름"이다.
    assert_eq!(
        desk.place_raw(live.request(None), at(0)),
        Err(BrokerError::AccountKindMismatch {
            order: AccountKind::Live,
            broker: AccountKind::Paper,
        })
    );
}

// 검증: E13
#[test]
fn 증권사가_주문을_받지_않는_장애를_일으킬_수_있다() {
    let desk = Desk::standard();
    desk.broker.set_fault(Some(Fault::RejectOrders));
    assert_eq!(
        desk.place(&buy(SAM, "1", "70000"), at(0)),
        Err(BrokerError::Unavailable)
    );
    nothing_happened(&desk);

    desk.broker.set_fault(None);
    desk.place(&buy(SAM, "1", "70000"), at(1)).unwrap();
}

// 검증: E13
#[test]
fn 증권사가_응답하지_않는_장애를_일으킬_수_있다() {
    let desk = Desk::standard();
    desk.broker.set_fault(Some(Fault::NoResponse));
    assert_eq!(
        desk.place(&buy(SAM, "1", "70000"), at(0)),
        Err(BrokerError::NoResponse)
    );
    nothing_happened(&desk);
}

// 검증: E13
#[test]
fn 장애_중에도_이미_있는_주문과_조회는_그대로다() {
    let desk = Desk::standard();
    let order_id = desk.accepted(&buy(SAM, "2", "70000"), at(0));
    desk.broker.set_fault(Some(Fault::RejectOrders));

    assert_eq!(desk.cancel(&order_id, at(1)), Err(BrokerError::Unavailable));
    assert_eq!(
        desk.modify(&order_id, Some("2"), Some("69900"), "2", at(1)),
        Err(BrokerError::Unavailable)
    );
    assert_eq!(desk.order(&order_id).status, OrderStatus::Pending);
    assert_eq!(desk.krw(), cash("9859979", "140021", "0"));

    // 시세는 평소대로 처리된다: 이미 접수된 주문은 체결된다.
    desk.quote(SAM, "70000", at(2));
    assert_eq!(desk.order(&order_id).status, OrderStatus::Filled);
}

// 검증: E30
#[test]
fn 파는_주문이_실패해도_보유는_그대로이고_다시_내면_한_번만_접수된다() {
    let desk = Desk::standard();
    desk.own(SAM, "10", "70000", at(0));
    let stop_loss = sell(SAM, "10", "68000")
        .key("stop-loss-1")
        .timeout(UnfilledTimeout::LONG_TERM);

    // 증권사가 받지 않는 동안 계속 다시 낸다. 몇 번을 내도 아무것도 바뀌지 않는다.
    desk.broker.set_fault(Some(Fault::RejectOrders));
    for attempt in 1..=3 {
        assert_eq!(
            desk.place(&stop_loss, at(attempt)),
            Err(BrokerError::Unavailable)
        );
    }
    desk.broker.set_fault(Some(Fault::NoResponse));
    assert_eq!(desk.place(&stop_loss, at(4)), Err(BrokerError::NoResponse));
    assert_eq!(desk.sellable(SAM), d("10"));
    assert_eq!(desk.holding(SAM).unwrap().quantity, d("10"));
    assert!(desk.orders(OrderGroup::Open).is_empty());

    // 장애가 풀리면 받는다. 같은 키로 한 번 더 내도 주문은 하나다.
    desk.broker.set_fault(None);
    let first = desk.place(&stop_loss, at(5)).unwrap();
    let again = desk.place(&stop_loss, at(6)).unwrap();
    assert_eq!(first, again);
    assert_eq!(first.client_order_id.as_deref(), Some("stop-loss-1"));
    assert_eq!(desk.orders(OrderGroup::Open).len(), 1);
    assert_eq!(desk.sellable(SAM), d("0"));

    desk.quote(SAM, "68000", at(7));
    assert_eq!(desk.order(&first.order_id).status, OrderStatus::Filled);
    assert!(desk.holding(SAM).is_none());
}

// 검증: C10
// 출처: 토스 명세의 clientOrderId 설명(같은 값으로 다시 요청하면 이전 결과를 그대로 돌려준다, 10분).
#[test]
fn 같은_키와_같은_내용으로_다시_내면_새_주문을_만들지_않는다() {
    let desk = Desk::standard();
    let req = buy(SAM, "10", "70000").key("order-001");
    let first = desk.place(&req, at(0)).unwrap();
    let second = desk.place(&req, at(599)).unwrap();
    assert_eq!(first, second);
    assert_eq!(desk.orders(OrderGroup::Open).len(), 1);
    assert_eq!(
        desk.krw(),
        cash("9299895", "700105", "0"),
        "돈은 한 번만 묶인다"
    );
    assert_eq!(
        desk.order(&first.order_id).client_order_id.as_deref(),
        Some("order-001")
    );
}

// 검증: C10
#[test]
fn 같은_키로_다른_내용을_내면_받지_않는다() {
    let desk = Desk::standard();
    desk.place(&buy(SAM, "10", "70000").key("order-001"), at(0))
        .unwrap();
    assert_eq!(
        desk.place(&buy(SAM, "9", "70000").key("order-001"), at(1)),
        Err(BrokerError::IdempotencyKeyConflict)
    );
    assert_eq!(desk.orders(OrderGroup::Open).len(), 1);
    // 키가 다르면 내용이 같아도 별개의 주문이다.
    desk.place(&buy(SAM, "10", "70000").key("order-002"), at(2))
        .unwrap();
    assert_eq!(desk.orders(OrderGroup::Open).len(), 2);
}

// 검증: C10
#[test]
fn 멱등성_키는_10분이_되면_새_주문으로_본다() {
    assert_eq!(IDEMPOTENCY_WINDOW, std::time::Duration::from_secs(600));
    let desk = Desk::open(config());
    let req = buy(SAM, "1", "70000").key("order-001");
    let first = desk.place(&req, at(0)).unwrap();
    let later = desk.place(&req, at(600)).unwrap();
    assert_ne!(first.order_id, later.order_id);
    assert_eq!(desk.orders(OrderGroup::Open).len(), 2);
}

// 검증: C10
#[test]
fn 키가_없는_주문은_내용이_같아도_매번_새_주문이다() {
    let desk = Desk::standard();
    let first = desk.accepted(&buy(SAM, "1", "70000"), at(0));
    let second = desk.accepted(&buy(SAM, "1", "70000"), at(0));
    assert_ne!(first, second);
    assert_eq!(desk.orders(OrderGroup::Open).len(), 2);
}
