//! 상태 내보내기와 되살리기 (V6): 프로그램을 껐다 켜도 꺼지기 전 상태와 보유가 그대로 이어진다.
//! 저장은 조립하는 쪽이 한다. 여기서는 값을 글자(JSON)로 바꿨다가 되돌려 되살린다.

use pi_market::Currency;
use pi_paper::{
    BrokerError, CloseReason, MaxFill, OrderEventKind, OrderGroup, OrderStatus, PaperBroker,
    Snapshot, UnfilledTimeout,
};

use crate::support::{
    AAPL, Desk, HYN, SAM, at, buy, cash, config, d, day, kinds, kr_calendar, market_buy, sell,
    us_at, us_buy, utc,
};

/// 여러 상태가 섞인 계좌: 보유, 결제 전인 돈, 일부 체결된 열린 주문, 묶인 수량, 취소된 주문, 달러 주문.
fn busy_desk() -> Desk {
    let mut config = config();
    config.kr.max_fill = MaxFill::PerQuote(d("3"));
    let desk = Desk::open(config);
    // 달러 사기는 금요일 미국 정규장에 낸다(시각이 뒤로 가지 않게). 일주일 동안 열어 둔다.
    let week = UnfilledTimeout::After(std::time::Duration::from_secs(7 * 24 * 3600));
    desk.accepted(
        &us_buy(AAPL, "10", "150.25").timeout(week),
        utc(day(2026, 11, 13), 15, 0, 0),
    );
    desk.own(SAM, "3", "70000", at(0));
    desk.own(SAM, "3", "70000", at(1));
    desk.accepted(&sell(SAM, "2", "70000"), at(2));
    desk.quote(SAM, "70000", at(3));
    let canceled = desk.accepted(&buy(HYN, "1", "30000"), at(4));
    desk.cancel(&canceled, at(5)).unwrap();
    // 열린 주문들(달러 사기에 더해): 일부 체결된 사기(단타), 묶인 팔기.
    desk.accepted(
        &buy(HYN, "10", "30000")
            .timeout(UnfilledTimeout::SCALP)
            .key("hyn-1"),
        at(6),
    );
    desk.quote(HYN, "30000", at(7));
    desk.accepted(&sell(SAM, "4", "71000"), at(8));
    desk
}

fn busy_config() -> pi_paper::PaperConfig {
    let mut config = config();
    config.kr.max_fill = MaxFill::PerQuote(d("3"));
    config
}

// 검증: V6
#[test]
fn 내보낸_값에서_되살리면_현금_보유_주문_체결_기록이_그대로다() {
    let before = busy_desk();
    let after = before.reopen(busy_config());

    assert_eq!(after.krw(), before.krw());
    assert_eq!(after.usd(), before.usd());
    assert_eq!(after.holdings(), before.holdings());
    assert_eq!(
        after.orders(OrderGroup::Open),
        before.orders(OrderGroup::Open)
    );
    assert_eq!(
        after.orders(OrderGroup::Closed),
        before.orders(OrderGroup::Closed)
    );
    assert_eq!(after.broker.fills(), before.broker.fills());
    assert_eq!(after.sellable(SAM), before.sellable(SAM));
    assert_eq!(
        after.buying_power(Currency::Krw),
        before.buying_power(Currency::Krw)
    );
    assert_eq!(after.broker.snapshot(), before.broker.snapshot());

    // 비어 있지 않은 상태를 비교했는지 확인한다.
    assert_eq!(before.orders(OrderGroup::Open).len(), 3);
    assert_eq!(before.holding(SAM).unwrap().quantity, d("4"));
    assert_eq!(before.sellable(SAM), d("0"));
    assert!(before.krw().held > d("0"));
    assert!(before.krw().unsettled > d("0"));
    assert!(before.usd().held > d("0"));
}

// 검증: V6
#[test]
fn 내보낸_값은_글자로_바꿨다가_되돌려도_같다() {
    let desk = busy_desk();
    let snapshot = desk.broker.snapshot();
    let text = serde_json::to_string(&snapshot).unwrap();
    let back: Snapshot = serde_json::from_str(&text).unwrap();
    assert_eq!(back, snapshot);
    // 상태가 바뀌면 값도 달라진다.
    desk.quote(HYN, "30000", at(9));
    assert_ne!(desk.broker.snapshot(), snapshot);
}

// 검증: V6
#[test]
fn 되살린_뒤_같은_시세를_주면_끊기지_않은_계좌와_같은_결과가_나온다() {
    let straight = busy_desk();
    let restored = busy_desk().reopen(busy_config());

    for desk in [&straight, &restored] {
        desk.quote(HYN, "30000", at(10));
        desk.quote(SAM, "71000", at(11));
        desk.accepted(&market_buy(HYN, "2"), at(12));
        desk.quote(HYN, "30050", at(13));
    }
    let straight_events = straight.quote(SAM, "71500", at(14));
    let restored_events = restored.quote(SAM, "71500", at(14));
    assert_eq!(kinds(&straight_events), vec![OrderEventKind::Fill]);
    assert_eq!(restored_events, straight_events);

    // 미국 장 시각의 달러 시세: 그사이 마감이 된 단타 주문이 취소되고 달러 주문이 체결된다.
    let straight_events = straight.us_quote(AAPL, "150.00", us_at(1));
    let restored_events = restored.us_quote(AAPL, "150.00", us_at(1));
    assert_eq!(
        kinds(&straight_events),
        vec![OrderEventKind::Canceled, OrderEventKind::Fill]
    );
    assert_eq!(restored_events, straight_events);

    assert_eq!(restored.broker.snapshot(), straight.broker.snapshot());
    assert_eq!(restored.krw(), straight.krw());
    assert_eq!(restored.usd(), straight.usd());
    assert_eq!(restored.holdings(), straight.holdings());
    assert_eq!(restored.broker.fills(), straight.broker.fills());
    assert_eq!(
        restored.orders(OrderGroup::Closed),
        straight.orders(OrderGroup::Closed)
    );
}

// 검증: V6, E12
#[test]
fn 되살린_열린_주문은_묶은_것과_마감을_그대로_지닌다() {
    let before = busy_desk();
    let after = before.reopen(busy_config());
    let open = after.orders(OrderGroup::Open);
    let hyn = open
        .iter()
        .find(|order| order.symbol.as_str() == HYN)
        .unwrap();
    assert_eq!(hyn.status, OrderStatus::PartialFilled);
    assert_eq!(hyn.execution.filled_quantity, d("3"));
    let held_before = after.krw().held;

    // 단타 주문(접수 at(6))은 되살린 뒤에도 at(66)에 취소된다.
    assert!(after.broker.on_time(at(65)).is_empty());
    let events = after.broker.on_time(at(66));
    assert_eq!(kinds(&events), vec![OrderEventKind::Canceled]);
    let canceled = after.order(&hyn.order_id);
    assert_eq!(canceled.close_reason, Some(CloseReason::UnfilledTimeout));
    // 남은 7주 × 30,000 = 210,000 + 올림(31.5) 32 = 210,032 가 풀린다.
    assert_eq!(held_before - after.krw().held, d("210032"));
    assert_eq!(after.holding(HYN).unwrap().quantity, d("3"));

    // 묶인 팔기도 취소하면 풀린다.
    let sam = open
        .iter()
        .find(|order| order.symbol.as_str() == SAM)
        .unwrap();
    after.cancel(&sam.order_id, at(67)).unwrap();
    assert_eq!(after.sellable(SAM), d("4"));
}

// 검증: V6
#[test]
fn 되살린_뒤의_새_주문은_앞의_식별자와_겹치지_않는다() {
    let before = busy_desk();
    let after = before.reopen(busy_config());
    let mut known: Vec<_> = after
        .orders(OrderGroup::Open)
        .into_iter()
        .chain(after.orders(OrderGroup::Closed))
        .map(|order| order.order_id)
        .collect();
    let new_id = after.accepted(&buy(HYN, "1", "29000"), at(20));
    assert!(!known.contains(&new_id));
    known.push(new_id);
    known.sort();
    known.dedup();
    assert_eq!(known.len(), 8, "앞의 주문 7개와 새 주문 1개");
}

// 검증: V6
#[test]
fn 결제_전인_돈과_멱등성_키도_되살아난다() {
    let before = busy_desk();
    let unsettled = before.krw().unsettled;
    let available = before.krw().available;
    let first = before
        .orders(OrderGroup::Open)
        .into_iter()
        .find(|order| order.client_order_id.as_deref() == Some("hyn-1"))
        .unwrap();
    let after = before.reopen(busy_config());

    // 같은 키 · 같은 내용: 새 주문 없이 앞의 식별자를 돌려준다.
    let again = after
        .place(
            &buy(HYN, "10", "30000")
                .timeout(UnfilledTimeout::SCALP)
                .key("hyn-1"),
            at(9),
        )
        .unwrap();
    assert_eq!(again.order_id, first.order_id);
    assert_eq!(after.orders(OrderGroup::Open).len(), 3);

    // 수요일 달력을 넣으면 결제 전이던 돈이 풀린다.
    after.broker.set_calendar(kr_calendar(day(2026, 11, 18)));
    assert_eq!(after.krw().unsettled, d("0"));
    assert_eq!(after.krw().available, available + unsettled);
}

// 검증: V6, E33
#[test]
fn 되살리면_권한이_새로_나오고_앞의_권한이_낸_표시는_받지_않는다() {
    let before = Desk::standard();
    let after = before.reopen(config());
    let req = buy(SAM, "1", "70000");

    let old_permit = before.authority.approve(req.approved());
    assert_eq!(
        after.place_raw(req.request(Some(old_permit)), at(0)),
        Err(BrokerError::CheckInvalid)
    );
    after.place(&req, at(1)).unwrap();
}

// 검증: V6
#[test]
fn 되살릴_때는_설정의_처음_돈을_쓰지_않는다() {
    let before = Desk::standard();
    before.own(SAM, "10", "70000", at(0));
    let mut other = config();
    other.initial_cash.krw = d("1");
    other.initial_cash.usd = d("2");
    let after = before.reopen(other);
    assert_eq!(after.krw(), cash("9299895", "0", "0"));
    assert_eq!(after.usd(), cash("10000", "0", "0"));
    assert_eq!(after.holding(SAM).unwrap().quantity, d("10"));
}

// 검증: V6
#[test]
fn 달력과_기준가는_되살린_뒤_다시_넣어야_한다() {
    let before = Desk::standard();
    let after = before.reopen_bare(config());
    assert_eq!(
        after.place(&buy(SAM, "1", "70000"), at(0)),
        Err(BrokerError::OrderHoursClosed)
    );
    after.feed();
    after.place(&buy(SAM, "1", "70000"), at(1)).unwrap();
}

// 검증: V6
#[test]
fn 마지막_시세는_되살아나서_시장가_주문을_바로_받을_수_있다() {
    let before = Desk::standard();
    before.quote(SAM, "70000", at(0));
    let after = before.reopen(config());
    after.accepted(&market_buy(SAM, "5"), at(1));
    assert_eq!(after.krw(), cash("9649447", "350553", "0"));
}

// 검증: V6
#[test]
fn 같은_입력은_같은_값을_낸다() {
    let first = busy_desk();
    let second = busy_desk();
    assert_eq!(first.broker.snapshot(), second.broker.snapshot());
    assert_eq!(first.broker.fills(), second.broker.fills());
    assert_eq!(
        first.orders(OrderGroup::Open),
        second.orders(OrderGroup::Open)
    );
}

// 검증: V6
#[test]
fn 새_계좌도_내보냈다가_되살릴_수_있다() {
    let (broker, _authority) = PaperBroker::open(config()).unwrap();
    let (restored, _new_authority) = PaperBroker::restore(config(), broker.snapshot()).unwrap();
    assert_eq!(restored.cash(&Currency::Krw), cash("10000000", "0", "0"));
    assert_eq!(restored.cash(&Currency::Usd), cash("10000", "0", "0"));
    assert!(restored.fills().is_empty());
}
