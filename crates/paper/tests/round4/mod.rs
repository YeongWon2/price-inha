//! 4차 작업 1~5 (`docs/dev/modules/paper.md` "4차 작업", `docs/dev/decisions.md` 2026-11-15).
//!
//! 1. (가)로 정함: 셈은 그대로 둔다. 같은 값 · 같은 수량 정정이 1원 때문에 거부되지 않는 것은
//!    원 미만 **버림**일 때뿐이다. 올림 · 반올림에서는 정정한 새 주문의 수수료가 따로 셈해져 1원이
//!    더 필요할 수 있다(그 1원이 없으면 돈이 모자라 거부되고, 있으면 받는다).
//! 2. 거래소 시각이 있는 시세와 없는 시세가 섞여도 옛 시세(거래소 시각이 그 종목에서 가장 늦게
//!    기억한 거래소 시각보다 앞)로는 체결도 기억도 하지 않는다.
//! 3. 거래소 시각이 같고 값만 다른 시세가 번갈아 와도, 그 시각에 이미 쓴 거래로는 다시 체결하지
//!    않는다(실제 거래 수보다 많이 체결하지 않는다). 되살린 뒤에도 같다.
//! 4. 거래소 시각이 있어도 받은 시각이 주문 시각보다 뒤여야 체결에 쓴다(거래소 시계가 앞선 경우).
//! 5. 상태 값 모양 2(내보낸 시각 `ledger.last_now`가 없다)를 읽으면 지금 모양으로 올려 되살린다.
//!
//! 끝에 "돈이 어긋나지 않는다" 속성 테스트를 넓힌 판을 둔다: 시각 없는 시세, 그 종목의 가장 늦은
//! 거래소 시각과 같은 시각의 다른 값, 거래소 시계가 앞선 시세를 섞는다.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use pi_market::{Currency, Quote};
use pi_paper::{
    Broker, BrokerError, CloseReason, MaxFill, Order, OrderGroup, OrderId, OrderStatus, OrderType,
    PaperBroker, PaperConfig, ProceedsUsable, RoundingMode, Side, Snapshot, SnapshotReason,
    UnfilledTimeout,
};
use proptest::prelude::{
    Just, ProptestConfig, Strategy, any, prop_assert, prop_assert_eq, prop_oneof, proptest,
};
use proptest::test_runner::TestCaseError;
use rust_decimal::{Decimal, RoundingStrategy};
use serde_json::Value;

use crate::support::{
    Desk, HYN, SAM, at, buy, cash, config, config_with_krw, d, day, kr_calendar, market_buy,
    market_sell, monday, quote_full, quote_received, sell, sym,
};

// ───────────── 1. (가) 정정의 1원은 버림일 때만 생기지 않는다 ─────────────

/// 2주를 70,000원에 사는 주문을 내고 1주만 체결시킨 계좌(한 시세에 1주까지).
/// 70,000원 1주의 수수료는 10.5원, 2주는 21원이다(0.015%).
fn half_filled(mode: RoundingMode, krw: &str) -> (Desk, OrderId) {
    let mut config = config_with_krw(krw);
    config.kr.rounding.mode = mode;
    config.kr.max_fill = MaxFill::PerQuote(d("1"));
    let desk = Desk::open(config);
    let order_id = desk.accepted(&buy(SAM, "2", "70000"), at(0));
    desk.quote(SAM, "70000", at(1));
    assert_eq!(
        desk.order(&order_id).execution.filled_quantity,
        d("1"),
        "한 시세에 1주까지 체결된다"
    );
    (desk, order_id)
}

// 검증: E12, E29
#[test]
fn 버림이면_같은_값_같은_수량_정정이_1원_때문에_거부되지_않는다() {
    // 묶은 돈 140,021원(140,000 + 올림 21)으로 남는 돈이 없다.
    let (desk, order_id) = half_filled(RoundingMode::Down, "140021");
    let order = desk.order(&order_id);
    // 버림: 1주 체결에서 뗀 수수료는 10원. 남은 묶음 = 70,000 + 21 − 10 = 70,011.
    assert_eq!(order.execution.commission, d("10"));
    assert_eq!(desk.krw(), cash("0", "70011", "0"));

    let new_id = desk
        .modify(&order_id, Some("1"), Some("70000"), "1", at(2))
        .expect("버림이면 같은 값 · 남은 수량 그대로의 정정은 돈 때문에 거부되지 않는다");
    // 새 주문 1주 = 70,000 + 올림(10.5) = 70,011. 더 가져가지도 남기지도 않는다.
    assert_eq!(desk.order(&new_id).status, OrderStatus::Pending);
    assert_eq!(desk.krw(), cash("0", "70011", "0"));
}

// 검증: E12, E29
#[test]
fn 올림과_반올림에서는_같은_값_같은_수량_정정에_1원이_더_필요할_수_있다() {
    for mode in [RoundingMode::Up, RoundingMode::HalfUp] {
        let (desk, order_id) = half_filled(mode, "140021");
        let order = desk.order(&order_id);
        // 올림 · 반올림: 1주 체결에서 뗀 수수료는 11원. 남은 묶음 = 70,000 + 21 − 11 = 70,010.
        assert_eq!(order.execution.commission, d("11"), "{mode:?}");
        assert_eq!(desk.krw(), cash("0", "70010", "0"), "{mode:?}");

        // 새 주문 1주는 70,000 + 올림(10.5) = 70,011원을 묶어야 해 1원이 모자란다.
        desk.on_time(at(2));
        let before = desk.broker.snapshot();
        assert_eq!(
            desk.modify(&order_id, Some("1"), Some("70000"), "1", at(2)),
            Err(BrokerError::InsufficientBuyingPower),
            "{mode:?}: 셈은 그대로 둔다((가)) — 정정한 새 주문의 수수료는 따로 셈한다"
        );
        assert_eq!(
            desk.broker.snapshot(),
            before,
            "{mode:?}: 거부는 아무것도 바꾸지 않는다"
        );
        assert_eq!(desk.order(&order_id).status, OrderStatus::PartialFilled);
    }
}

// 검증: E12, E29
#[test]
fn 올림과_반올림에서도_1원이_더_있으면_같은_값_같은_수량_정정을_받는다() {
    for mode in [RoundingMode::Up, RoundingMode::HalfUp] {
        let (desk, order_id) = half_filled(mode, "140022");
        assert_eq!(desk.krw(), cash("1", "70010", "0"), "{mode:?}");
        let new_id = desk
            .modify(&order_id, Some("1"), Some("70000"), "1", at(2))
            .unwrap_or_else(|error| panic!("{mode:?}: 1원이 있으면 받는다: {error:?}"));
        assert_eq!(desk.order(&new_id).status, OrderStatus::Pending);
        assert_eq!(desk.krw(), cash("0", "70011", "0"), "{mode:?}");
    }
}

// ───────────── 2. 시각 없는 시세가 끼어도 옛 시세를 막는다 ─────────────

/// 거래소 시각이 없는 원화 시세.
fn untimed(code: &str, price: &str, received_at: DateTime<Utc>) -> Quote {
    quote_full(code, price, Currency::Krw, None, received_at)
}

/// 거래소 시각과 받은 시각을 저마다 정한 원화 시세.
fn timed(code: &str, price: &str, traded_at: DateTime<Utc>, received_at: DateTime<Utc>) -> Quote {
    quote_full(code, price, Currency::Krw, Some(traded_at), received_at)
}

// 검증: E25, N6
#[test]
fn 시각_없는_시세가_끼어도_가장_늦은_거래소_시각보다_앞인_시세로는_체결하지_않는다() {
    let desk = Desk::standard();
    let order_id = desk.accepted(&buy(SAM, "1", "70000"), at(0));
    // 거래소 시각 10초의 71,000원(지정가보다 비싸 체결되지 않는다).
    desk.quote(SAM, "71000", at(10));
    // 거래소 시각이 없는 71,000원.
    desk.on_quote(&untimed(SAM, "71000", at(11)), at(11));
    // 거래소 시각 5초의 69,000원: 10초보다 앞이라 옛 시세다.
    let events = desk.on_quote(&timed(SAM, "69000", at(5), at(12)), at(12));

    assert!(
        desk.broker.fills().is_empty(),
        "옛 시세로 체결됐다: {events:?}"
    );
    let order = desk.order(&order_id);
    assert_eq!(order.execution.filled_quantity, d("0"));
    assert_eq!(order.status, OrderStatus::Pending);

    // 그 뒤에 온 새 시세로는 체결된다(막는 것은 옛 시세뿐이다).
    desk.quote(SAM, "69000", at(13));
    assert_eq!(desk.order(&order_id).status, OrderStatus::Filled);
}

// 검증: E25, N6
#[test]
fn 시각_없는_시세가_끼어도_옛_시세는_기억하지_않는다() {
    // 같은 시세를 받은 두 계좌 가운데 하나만 옛 시세를 더 받는다. 시장가 사기가 묶는 돈이 같아야 한다.
    let given = Desk::standard();
    let control = Desk::standard();
    for desk in [&given, &control] {
        desk.quote(SAM, "71000", at(10));
        desk.on_quote(&untimed(SAM, "71000", at(11)), at(11));
    }
    given.on_quote(&timed(SAM, "60000", at(5), at(12)), at(12));
    control.on_time(at(12));

    let ordered = given.place(&market_buy(SAM, "10"), at(13));
    let expected = control.place(&market_buy(SAM, "10"), at(13));
    assert!(expected.is_ok(), "{expected:?}");
    assert!(ordered.is_ok(), "{ordered:?}");
    assert_eq!(
        given.krw(),
        control.krw(),
        "옛 시세의 값으로 시장가 사기의 묶음을 셈했다"
    );
}

// ───────────── 3. 같은 거래소 시각의 다른 값이 번갈아 와도 ─────────────

/// 한 시세에 1주까지 체결되는 계좌에서 10주를 70,000원에 사는 주문.
fn one_per_quote() -> (Desk, OrderId, PaperConfig) {
    let mut config = config();
    config.kr.max_fill = MaxFill::PerQuote(d("1"));
    let desk = Desk::open(config.clone());
    let order_id = desk.accepted(&buy(SAM, "10", "70000"), at(0));
    (desk, order_id, config)
}

// 검증: E25, N6
#[test]
fn 거래소_시각이_같고_값만_다른_두_시세가_번갈아_와도_거래_수보다_많이_체결하지_않는다() {
    let (desk, order_id, _) = one_per_quote();
    // 5초에 일어난 두 거래(69,900 · 69,800)를 조회할 때마다 번갈아 받는다.
    for round in 0..6 {
        let price = if round % 2 == 0 { "69900" } else { "69800" };
        let now = at(5 + round);
        desk.on_quote(&timed(SAM, price, at(5), now), now);
    }
    assert_eq!(
        desk.order(&order_id).execution.filled_quantity,
        d("2"),
        "거래 둘(한 거래에 1주)보다 많이 체결됐다"
    );
    assert_eq!(desk.broker.fills().len(), 2);
}

// 검증: E25, N6, V6
#[test]
fn 같은_거래소_시각에_이미_쓴_거래는_되살린_뒤에도_다시_쓰지_않는다() {
    let (desk, order_id, config) = one_per_quote();
    desk.on_quote(&timed(SAM, "69900", at(5), at(5)), at(5));
    desk.on_quote(&timed(SAM, "69800", at(5), at(6)), at(6));
    assert_eq!(desk.order(&order_id).execution.filled_quantity, d("2"));

    let reopened = desk.reopen(config);
    reopened.on_quote(&timed(SAM, "69900", at(5), at(7)), at(7));
    reopened.on_quote(&timed(SAM, "69800", at(5), at(8)), at(8));
    assert_eq!(
        reopened.order(&order_id).execution.filled_quantity,
        d("2"),
        "되살린 뒤 같은 시각의 쓴 거래로 다시 체결됐다"
    );
}

// 검증: E25, N6
#[test]
fn 시각_없는_시세가_끼어도_같은_거래소_시각의_쓴_거래로_다시_체결하지_않는다() {
    let (desk, order_id, _) = one_per_quote();
    desk.on_quote(&timed(SAM, "69900", at(5), at(5)), at(5));
    desk.on_quote(&untimed(SAM, "69900", at(6)), at(6));
    let before = desk.broker.fills().len();
    let events = desk.on_quote(&timed(SAM, "69900", at(5), at(7)), at(7));
    assert_eq!(
        desk.broker.fills().len(),
        before,
        "5초의 69,900원 거래를 두 번 썼다: {events:?}"
    );
    assert!(desk.order(&order_id).execution.filled_quantity >= d("1"));
}

// ───────────── 4. 거래소 시계가 앞서도 받은 시각을 본다 ─────────────

// 검증: E25, N6
#[test]
fn 거래소_시각이_주문_뒤여도_받은_시각이_주문_시각_이하면_체결하지_않는다() {
    for received in [at(5), at(10)] {
        let desk = Desk::standard();
        let order_id = desk.accepted(&buy(SAM, "1", "70000"), at(10));
        // 거래소 시계가 앞서 거래소 시각은 20초지만, 받은 것은 주문을 내기 전(또는 같은 때)이다.
        let events = desk.on_quote(&timed(SAM, "69000", at(20), received), at(11));
        assert!(
            desk.broker.fills().is_empty(),
            "받은 시각 {received}: 주문 전에 받은 시세로 체결됐다: {events:?}"
        );
        assert_eq!(desk.order(&order_id).status, OrderStatus::Pending);
    }
}

// 검증: E25, N6
#[test]
fn 거래소_시각과_받은_시각이_모두_주문_뒤면_체결한다() {
    let desk = Desk::standard();
    let order_id = desk.accepted(&buy(SAM, "1", "70000"), at(10));
    desk.on_quote(&timed(SAM, "69000", at(20), at(11)), at(11));
    assert_eq!(desk.order(&order_id).status, OrderStatus::Filled);
}

// ───────────── 5. 상태 값 모양 2를 올려 되살린다 ─────────────

/// 상태 값을 모양 2로 바꾼다: 모양 번호를 2로, 모양 3에서 더한 내보낸 시각(`ledger.last_now`)을 뺀다.
fn as_shape_two(snapshot: &Snapshot) -> Snapshot {
    let mut value = serde_json::to_value(snapshot).unwrap();
    value["version"] = Value::from(2);
    value["ledger"].as_object_mut().unwrap().remove("last_now");
    serde_json::from_value(value).unwrap()
}

/// 모양 번호만 바꾼 상태 값.
fn with_version(snapshot: &Snapshot, version: u32) -> Snapshot {
    let mut value = serde_json::to_value(snapshot).unwrap();
    value["version"] = Value::from(version);
    serde_json::from_value(value).unwrap()
}

/// 상태 값의 모양 번호.
fn version_of(snapshot: &Snapshot) -> Value {
    serde_json::to_value(snapshot).unwrap()["version"].clone()
}

/// 보유 · 열린 사는 주문 · 열린 파는 주문 · 마지막 시세가 있는 계좌.
fn busy_desk() -> Desk {
    let desk = Desk::standard();
    desk.own(SAM, "5", "70000", at(0));
    desk.accepted(&buy(HYN, "3", "29000"), at(2));
    desk.accepted(&sell(SAM, "2", "72000"), at(3));
    desk.quote(HYN, "29500", at(4));
    desk
}

// 검증: V6
#[test]
fn 모양_2_상태_값을_읽으면_지금_모양으로_올려_되살린다() {
    let desk = busy_desk();
    let current = desk.broker.snapshot();
    let (restored, _) = PaperBroker::restore(config(), as_shape_two(&current))
        .expect("모양 2 상태 값은 올려서 되살린다");

    assert_eq!(
        version_of(&restored.snapshot()),
        version_of(&current),
        "되살린 계좌는 지금 모양으로 내보낸다"
    );
    let now = at(5);
    assert_eq!(restored.cash(&Currency::Krw, now), desk.krw_at(now));
    assert_eq!(
        desk.run(restored.holdings(None, now)).unwrap(),
        desk.holdings_at(now),
        "보유가 그대로다"
    );
    assert_eq!(restored.fills(), desk.broker.fills());
}

// 검증: V6
#[test]
fn 모양_2에서_올린_계좌는_끊기지_않은_계좌와_같은_결과를_낸다() {
    let straight = busy_desk();
    let (restored, _) =
        PaperBroker::restore(config(), as_shape_two(&straight.broker.snapshot())).unwrap();
    restored.set_calendar(kr_calendar(monday()));
    restored.set_reference_price(sym(SAM), d("70000"), monday());
    restored.set_reference_price(sym(HYN), d("30000"), monday());

    // 같은 시세를 주면 같은 체결이 생기고, 같은 상태가 된다.
    let quote = quote_received(HYN, "28950", Currency::Krw, at(6));
    let straight_events = straight.on_quote(&quote, at(6));
    let restored_events = restored.on_quote(&quote, at(6));
    assert_eq!(restored_events, straight_events);
    assert_eq!(restored.fills(), straight.broker.fills());
    assert_eq!(restored.cash(&Currency::Krw, at(6)), straight.krw_at(at(6)));
    assert_eq!(restored.snapshot(), straight.broker.snapshot());
}

// 검증: V6
#[test]
fn 아무것도_하지_않은_모양_2_상태_값도_되살린다() {
    let desk = Desk::standard();
    let (restored, _) = PaperBroker::restore(config(), as_shape_two(&desk.broker.snapshot()))
        .expect("빈 모양 2 상태 값도 올려서 되살린다");
    assert_eq!(
        restored.cash(&Currency::Krw, at(0)),
        cash("10000000", "0", "0")
    );
}

// 검증: V6
#[test]
fn 모양_2도_올린_뒤_말이_안_되면_되살리지_않는다() {
    // 끝난 주문(묶인 것 없음, 체결 0)의 수량만 0으로 바꾼다.
    let desk = Desk::standard();
    let order_id = desk.accepted(&buy(SAM, "1", "70000"), at(0));
    desk.cancel(&order_id, at(1)).unwrap();
    let mut value = serde_json::to_value(as_shape_two(&desk.broker.snapshot())).unwrap();
    value["ledger"]["orders"][0]["quantity"] = Value::from("0");
    let snapshot: Snapshot = serde_json::from_value(value).unwrap();
    assert_eq!(
        PaperBroker::restore(config(), snapshot).err(),
        Some(BrokerError::InvalidSnapshot(
            SnapshotReason::NonPositiveOrderQuantity
        ))
    );
}

// 검증: V6
#[test]
fn 모르는_모양_번호는_여전히_되살리지_않는다() {
    let snapshot = busy_desk().broker.snapshot();
    for version in [0, 1, 99] {
        assert_eq!(
            PaperBroker::restore(config(), with_version(&snapshot, version)).err(),
            Some(BrokerError::InvalidSnapshot(
                SnapshotReason::UnknownVersion(version)
            )),
            "모양 {version}"
        );
    }
}

// ───────────── 넓힌 속성 테스트 ─────────────

/// 섞어 볼 동작 하나.
#[derive(Debug, Clone)]
enum Op {
    /// 지정가 주문. 값 = 기준가 + `step` × 호가 단위.
    Limit {
        hynix: bool,
        selling: bool,
        quantity: u8,
        step: i8,
        timeout: u8,
    },
    /// 시장가 주문.
    Market {
        hynix: bool,
        selling: bool,
        quantity: u8,
    },
    /// 방금 체결된 시세(거래소 시각 = 받은 시각 = 지금).
    Quote { hynix: bool, step: i8 },
    /// 4차 2번: 거래소 시각이 없는 시세를 방금 받았다.
    Untimed { hynix: bool, step: i8 },
    /// 4차 3번: 그 종목의 가장 늦은 거래소 시각과 같은 시각의 (다를 수 있는) 값을 방금 받았다.
    SameTime { hynix: bool, step: i8 },
    /// 4차 4번: 거래소 시계가 `ahead`초 앞서고, 받은 것은 `back`초 전이다.
    Ahead {
        hynix: bool,
        step: i8,
        ahead: u8,
        back: u8,
    },
    /// 거래소 시각이 `back`초 전인 시세를 방금 받았다(늦게 도착한 거래).
    OldQuote { hynix: bool, step: i8, back: u8 },
    /// 그 종목에 앞서 준 시세 하나를 방금 받았다고 다시 준다(같은 시각의 값들이 번갈아 온다).
    Alternate { hynix: bool, pick: u8 },
    /// 바로 앞에 준 시세를 그대로 다시 준다.
    Requote,
    /// 지금까지 낸 주문 가운데 하나를 취소해 본다.
    Cancel { pick: u8 },
    /// 열린 지정가 주문 하나를 같은 값 · 남은 수량 그대로 정정해 본다.
    Modify { pick: u8 },
    /// 시간이 흐른다. `notify`가 거짓이면 알려 주지 않는다.
    Wait { seconds: u8, notify: bool },
    /// 결제일이 지난 달력을 넣었다가 월요일 달력으로 되돌린다.
    Settle,
    /// 상태를 내보냈다가 되살린 계좌로 갈아탄다.
    Reopen,
}

fn op() -> impl Strategy<Value = Op> {
    prop_oneof![
        4 => (any::<bool>(), any::<bool>(), 1u8..=12, -20i8..=20, 0u8..3).prop_map(
            |(hynix, selling, quantity, step, timeout)| Op::Limit {
                hynix,
                selling,
                quantity,
                step,
                timeout,
            }
        ),
        2 => (any::<bool>(), any::<bool>(), 1u8..=12).prop_map(|(hynix, selling, quantity)| {
            Op::Market {
                hynix,
                selling,
                quantity,
            }
        }),
        4 => (any::<bool>(), -20i8..=20).prop_map(|(hynix, step)| Op::Quote { hynix, step }),
        3 => (any::<bool>(), -20i8..=20).prop_map(|(hynix, step)| Op::Untimed { hynix, step }),
        3 => (any::<bool>(), -20i8..=20).prop_map(|(hynix, step)| Op::SameTime { hynix, step }),
        2 => (any::<bool>(), -20i8..=20, 1u8..=5, 1u8..=3).prop_map(
            |(hynix, step, ahead, back)| Op::Ahead {
                hynix,
                step,
                ahead,
                back,
            }
        ),
        2 => (any::<bool>(), -20i8..=20, 1u8..=40)
            .prop_map(|(hynix, step, back)| Op::OldQuote { hynix, step, back }),
        3 => (any::<bool>(), any::<u8>()).prop_map(|(hynix, pick)| Op::Alternate { hynix, pick }),
        1 => Just(Op::Requote),
        2 => any::<u8>().prop_map(|pick| Op::Cancel { pick }),
        2 => any::<u8>().prop_map(|pick| Op::Modify { pick }),
        3 => (1u8..=45, any::<bool>()).prop_map(|(seconds, notify)| Op::Wait { seconds, notify }),
        1 => Just(Op::Settle),
        1 => Just(Op::Reopen),
    ]
}

fn code(hynix: bool) -> &'static str {
    if hynix { HYN } else { SAM }
}

/// 기준가에서 호가 단위로 `step`칸 떨어진 값.
fn price(hynix: bool, step: i8) -> String {
    let (base, tick) = if hynix { (30_000, 50) } else { (70_000, 100) };
    (base + i64::from(step) * tick).to_string()
}

fn timeout(index: u8) -> UnfilledTimeout {
    match index {
        0 => UnfilledTimeout::SCALP,
        1 => UnfilledTimeout::SWING,
        _ => UnfilledTimeout::LONG_TERM,
    }
}

const MODES: [RoundingMode; 3] = [RoundingMode::Down, RoundingMode::HalfUp, RoundingMode::Up];

fn scenario_config(immediate: bool, max_fill: u8, mode: usize) -> PaperConfig {
    let mut config = config();
    config.initial_cash.krw = d("3000000");
    config.kr.rounding.mode = MODES[mode % MODES.len()];
    if immediate {
        config.kr.settlement.proceeds_usable = ProceedsUsable::Immediately;
    }
    config.kr.max_fill = if max_fill == 0 {
        MaxFill::Unlimited
    } else {
        MaxFill::PerQuote(Decimal::from(max_fill))
    };
    config
}

/// 동작을 차례로 돌리는 쪽. 시각은 뒤로 가지 않는다.
struct Runner {
    config: PaperConfig,
    desk: Desk,
    seconds: i64,
    placed: Vec<OrderId>,
    /// 바로 앞에 준(옛 시세가 아닌) 시세.
    last_quote: Option<Quote>,
    /// 시간으로 마감이 정해진 주문과 그 마감(초).
    deadlines: BTreeMap<String, i64>,
    /// 종목마다 받아들였어야 할 시세들(같은 거래소 시각 · 값은 하나로).
    history: BTreeMap<&'static str, Vec<Quote>>,
    /// 종목마다 가장 늦은 거래소 시각과, 그 시각에 이미 쓴 거래의 값들.
    latest: BTreeMap<&'static str, (DateTime<Utc>, Vec<Decimal>)>,
}

impl Runner {
    fn new(config: PaperConfig) -> Self {
        Self {
            desk: Desk::open(config.clone()),
            config,
            seconds: 0,
            placed: Vec::new(),
            last_quote: None,
            deadlines: BTreeMap::new(),
            history: BTreeMap::new(),
            latest: BTreeMap::new(),
        }
    }

    fn now(&self) -> DateTime<Utc> {
        at(self.seconds)
    }

    /// 시세를 주고 확인한다.
    /// - 거래소 시각이 그 종목의 가장 늦은 거래소 시각보다 앞이면(옛 시세) 체결이 없다.
    /// - 가장 늦은 거래소 시각과 같고 그 시각에 이미 쓴 값이면(쓴 거래) 체결이 없다.
    /// - 그 밖에 생긴 체결은 모두 받은 시각보다(거래소 시각이 있으면 그것보다도) 앞서 접수한 주문의 것이다.
    fn give(&mut self, symbol: &'static str, quote: Quote) -> Result<(), TestCaseError> {
        let now = self.now();
        let known = self.latest.get(symbol).cloned();
        let (older, used) = match (quote.exchange_time, &known) {
            (Some(traded_at), Some((latest, prices))) => (
                traded_at < *latest,
                traded_at == *latest && prices.contains(&quote.price),
            ),
            _ => (false, false),
        };
        let before = self.desk.broker.fills().len();
        let events = self.desk.on_quote(&quote, now);
        let fills = self.desk.broker.fills();
        if older || used {
            prop_assert_eq!(
                fills.len(),
                before,
                "옛 시세 또는 이미 쓴 거래로 체결됐다: {:?} {:?}",
                quote,
                events
            );
        }
        for fill in &fills[before..] {
            let order = self.desk.order_at(&fill.order_id, now);
            prop_assert!(
                order.ordered_at < quote.received_at,
                "받은 시각보다 뒤에 접수한 주문이 체결됐다: {:?} {:?}",
                quote,
                order
            );
            if let Some(traded_at) = quote.exchange_time {
                prop_assert!(
                    order.ordered_at < traded_at,
                    "거래소 시각보다 뒤에 접수한 주문이 체결됐다: {:?} {:?}",
                    quote,
                    order
                );
            }
        }
        if older {
            return Ok(());
        }
        if let Some(traded_at) = quote.exchange_time {
            match self.latest.get_mut(symbol) {
                Some((latest, prices)) if *latest == traded_at => {
                    if !prices.contains(&quote.price) {
                        prices.push(quote.price);
                    }
                }
                _ => {
                    self.latest.insert(symbol, (traded_at, vec![quote.price]));
                }
            }
        }
        let history = self.history.entry(symbol).or_default();
        history.retain(|known| {
            known.exchange_time != quote.exchange_time || known.price != quote.price
        });
        history.push(quote.clone());
        self.last_quote = Some(quote);
        Ok(())
    }

    fn apply(&mut self, op: &Op) -> Result<(), TestCaseError> {
        self.seconds += 1;
        let now = self.now();
        self.desk.set_clock(now);
        match *op {
            Op::Limit {
                hynix,
                selling,
                quantity,
                step,
                timeout: index,
            } => {
                let quantity = quantity.to_string();
                let price = price(hynix, step);
                let req = if selling {
                    sell(code(hynix), &quantity, &price)
                } else {
                    buy(code(hynix), &quantity, &price)
                };
                if let Ok(ack) = self.desk.place(&req.timeout(timeout(index)), now) {
                    let wait = match index {
                        0 => Some(60),
                        1 => Some(600),
                        _ => None,
                    };
                    if let Some(wait) = wait {
                        self.deadlines
                            .insert(ack.order_id.as_str().to_owned(), self.seconds + wait);
                    }
                    self.placed.push(ack.order_id);
                }
            }
            Op::Market {
                hynix,
                selling,
                quantity,
            } => {
                let quantity = quantity.to_string();
                let req = if selling {
                    market_sell(code(hynix), &quantity)
                } else {
                    market_buy(code(hynix), &quantity)
                };
                if let Ok(ack) = self.desk.place(&req, now) {
                    self.placed.push(ack.order_id);
                }
            }
            Op::Quote { hynix, step } => {
                let quote = quote_received(code(hynix), &price(hynix, step), Currency::Krw, now);
                self.give(code(hynix), quote)?;
            }
            Op::Untimed { hynix, step } => {
                let quote = quote_full(code(hynix), &price(hynix, step), Currency::Krw, None, now);
                self.give(code(hynix), quote)?;
            }
            Op::SameTime { hynix, step } => {
                if let Some((latest, _)) = self.latest.get(code(hynix)).cloned() {
                    let quote = quote_full(
                        code(hynix),
                        &price(hynix, step),
                        Currency::Krw,
                        Some(latest),
                        now,
                    );
                    self.give(code(hynix), quote)?;
                }
            }
            Op::Ahead {
                hynix,
                step,
                ahead,
                back,
            } => {
                let quote = quote_full(
                    code(hynix),
                    &price(hynix, step),
                    Currency::Krw,
                    Some(now + chrono::Duration::seconds(i64::from(ahead))),
                    now - chrono::Duration::seconds(i64::from(back)),
                );
                self.give(code(hynix), quote)?;
            }
            Op::OldQuote { hynix, step, back } => {
                let quote = quote_full(
                    code(hynix),
                    &price(hynix, step),
                    Currency::Krw,
                    Some(now - chrono::Duration::seconds(i64::from(back))),
                    now,
                );
                self.give(code(hynix), quote)?;
            }
            Op::Alternate { hynix, pick } => {
                let earlier = self.history.get(code(hynix)).cloned().unwrap_or_default();
                if !earlier.is_empty() {
                    let old = &earlier[usize::from(pick) % earlier.len()];
                    let again = Quote {
                        received_at: now,
                        ..old.clone()
                    };
                    self.give(code(hynix), again)?;
                }
            }
            Op::Requote => {
                if let Some(quote) = self.last_quote.clone() {
                    let before = self.desk.broker.fills().len();
                    let events = self.desk.on_quote(&quote, now);
                    prop_assert_eq!(
                        self.desk.broker.fills().len(),
                        before,
                        "같은 시세로 다시 체결됐다: {:?}",
                        events
                    );
                }
            }
            Op::Cancel { pick } => {
                if !self.placed.is_empty() {
                    let index = usize::from(pick) % self.placed.len();
                    let _ = self.desk.cancel(&self.placed[index], now);
                }
            }
            Op::Modify { pick } => self.modify(pick)?,
            Op::Wait { seconds, notify } => {
                self.seconds += i64::from(seconds);
                if notify {
                    self.desk.on_time(self.now());
                } else {
                    self.desk.set_clock(self.now());
                }
            }
            Op::Settle => {
                self.desk
                    .broker
                    .set_calendar(kr_calendar(day(2026, 11, 20)));
                self.desk.broker.set_calendar(kr_calendar(monday()));
            }
            Op::Reopen => {
                self.desk = self.desk.reopen(self.config.clone());
            }
        }
        Ok(())
    }

    /// 같은 값 · 남은 수량 그대로의 정정. 거부되면 아무것도 바뀌지 않는다. 4차 1번((가)): 돈 때문에
    /// 거부되지 않는 것은 원 미만 버림일 때만 단언한다.
    fn modify(&mut self, pick: u8) -> Result<(), TestCaseError> {
        let now = self.now();
        self.desk.on_time(now);
        let open: Vec<Order> = self
            .desk
            .orders_at(OrderGroup::Open, now)
            .into_iter()
            .filter(|order| order.order_type == OrderType::Limit)
            .collect();
        if open.is_empty() {
            return Ok(());
        }
        let order = &open[usize::from(pick) % open.len()];
        let rest = remaining(order).to_string();
        let price = order.price.unwrap().to_string();
        let before = self.desk.broker.snapshot();
        match self
            .desk
            .modify(&order.order_id, Some(&rest), Some(&price), &rest, now)
        {
            Ok(new_id) => {
                if let Some(deadline) = self.deadlines.get(order.order_id.as_str()).copied() {
                    self.deadlines.insert(new_id.as_str().to_owned(), deadline);
                }
                self.placed.push(new_id);
            }
            Err(error) => {
                prop_assert_eq!(
                    self.desk.broker.snapshot(),
                    before,
                    "거부된 정정이 상태를 바꿨다: {:?}",
                    error
                );
                if self.config.kr.rounding.mode == RoundingMode::Down {
                    prop_assert!(
                        error != BrokerError::InsufficientBuyingPower,
                        "버림인데 같은 값 · 같은 수량 정정이 돈 때문에 거부됐다: {:?}",
                        order
                    );
                }
                prop_assert!(error != BrokerError::InsufficientSellableQuantity);
            }
        }
        Ok(())
    }

    /// 마감이 된 주문은 시간을 알려 주지 않았어도 열린 채로 보이지 않는다.
    fn check_deadlines(&self) -> Result<(), TestCaseError> {
        self.desk.set_clock(self.now());
        for order in self.desk.orders(OrderGroup::Open) {
            if let Some(deadline) = self.deadlines.get(order.order_id.as_str()) {
                prop_assert!(
                    self.seconds < *deadline,
                    "마감이 지났는데 열려 있다: {order:?}"
                );
            }
        }
        Ok(())
    }
}

/// `금액 × 비율`을 그 방향으로 원 단위에 맞춘 값.
fn charge(amount: Decimal, rate: &str, mode: RoundingMode) -> Decimal {
    let strategy = match mode {
        RoundingMode::Down => RoundingStrategy::ToZero,
        RoundingMode::HalfUp => RoundingStrategy::MidpointAwayFromZero,
        RoundingMode::Up => RoundingStrategy::AwayFromZero,
    };
    (amount * d(rate)).round_dp_with_strategy(0, strategy)
}

fn remaining(order: &Order) -> Decimal {
    order.quantity - order.execution.filled_quantity
}

/// 지정가로 사는 열린 주문이 묶는 돈: 남은 수량 × 지정가 + 올림((누적 체결 금액 + 남은 수량 ×
/// 지정가) × 0.015%) − 이미 뗀 수수료.
fn limit_hold(order: &Order) -> Decimal {
    let notional = remaining(order) * order.price.unwrap();
    let whole = order.execution.filled_amount + notional;
    notional + (whole * d("0.00015")).ceil() - order.execution.commission
}

/// 돈과 수량이 어긋나지 않았는지 공개 API만으로 확인한다.
fn check(
    desk: &Desk,
    initial: Decimal,
    mode: RoundingMode,
    now: DateTime<Utc>,
) -> Result<(), TestCaseError> {
    desk.set_clock(now);
    let cash = desk.krw();
    let fills = desk.broker.fills();
    let open = desk.orders(OrderGroup::Open);
    let closed = desk.orders(OrderGroup::Closed);
    let zero = d("0");

    prop_assert!(cash.available >= zero, "쓸 수 있는 돈이 음수: {cash:?}");
    prop_assert!(cash.held >= zero, "묶인 돈이 음수: {cash:?}");
    prop_assert!(cash.unsettled >= zero, "결제 전인 돈이 음수: {cash:?}");

    let mut expected_total = initial;
    let mut net: BTreeMap<String, Decimal> = BTreeMap::new();
    let mut filled_by_order: BTreeMap<String, Decimal> = BTreeMap::new();
    for fill in &fills {
        prop_assert!(fill.quantity > zero);
        prop_assert_eq!(fill.amount, fill.quantity * fill.price);
        let position = net.entry(fill.symbol.as_str().to_owned()).or_default();
        match fill.side {
            Side::Buy => {
                expected_total -= fill.amount + fill.commission;
                *position += fill.quantity;
            }
            Side::Sell => {
                expected_total += fill.amount - fill.commission - fill.tax;
                *position -= fill.quantity;
            }
        }
        *filled_by_order
            .entry(fill.order_id.as_str().to_owned())
            .or_default() += fill.quantity;
    }
    prop_assert_eq!(cash.total(), expected_total, "돈이 생기거나 사라졌다");

    for order in open.iter().chain(closed.iter()) {
        let filled = order.execution.filled_quantity;
        prop_assert!(filled >= zero && filled <= order.quantity, "{order:?}");
        let recorded = filled_by_order
            .get(order.order_id.as_str())
            .copied()
            .unwrap_or(zero);
        prop_assert_eq!(filled, recorded, "체결 기록과 주문의 체결 수량이 다르다");
        prop_assert_eq!(
            order.execution.commission,
            charge(order.execution.filled_amount, "0.00015", mode),
            "수수료가 한 번에 체결됐을 때와 다르다: {:?}",
            order
        );
        for fill in fills.iter().filter(|fill| fill.order_id == order.order_id) {
            prop_assert!(fill.filled_at > order.ordered_at, "{:?} {:?}", fill, order);
        }
    }
    for order in &closed {
        if order.order_type == OrderType::Limit {
            prop_assert!(
                order.status != OrderStatus::Rejected
                    && order.close_reason != Some(CloseReason::InsufficientBuyingPower),
                "지정가 주문이 거부로 끝났다: {:?}",
                order
            );
        }
    }

    for symbol in [SAM, HYN] {
        let owned = net.get(symbol).copied().unwrap_or(zero);
        prop_assert!(owned >= zero, "없는 주식을 팔았다: {symbol} {owned}");
        let holding = desk
            .holding(symbol)
            .map_or(zero, |holding| holding.quantity);
        prop_assert_eq!(holding, owned, "보유가 체결 기록과 다르다: {}", symbol);
        let locked: Decimal = open
            .iter()
            .filter(|order| order.symbol.as_str() == symbol && order.side == Side::Sell)
            .map(remaining)
            .sum();
        prop_assert_eq!(
            holding - desk.sellable(symbol),
            locked,
            "묶인 수량이 다르다: {}",
            symbol
        );
    }

    let open_buys: Vec<&Order> = open
        .iter()
        .filter(|order| order.side == Side::Buy)
        .collect();
    let limit_holds: Decimal = open_buys
        .iter()
        .filter(|order| order.order_type == OrderType::Limit)
        .map(|order| limit_hold(order))
        .sum();
    if open_buys
        .iter()
        .all(|order| order.order_type == OrderType::Limit)
    {
        prop_assert_eq!(cash.held, limit_holds, "묶인 돈이 열린 주문과 다르다");
    } else {
        prop_assert!(
            cash.held >= limit_holds,
            "지정가 주문이 묶은 돈보다 적게 묶였다"
        );
    }
    Ok(())
}

proptest! {
    // 실패한 씨앗을 테스트 폴더에 파일로 남기지 않는다(잠긴 폴더가 바뀌면 안 된다).
    #![proptest_config(ProptestConfig {
        cases: 96,
        failure_persistence: None,
        ..ProptestConfig::default()
    })]

    // 검증: C10, E12, E25, E29, N4, N6, V6
    #[test]
    fn 시각_없는_시세와_같은_시각의_다른_값을_섞어도_돈과_수량이_어긋나지_않는다(
        immediate in any::<bool>(),
        max_fill in 0u8..=4,
        mode in 0usize..3,
        ops in proptest::collection::vec(op(), 1..60),
    ) {
        let config = scenario_config(immediate, max_fill, mode);
        let initial = config.initial_cash.krw;
        let mode = config.kr.rounding.mode;
        let mut runner = Runner::new(config);
        check(&runner.desk, initial, mode, runner.now())?;
        for op in &ops {
            runner.apply(op)?;
            check(&runner.desk, initial, mode, runner.now())?;
            runner.check_deadlines()?;
        }
        let now = runner.now();
        for order in runner.desk.orders(OrderGroup::Open) {
            runner.desk.cancel(&order.order_id, now).unwrap();
        }
        check(&runner.desk, initial, mode, now)?;
        prop_assert_eq!(runner.desk.krw().held, d("0"));
    }

    // 검증: V6
    #[test]
    fn 시각_없는_시세와_같은_시각의_값을_섞어도_껐다_켠_계좌가_같은_결과를_낸다(
        immediate in any::<bool>(),
        max_fill in 0u8..=4,
        mode in 0usize..3,
        before in proptest::collection::vec(op(), 0..30),
        after in proptest::collection::vec(op(), 1..30),
    ) {
        let mut straight = Runner::new(scenario_config(immediate, max_fill, mode));
        let mut restarted = Runner::new(scenario_config(immediate, max_fill, mode));
        for op in &before {
            straight.apply(op)?;
            restarted.apply(op)?;
        }
        restarted.desk = restarted.desk.reopen(restarted.config.clone());
        for op in &after {
            straight.apply(op)?;
            restarted.apply(op)?;
        }
        prop_assert_eq!(restarted.desk.broker.snapshot(), straight.desk.broker.snapshot());
        prop_assert_eq!(restarted.desk.broker.fills(), straight.desk.broker.fills());
        prop_assert_eq!(restarted.placed, straight.placed);
    }
}
