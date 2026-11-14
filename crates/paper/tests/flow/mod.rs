//! 2차 6~9번: 실제 증권사와 다르게 끝나는 흐름을 없앤다.
//! 수수료 · 세금을 주문 단위로 누적해 떼기(6), 마감 지난 주문을 어디서든 먼저 취소하기(7),
//! 주문이 장 마감을 넘기지 않기(8), 큰 주문의 금액 확인과 최대 주문 금액(9).

use chrono::{DateTime, Utc};
use pi_market::{Currency, SessionKind};
use pi_paper::{
    BrokerError, CloseReason, MaxFill, OrderEventKind, OrderGroup, OrderStatus, PaperConfig,
    RoundingMode, UnfilledTimeout,
};
use rust_decimal::Decimal;

use crate::support::{
    AAPL, Desk, HYN, SAM, at, buy, cash, config, config_with_krw, d, day, kinds, kr_calendar,
    kr_close, monday, sell, us_at, us_buy, utc,
};

fn decimals(values: &[&str]) -> Vec<Decimal> {
    values.iter().map(|value| d(value)).collect()
}

/// 원화 703,106원, 그 반올림 방향, 한 시세에 `most`까지(없으면 전부).
fn tight(mode: RoundingMode, most: Option<&str>) -> PaperConfig {
    let mut config = config_with_krw("703106");
    config.kr.rounding.mode = mode;
    config.kr.max_fill = most.map_or(MaxFill::Unlimited, |most| MaxFill::PerQuote(d(most)));
    config
}

// ---------------------------------------------------------------------------------------------
// 6. 수수료 · 세금은 주문 단위로 누적해 뗀다
// ---------------------------------------------------------------------------------------------

// 검증: C10, E29
// 1차 검토의 실험: 10주 × 70,300원, 수수료 0.015%, 처음 돈 703,106원, 1주씩 체결하면 올림 · 반올림에서
// 9주 체결 뒤 `Rejected`로 끝났다(주당 10.545원을 체결마다 11원으로 떼어 110원 > 묶어 둔 106원).
//
// 손셈: 10주 × 70,300 = 703,000. 수수료 105.45. 묶는 돈 703,000 + 올림 106 = 703,106(가진 돈 전부).
// k주까지의 누적 수수료 10.545 × k = 10.545 · 21.09 · 31.635 · 42.18 · 52.725 · 63.27 · 73.815 · 84.36 · 94.905 · 105.45
// - 버림:   10 · 21 · 31 · 42 · 52 · 63 · 73 · 84 · 94 · 105 → 체결마다 10 · 11 · 10 · 11 · 10 · 11 · 10 · 11 · 10 · 11
// - 반올림: 11 · 21 · 32 · 42 · 53 · 63 · 74 · 84 · 95 · 105 → 체결마다 11 · 10 · 11 · 10 · 11 · 10 · 11 · 10 · 11 · 10
// - 올림:   11 · 22 · 32 · 43 · 53 · 64 · 74 · 85 · 95 · 106 → 체결마다 11 · 11 · 10 · 11 · 10 · 11 · 10 · 11 · 10 · 11
#[test]
fn 한_주씩_나눠_체결돼도_전부_체결되고_뗀_수수료_합은_한_번에_체결됐을_때와_같다() {
    let cases: [(RoundingMode, &str, [&str; 10], &str); 3] = [
        (
            RoundingMode::Down,
            "105",
            ["10", "11", "10", "11", "10", "11", "10", "11", "10", "11"],
            "1",
        ),
        (
            RoundingMode::HalfUp,
            "105",
            ["11", "10", "11", "10", "11", "10", "11", "10", "11", "10"],
            "1",
        ),
        (
            RoundingMode::Up,
            "106",
            ["11", "11", "10", "11", "10", "11", "10", "11", "10", "11"],
            "0",
        ),
    ];
    for (mode, total, each, left) in cases {
        // 한 번에 체결됐을 때.
        let whole = Desk::open(tight(mode, None));
        let whole_id = whole.accepted(&buy(SAM, "10", "70300"), at(0));
        whole.quote(SAM, "70300", at(1));
        let whole_order = whole.order(&whole_id);
        assert_eq!(whole_order.status, OrderStatus::Filled, "{mode:?}");
        assert_eq!(whole_order.execution.commission, d(total), "{mode:?}");
        assert_eq!(whole.krw(), cash(left, "0", "0"), "{mode:?}");

        // 1주씩 열 번에 나눠 체결됐을 때.
        let desk = Desk::open(tight(mode, Some("1")));
        let order_id = desk.accepted(&buy(SAM, "10", "70300"), at(0));
        assert_eq!(desk.krw(), cash("0", "703106", "0"), "{mode:?}");
        for second in 1..=10 {
            let events = desk.quote(SAM, "70300", at(second));
            assert_eq!(events.len(), 1, "{mode:?} {second}번째 시세: {events:?}");
            assert_ne!(
                events[0].kind,
                OrderEventKind::Rejected,
                "{mode:?} {second}번째"
            );
            let cash = desk.krw();
            assert!(cash.available >= d("0") && cash.held >= d("0"), "{cash:?}");
        }

        let order = desk.order(&order_id);
        assert_eq!(order.status, OrderStatus::Filled, "{mode:?}");
        assert_eq!(order.close_reason, None, "{mode:?}");
        assert_eq!(order.execution.filled_quantity, d("10"), "{mode:?}");
        assert_eq!(order.execution.filled_amount, d("703000"), "{mode:?}");
        assert_eq!(order.execution.commission, d(total), "{mode:?}");
        assert_eq!(
            order.execution.commission, whole_order.execution.commission,
            "{mode:?}: 한 번에 체결됐을 때와 같아야 한다"
        );

        let fills = desk.broker.fills();
        let taken: Vec<Decimal> = fills.iter().map(|fill| fill.commission).collect();
        assert_eq!(taken, decimals(&each), "{mode:?}");
        assert_eq!(taken.iter().sum::<Decimal>(), d(total), "{mode:?}");

        assert_eq!(desk.krw(), cash(left, "0", "0"), "{mode:?}");
        assert_eq!(desk.holding(SAM).unwrap().quantity, d("10"), "{mode:?}");
    }
}

// 검증: C10, E29
// 10주를 71,100원에 3 · 3 · 3 · 1주로 나눠 판다. 누적 금액 213,300 · 426,600 · 639,900 · 711,000.
// 누적 수수료(0.015%) 31.995 · 63.99 · 95.985 · 106.65, 누적 거래세(0.20%) 426.6 · 853.2 · 1,279.8 · 1,422.
// 한 번에 팔았을 때: 수수료 106.65 → 버림 106 · 반올림 107 · 올림 107, 거래세 1,422.
// (체결마다 따로 맞추면 버림에서 수수료 31 × 3 + 10 = 103, 거래세 426 × 3 + 142 = 1,420이 돼 다르다.)
#[test]
fn 나눠_판_주문의_수수료와_거래세_합도_한_번에_팔았을_때와_같다() {
    for (mode, commission, proceeds) in [
        (RoundingMode::Down, "106", "709472"),
        (RoundingMode::HalfUp, "107", "709471"),
        (RoundingMode::Up, "107", "709471"),
    ] {
        let mut config = config();
        config.kr.rounding.mode = mode;
        config.kr.max_fill = MaxFill::PerQuote(d("3"));
        let desk = Desk::open(config);
        for (second, quantity) in [(0, "3"), (1, "3"), (2, "3"), (3, "1")] {
            desk.own(SAM, quantity, "70000", at(second));
        }
        let available = desk.krw().available;

        let order_id = desk.accepted(&sell(SAM, "10", "71100"), at(10));
        for second in 11..=14 {
            desk.quote(SAM, "71100", at(second));
        }
        let order = desk.order(&order_id);
        assert_eq!(order.status, OrderStatus::Filled, "{mode:?}");
        assert_eq!(order.execution.filled_amount, d("711000"), "{mode:?}");
        assert_eq!(order.execution.commission, d(commission), "{mode:?}");
        assert_eq!(order.execution.tax, d("1422"), "{mode:?}");

        let sells: Vec<_> = desk
            .broker
            .fills()
            .into_iter()
            .filter(|fill| fill.order_id == order_id)
            .collect();
        assert_eq!(sells.len(), 4, "{mode:?}");
        let commissions: Decimal = sells.iter().map(|fill| fill.commission).sum();
        let taxes: Decimal = sells.iter().map(|fill| fill.tax).sum();
        assert_eq!(commissions, d(commission), "{mode:?}");
        assert_eq!(taxes, d("1422"), "{mode:?}");

        // 받는 돈 = 711,000 − 수수료 − 1,422.
        let after = desk.krw();
        assert_eq!(after.unsettled, d(proceeds), "{mode:?}");
        assert_eq!(after.available, available, "{mode:?}");
    }
}

// 검증: C10
// 달러도 같다. 7주 × 12.99 = 90.93, 수수료 0.1% = 0.09093 → 센트 미만 버려 0.09.
// (체결마다 따로 버리면 0.01299 → 0.01씩 7번 = 0.07이 돼 다르다.)
// 묶는 돈 90.93 + 올림 0.10 = 91.03.
#[test]
fn 미국_주식도_주문_단위로_누적해_뗀다() {
    let mut config = config();
    config.us.max_fill = MaxFill::PerQuote(d("1"));
    let desk = Desk::open(config);
    let order_id = desk.accepted(&us_buy(AAPL, "7", "12.99"), us_at(0));
    assert_eq!(desk.usd(), cash("9908.97", "91.03", "0"));
    for second in 1..=7 {
        desk.us_quote(AAPL, "12.99", us_at(second));
    }
    let order = desk.order(&order_id);
    assert_eq!(order.status, OrderStatus::Filled);
    assert_eq!(order.execution.filled_amount, d("90.93"));
    assert_eq!(order.execution.commission, d("0.09"));
    assert_eq!(desk.usd(), cash("9908.98", "0", "0"));
}

// ---------------------------------------------------------------------------------------------
// 7. 마감이 지난 주문은 어디서든 먼저 취소한다
// ---------------------------------------------------------------------------------------------

/// 1분 마감의 5주 × 69,000 지정가 사기를 0초에 낸다. 묶는 돈 345,000 + 올림(51.75) 52 = 345,052.
fn scalp_order(desk: &Desk) -> pi_paper::OrderId {
    desk.accepted(
        &buy(SAM, "5", "69000").timeout(UnfilledTimeout::SCALP),
        at(0),
    )
}

// 검증: E12, N4
// 1차 검토의 실험: 1분 마감 주문이 시간 알림도 시세도 없으면 5분 뒤에도 열려 있었다.
#[test]
fn 마감이_지난_주문은_시간을_알려_주지_않아도_상세_조회에서_취소돼_있다() {
    let desk = Desk::standard();
    let order_id = scalp_order(&desk);

    // 마감 1초 전에는 열려 있다.
    assert_eq!(
        desk.order_at(&order_id, at(59)).status,
        OrderStatus::Pending
    );

    let order = desk.order_at(&order_id, at(300));
    assert_eq!(order.status, OrderStatus::Canceled);
    assert_eq!(order.close_reason, Some(CloseReason::UnfilledTimeout));
    assert_eq!(order.execution.filled_quantity, d("0"));
    assert!(order.canceled_at.is_some());
}

// 검증: E12, N4
#[test]
fn 마감이_된_바로_그_시각의_조회에서도_취소돼_있다() {
    let desk = Desk::standard();
    let order_id = scalp_order(&desk);
    let order = desk.order_at(&order_id, at(60));
    assert_eq!(order.status, OrderStatus::Canceled);
    assert_eq!(order.close_reason, Some(CloseReason::UnfilledTimeout));
}

// 검증: E12
#[test]
fn 마감이_지난_주문은_목록_조회에서_끝난_쪽에_있다() {
    let desk = Desk::standard();
    let order_id = scalp_order(&desk);
    assert_eq!(desk.orders_at(OrderGroup::Open, at(59)).len(), 1);

    assert!(desk.orders_at(OrderGroup::Open, at(300)).is_empty());
    let closed = desk.orders_at(OrderGroup::Closed, at(300));
    assert_eq!(closed.len(), 1);
    assert_eq!(closed[0].order_id, order_id);
    assert_eq!(closed[0].status, OrderStatus::Canceled);
}

// 검증: E12
#[test]
fn 마감이_지난_주문이_묶은_돈은_돈_조회에서_풀려_있다() {
    let desk = Desk::standard();
    scalp_order(&desk);
    assert_eq!(desk.krw_at(at(59)), cash("9654948", "345052", "0"));
    assert_eq!(desk.buying_power_at(Currency::Krw, at(59)), d("9654948"));

    // 매수 가능 금액 조회만으로도 풀린다.
    let other = Desk::standard();
    scalp_order(&other);
    assert_eq!(other.buying_power_at(Currency::Krw, at(300)), d("10000000"));

    assert_eq!(desk.krw_at(at(300)), cash("10000000", "0", "0"));
}

// 검증: E12
#[test]
fn 마감이_지난_파는_주문이_묶은_수량은_수량_조회에서_풀려_있다() {
    let desk = Desk::standard();
    desk.own(SAM, "10", "70000", at(0));
    desk.accepted(
        &sell(SAM, "10", "75000").timeout(UnfilledTimeout::SCALP),
        at(1),
    );
    assert_eq!(desk.sellable_at(SAM, at(60)), d("0"));
    assert_eq!(desk.sellable_at(SAM, at(300)), d("10"));
    let holdings = desk.holdings_at(at(300));
    assert_eq!(holdings.len(), 1);
    assert_eq!(holdings[0].quantity, d("10"));
}

// 검증: E12
// 1차 검토의 실험: 마감이 지난 주문이 300초 뒤 정정(수량 5 → 50)을 받아 새 주문이 열렸다.
#[test]
fn 마감이_지난_주문은_정정을_받지_않는다() {
    let desk = Desk::standard();
    let order_id = scalp_order(&desk);

    assert_eq!(
        desk.modify(&order_id, Some("50"), Some("70000"), "50", at(300)),
        Err(BrokerError::AlreadyCanceled)
    );
    let order = desk.order(&order_id);
    assert_eq!(order.status, OrderStatus::Canceled);
    assert_eq!(order.close_reason, Some(CloseReason::UnfilledTimeout));
    assert!(desk.orders(OrderGroup::Open).is_empty(), "새 주문이 없다");
    assert_eq!(desk.orders(OrderGroup::Closed).len(), 1);
    assert_eq!(desk.krw(), cash("10000000", "0", "0"));
}

// 검증: E12
#[test]
fn 마감이_지난_주문을_취소하려_하면_이미_시간이_지나_취소돼_있다() {
    let desk = Desk::standard();
    let order_id = scalp_order(&desk);
    assert_eq!(
        desk.cancel(&order_id, at(300)),
        Err(BrokerError::AlreadyCanceled)
    );
    // 이유는 "취소를 요청받음"이 아니라 "시간 안에 체결되지 않음"이다.
    assert_eq!(
        desk.order(&order_id).close_reason,
        Some(CloseReason::UnfilledTimeout)
    );
    assert_eq!(desk.krw(), cash("10000000", "0", "0"));
}

// 검증: E12
#[test]
fn 새_주문을_낼_때_마감이_지난_주문이_먼저_취소돼_돈이_풀린다() {
    // 10주 × 70,000 + 105 = 700,105: 첫 주문이 가진 돈을 다 묶는다.
    let desk = Desk::open(config_with_krw("700105"));
    let first = desk.accepted(
        &buy(SAM, "10", "70000").timeout(UnfilledTimeout::SCALP),
        at(0),
    );
    assert_eq!(desk.krw(), cash("0", "700105", "0"));
    // 마감 전에는 돈이 없어 못 낸다.
    assert_eq!(
        desk.place(&buy(HYN, "20", "30000"), at(59)),
        Err(BrokerError::InsufficientBuyingPower)
    );

    // 5분 뒤: 첫 주문이 먼저 취소되고, 풀린 돈으로 새 주문을 받는다.
    // 20주 × 30,000 = 600,000, 수수료 90 → 600,090.
    desk.accepted(&buy(HYN, "20", "30000"), at(300));
    let first = desk.order(&first);
    assert_eq!(first.status, OrderStatus::Canceled);
    assert_eq!(first.close_reason, Some(CloseReason::UnfilledTimeout));
    assert_eq!(desk.krw(), cash("100015", "600090", "0"));
    assert_eq!(desk.orders(OrderGroup::Open).len(), 1);
}

// 검증: E12
#[test]
fn 마감이_지난_반대_방향_주문은_새_주문을_막지_않는다() {
    let desk = Desk::standard();
    desk.own(SAM, "10", "70000", at(0));
    let stale_sell = desk.accepted(
        &sell(SAM, "10", "75000").timeout(UnfilledTimeout::SCALP),
        at(1),
    );
    assert_eq!(
        desk.place(&buy(SAM, "1", "69000"), at(60)),
        Err(BrokerError::OppositePendingOrderExists)
    );

    desk.accepted(&buy(SAM, "1", "69000"), at(300));
    assert_eq!(desk.order(&stale_sell).status, OrderStatus::Canceled);
    assert_eq!(desk.sellable(SAM), d("10"));
}

// 검증: E12, N4
#[test]
fn 조회가_마감을_처리해도_취소_알림이_나간다() {
    let desk = Desk::standard();
    let order_id = scalp_order(&desk);
    let mut events = desk.as_broker().subscribe();

    desk.orders_at(OrderGroup::Open, at(300));
    let event = events.try_recv().unwrap();
    assert_eq!(event.kind, OrderEventKind::Canceled);
    assert_eq!(event.order.order_id, order_id);
    assert_eq!(event.order.status, OrderStatus::Canceled);
    assert_eq!(event.order.close_reason, Some(CloseReason::UnfilledTimeout));
    assert!(events.try_recv().is_err(), "한 번만 온다");

    // 이미 처리된 마감은 다시 알리지 않는다.
    assert!(desk.on_time(at(301)).is_empty());
    assert!(events.try_recv().is_err());
}

// ---------------------------------------------------------------------------------------------
// 8. 주문은 장 마감을 넘기지 않는다
// ---------------------------------------------------------------------------------------------

fn three_days() -> UnfilledTimeout {
    UnfilledTimeout::After(std::time::Duration::from_secs(3 * 24 * 3600))
}

// 검증: E12, N4
// 1차 검토의 실험: 3일 기다리는 주문이 월요일 장 마감 뒤에도 열려 있었다(묶인 돈 69,011원).
#[test]
fn 사흘을_기다리는_주문도_그_날_정규장이_끝나면_취소된다() {
    let desk = Desk::standard();
    let order_id = desk.accepted(&buy(SAM, "1", "69000").timeout(three_days()), at(0));
    assert_eq!(desk.krw(), cash("9930989", "69011", "0"));

    let close = kr_close(monday());
    assert!(
        desk.on_time(close - chrono::Duration::seconds(1))
            .is_empty()
    );
    assert_eq!(desk.order(&order_id).status, OrderStatus::Pending);

    let events = desk.on_time(close);
    assert_eq!(kinds(&events), vec![OrderEventKind::Canceled]);
    let order = desk.order(&order_id);
    assert_eq!(order.status, OrderStatus::Canceled);
    assert_eq!(order.close_reason, Some(CloseReason::UnfilledTimeout));
    assert_eq!(order.canceled_at, Some(close));
    assert_eq!(desk.krw(), cash("10000000", "0", "0"));
}

// 검증: E12, N4
// 1차 검토의 실험: 그 주문이 화요일 시세에 체결됐다.
#[test]
fn 장_마감을_넘긴_주문은_다음_날_시세에_체결되지_않는다() {
    let desk = Desk::standard();
    let order_id = desk.accepted(&buy(SAM, "1", "69000").timeout(three_days()), at(0));

    // 월요일에는 시간을 알려 주지 않았다. 화요일 달력과 기준가를 넣고 화요일 장중에 시세를 준다.
    let tuesday = day(2026, 11, 17);
    desk.broker.set_calendar(kr_calendar(tuesday));
    desk.reference(tuesday);
    let events = desk.quote(SAM, "69000", utc(tuesday, 1, 0, 0));
    assert_eq!(kinds(&events), vec![OrderEventKind::Canceled]);

    let order = desk.order(&order_id);
    assert_eq!(order.status, OrderStatus::Canceled);
    assert_eq!(order.close_reason, Some(CloseReason::UnfilledTimeout));
    assert_eq!(order.execution.filled_quantity, d("0"));
    assert!(desk.broker.fills().is_empty());
    assert!(desk.holdings().is_empty());
    assert_eq!(desk.krw(), cash("10000000", "0", "0"));
}

// 검증: E12, N4
#[test]
fn 장_마감_뒤의_조회에서_긴_주문은_이미_취소돼_있다() {
    let desk = Desk::standard();
    let order_id = desk.accepted(&buy(SAM, "1", "69000").timeout(three_days()), at(0));
    // 월요일 21:00 KST(12:00 UTC). 시간 알림도 시세도 없었다.
    let night = utc(monday(), 12, 0, 0);
    assert_eq!(
        desk.order_at(&order_id, night).status,
        OrderStatus::Canceled
    );
    assert_eq!(desk.krw_at(night), cash("10000000", "0", "0"));
}

// 검증: E12, N4
// "그 세션이 끝나면 취소한다": 프리마켓에 받은 주문은 프리마켓이 끝날 때(정규장 시작 시각) 취소된다.
#[test]
fn 프리마켓에_낸_긴_주문은_프리마켓이_끝나면_취소된다() {
    let mut config = config();
    config.kr.sessions.limit = vec![SessionKind::PreMarket, SessionKind::Regular];
    let desk = Desk::open(config);
    let open: DateTime<Utc> = utc(monday(), 0, 0, 0);
    let order_id = desk.accepted(
        &buy(SAM, "1", "69000").timeout(three_days()),
        open - chrono::Duration::minutes(30),
    );

    assert!(desk.on_time(open - chrono::Duration::seconds(1)).is_empty());
    assert_eq!(kinds(&desk.on_time(open)), vec![OrderEventKind::Canceled]);
    let order = desk.order(&order_id);
    assert_eq!(order.close_reason, Some(CloseReason::UnfilledTimeout));
    assert_eq!(order.canceled_at, Some(open));
}

// 검증: N4
// 세션이 끝나기 전에 끝나는 짧은 기다림은 그대로다.
#[test]
fn 세션보다_짧은_기다림은_준_시간에_취소된다() {
    let desk = Desk::standard();
    let last_minute = kr_close(monday()) - chrono::Duration::seconds(90);
    // 마감 90초 전에 낸 단타(1분): 60초 뒤에 취소된다.
    let scalp = desk.accepted(
        &buy(SAM, "1", "69000").timeout(UnfilledTimeout::SCALP),
        last_minute,
    );
    // 같은 때 낸 스윙(10분): 10분이 되기 전에 장이 끝나므로 장 마감에 취소된다.
    let swing = desk.accepted(
        &buy(HYN, "1", "29000").timeout(UnfilledTimeout::SWING),
        last_minute,
    );

    let events = desk.on_time(last_minute + chrono::Duration::seconds(60));
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].order.order_id, scalp);

    assert!(
        desk.on_time(kr_close(monday()) - chrono::Duration::seconds(1))
            .is_empty()
    );
    let events = desk.on_time(kr_close(monday()));
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].order.order_id, swing);
    assert_eq!(
        events[0].order.close_reason,
        Some(CloseReason::UnfilledTimeout)
    );
}

// ---------------------------------------------------------------------------------------------
// 9. 큰 주문
// 출처: 토스증권 공식 명세(openapi.json)의 주문 생성 · 정정 요청 `confirmHighValueOrder` 설명
// ("1억원 이상의 주문 시 true가 아니면 400 confirm-high-value-required", "본 플래그와 무관하게
// 422 max-order-amount-exceeded")과 오류 예(threshold 100000000 KRW, "30억원 초과 주문 (KR)" ·
// "30억원 초과 정정 (KR)" maxAmount 3000000000 KRW).
// 딱 30억 원인 주문은 명세의 말이 "초과"와 "이상"으로 갈려 단언하지 않았다(미확인).
// ---------------------------------------------------------------------------------------------

/// 원화 90억 원을 가진 계좌.
fn rich() -> Desk {
    Desk::open(config_with_krw("9000000000"))
}

fn confirm_required() -> BrokerError {
    BrokerError::ConfirmHighValueRequired {
        threshold: d("100000000"),
    }
}

fn max_exceeded() -> BrokerError {
    BrokerError::MaxOrderAmountExceeded {
        max_amount: d("3000000000"),
    }
}

// 검증: C10
#[test]
fn 금액이_1억_원_이상인_주문은_확인_표시가_없으면_받지_않는다() {
    let desk = rich();
    // 1,250주 × 80,000 = 100,000,000: 딱 1억 원도 "이상"이다.
    assert_eq!(
        desk.place(&buy(SAM, "1250", "80000"), at(0)),
        Err(confirm_required())
    );
    assert_eq!(desk.krw(), cash("9000000000", "0", "0"));
    assert!(desk.orders(OrderGroup::Open).is_empty());
    assert!(desk.orders(OrderGroup::Closed).is_empty());

    // 1,428주 × 70,000 = 99,960,000: 1억 원 미만은 확인 없이 받는다. 수수료 14,994.
    desk.accepted(&buy(SAM, "1428", "70000"), at(1));
    assert_eq!(desk.krw(), cash("8900025006", "99974994", "0"));

    // 확인 표시가 있으면 1억 원 이상도 받는다. 수수료 15,000.
    desk.accepted(&buy(SAM, "1250", "80000").confirmed(), at(2));
    assert_eq!(desk.krw(), cash("8800010006", "199989994", "0"));
}

// 검증: C10
#[test]
fn 파는_주문도_1억_원_이상이면_확인_표시가_있어야_한다() {
    let desk = rich();
    let bought = desk.accepted(&buy(SAM, "1500", "70000").confirmed(), at(0));
    desk.quote(SAM, "70000", at(1));
    assert_eq!(desk.order(&bought).status, OrderStatus::Filled);

    assert_eq!(
        desk.place(&sell(SAM, "1250", "80000"), at(2)),
        Err(confirm_required())
    );
    assert_eq!(desk.sellable(SAM), d("1500"));
    desk.accepted(&sell(SAM, "1250", "80000").confirmed(), at(3));
    assert_eq!(desk.sellable(SAM), d("250"));
}

// 검증: C10
#[test]
fn 금액이_30억_원을_넘는_주문은_확인_표시가_있어도_받지_않는다() {
    let desk = rich();
    // 42,858주 × 70,000 = 3,000,060,000.
    assert_eq!(
        desk.place(&buy(SAM, "42858", "70000").confirmed(), at(0)),
        Err(max_exceeded())
    );
    // 확인 표시가 없어도 이유는 "최대 주문 금액 초과"다(표시와 상관없다).
    assert_eq!(
        desk.place(&buy(SAM, "42858", "70000"), at(0)),
        Err(max_exceeded())
    );
    assert_eq!(desk.krw(), cash("9000000000", "0", "0"));
    assert!(desk.orders(OrderGroup::Open).is_empty());

    // 42,857주 × 70,000 = 2,999,990,000: 30억 원 아래는 확인 표시가 있으면 받는다.
    desk.accepted(&buy(SAM, "42857", "70000").confirmed(), at(1));
    assert_eq!(desk.orders(OrderGroup::Open).len(), 1);
}

// 검증: C10
#[test]
fn 정정한_주문의_금액에도_같은_확인을_한다() {
    let desk = rich();
    // 1,000주 × 70,000 = 70,000,000.
    let original = desk.accepted(&buy(SAM, "1000", "70000"), at(0));

    // 1,250주 × 80,000 = 1억 원으로 정정: 확인 표시가 없으면 받지 않고 원주문은 그대로다.
    assert_eq!(
        desk.modify(&original, Some("1250"), Some("80000"), "1250", at(1)),
        Err(confirm_required())
    );
    // 30억 원을 넘는 정정은 확인 표시가 있어도 받지 않는다.
    assert_eq!(
        desk.modify_with(
            &original,
            Some("42858"),
            Some("70000"),
            "42858",
            true,
            at(2)
        ),
        Err(max_exceeded())
    );
    assert_eq!(desk.order(&original).status, OrderStatus::Pending);
    assert_eq!(desk.orders(OrderGroup::Open).len(), 1);

    // 확인 표시가 있으면 받는다.
    let replaced = desk
        .modify_with(&original, Some("1250"), Some("80000"), "1250", true, at(3))
        .unwrap();
    assert_eq!(desk.order(&replaced).quantity, d("1250"));
    assert_eq!(desk.order(&original).status, OrderStatus::Replaced);
}

// 검증: C10
#[test]
fn 큰_주문의_기준_금액은_설정한_값이_그대로_적용된다() {
    let mut small = config();
    small.kr.high_value_confirm_from = Some(d("1000000"));
    small.kr.max_order_amount = Some(d("2000000"));
    let desk = Desk::open(small);
    // 15주 × 70,000 = 1,050,000.
    assert_eq!(
        desk.place(&buy(SAM, "15", "70000"), at(0)),
        Err(BrokerError::ConfirmHighValueRequired {
            threshold: d("1000000")
        })
    );
    desk.accepted(&buy(SAM, "15", "70000").confirmed(), at(0));
    // 14주 × 70,000 = 980,000.
    desk.accepted(&buy(SAM, "14", "70000"), at(0));
    // 29주 × 70,000 = 2,030,000.
    assert_eq!(
        desk.place(&buy(SAM, "29", "70000").confirmed(), at(0)),
        Err(BrokerError::MaxOrderAmountExceeded {
            max_amount: d("2000000")
        })
    );

    // 기준을 두지 않으면 얼마든 확인 없이 받는다.
    let mut none = config_with_krw("9000000000");
    none.kr.high_value_confirm_from = None;
    none.kr.max_order_amount = None;
    let desk = Desk::open(none);
    desk.accepted(&buy(SAM, "50000", "70000"), at(0));
}
