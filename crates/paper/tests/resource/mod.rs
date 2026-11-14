//! 2차 10~12번: 되살리기 검사(V6)와 자원(N1).
//! 말이 안 되는 상태 값은 되살리지 않고(10), 끝난 것은 덜어 낼 수 있고(11), 체결 알림은 끝없이
//! 쌓이지 않는다(12). 24시간 돌려도 계좌와 상태 값이 늘지 않아야 한다.

use chrono::{DateTime, Utc};
use pi_market::Currency;
use pi_paper::{
    Archived, BrokerError, CheckAuthority, OrderEventKind, OrderGroup, OrderId, OrderStatus,
    PaperBroker, Snapshot, SnapshotReason, UnfilledTimeout,
};
use serde_json::{Value, json};
use tokio::sync::mpsc::error::TryRecvError;

use crate::support::{
    Desk, HYN, SAM, at, buy, cash, config, d, day, kinds, kr_calendar, monday, sell, utc,
};

// ---------------------------------------------------------------------------------------------
// 10. 말이 안 되는 상태 값은 되살리지 않는다
// 상태 값의 속은 공개돼 있지 않아, 글자(JSON)로 바꾼 값을 고쳐서 만든다. 쓰는 이름은 `Snapshot`
// 문서가 바뀌지 않는다고 적은 것뿐이다: `ledger.next_id`, `ledger.holdings[].quantity`,
// `ledger.orders[].status` · `quantity` · `held_cash`.
// ---------------------------------------------------------------------------------------------

/// 계좌의 상태 값을 글자로 바꿔 `edit`로 고친 뒤 되살려 본다.
fn restore_edited(
    desk: &Desk,
    edit: impl FnOnce(&mut Value),
) -> Result<(PaperBroker, CheckAuthority), BrokerError> {
    let mut value = serde_json::to_value(desk.broker.snapshot()).unwrap();
    edit(&mut value);
    let snapshot: Snapshot = serde_json::from_value(value).unwrap();
    PaperBroker::restore(config(), snapshot)
}

fn refused(reason: SnapshotReason) -> Option<BrokerError> {
    Some(BrokerError::InvalidSnapshot(reason))
}

/// 열린 사는 주문 하나(1주 × 70,000, 묶은 돈 70,011)가 있는 계좌.
fn one_open_buy() -> Desk {
    let desk = Desk::standard();
    desk.accepted(&buy(SAM, "1", "70000"), at(0));
    desk
}

/// 취소된 주문 하나만 있는 계좌.
fn one_canceled() -> Desk {
    let desk = Desk::standard();
    let order_id = desk.accepted(&buy(SAM, "1", "70000"), at(0));
    desk.cancel(&order_id, at(1)).unwrap();
    desk
}

/// 삼성 10주를 갖고 그 10주를 파는 열린 주문이 있는 계좌.
fn holding_with_open_sell() -> Desk {
    let desk = Desk::standard();
    desk.own(SAM, "10", "70000", at(0));
    desk.accepted(&sell(SAM, "10", "71000"), at(1));
    desk
}

// 검증: V6
// 고치지 않은 값은 같은 길(글자 → 값 → 되살리기)로 되살아난다. 아래 테스트들이 값의 이름을 맞게
// 짚고 있는지도 여기서 확인한다(없는 이름이면 이 단언이 실패한다).
#[test]
fn 고치지_않은_상태_값은_되살아나고_테스트가_고칠_이름이_들어_있다() {
    for desk in [one_open_buy(), one_canceled(), holding_with_open_sell()] {
        assert!(restore_edited(&desk, |_| {}).is_ok());
    }
    let value = serde_json::to_value(holding_with_open_sell().broker.snapshot()).unwrap();
    assert!(value["ledger"]["next_id"].is_u64());
    assert_eq!(value["ledger"]["holdings"][0]["quantity"], json!("10"));
    let orders = value["ledger"]["orders"].as_array().unwrap();
    assert_eq!(orders.len(), 2);
    assert_eq!(orders[1]["quantity"], json!("10"));
    assert_eq!(orders[1]["held_cash"], json!("0"));
    assert_eq!(orders[1]["status"], json!("Pending"));
    let value = serde_json::to_value(one_open_buy().broker.snapshot()).unwrap();
    assert_eq!(value["ledger"]["orders"][0]["held_cash"], json!("70011"));
    assert_eq!(value["ledger"]["next_id"], json!(2));
}

// 검증: V6
// 1차 검토의 실험: 다음 식별자를 1로 바꿔도 되살리기가 통과했고, 새 주문이 앞의 주문과 같은 식별자를 받았다.
#[test]
fn 다음_식별자가_이미_낸_식별자보다_크지_않으면_되살리지_않는다() {
    let desk = one_open_buy();
    let result = restore_edited(&desk, |value| value["ledger"]["next_id"] = json!(1));
    assert_eq!(result.err(), refused(SnapshotReason::NextIdNotAhead));

    let result = restore_edited(&desk, |value| value["ledger"]["next_id"] = json!(0));
    assert_eq!(result.err(), refused(SnapshotReason::NextIdNotAhead));
}

// 검증: V6
// 1차 검토의 실험: 끝난 주문의 묶인 돈을 5로 바꿔도 되살리기가 통과했다(없던 돈 5원이 생긴다).
#[test]
fn 끝난_주문에_묶인_돈이_남아_있으면_되살리지_않는다() {
    let desk = one_canceled();
    let result = restore_edited(&desk, |value| {
        value["ledger"]["orders"][0]["held_cash"] = json!("5");
    });
    assert_eq!(result.err(), refused(SnapshotReason::ClosedOrderHoldsCash));
}

// 검증: V6
// 실험에서 함께 고친 값 그대로: 다음 식별자 1, 열린 주문을 체결됨으로 바꾸고 묶인 돈 5를 남김.
#[test]
fn 여러_곳이_어긋난_상태_값도_되살리지_않는다() {
    let desk = one_open_buy();
    let result = restore_edited(&desk, |value| {
        value["ledger"]["next_id"] = json!(1);
        value["ledger"]["orders"][0]["held_cash"] = json!("5");
        value["ledger"]["orders"][0]["status"] = json!("Filled");
    });
    assert!(matches!(
        result.err(),
        Some(BrokerError::InvalidSnapshot(_))
    ));
}

// 검증: V6
#[test]
fn 열린_파는_주문의_합이_보유를_넘으면_되살리지_않는다() {
    let desk = holding_with_open_sell();
    // 10주를 파는 주문이 열려 있는데 보유는 5주.
    let result = restore_edited(&desk, |value| {
        value["ledger"]["holdings"][0]["quantity"] = json!("5");
    });
    assert_eq!(result.err(), refused(SnapshotReason::SellExceedsHolding));

    // 보유가 아예 없는 것도 같다.
    let result = restore_edited(&desk, |value| value["ledger"]["holdings"] = json!([]));
    assert_eq!(result.err(), refused(SnapshotReason::SellExceedsHolding));
}

// 검증: V6
#[test]
fn 같은_종목의_보유가_둘이면_되살리지_않는다() {
    let desk = Desk::standard();
    desk.own(SAM, "10", "70000", at(0));
    let result = restore_edited(&desk, |value| {
        let holdings = value["ledger"]["holdings"].as_array_mut().unwrap();
        let copy = holdings[0].clone();
        holdings.push(copy);
    });
    assert_eq!(result.err(), refused(SnapshotReason::DuplicateHolding));
}

// 검증: V6
#[test]
fn 주문_수량이_0_이하이면_되살리지_않는다() {
    // 끝난 주문(묶인 것 없음, 체결 0)의 수량만 0으로 바꾼다.
    let desk = one_canceled();
    let result = restore_edited(&desk, |value| {
        value["ledger"]["orders"][0]["quantity"] = json!("0");
    });
    assert_eq!(
        result.err(),
        refused(SnapshotReason::NonPositiveOrderQuantity)
    );

    // 음수 수량은 어느 이유로든 되살리지 않는다.
    let result = restore_edited(&desk, |value| {
        value["ledger"]["orders"][0]["quantity"] = json!("-3");
    });
    assert!(matches!(
        result.err(),
        Some(BrokerError::InvalidSnapshot(_))
    ));
}

// 검증: V6
// "묶인 돈의 합이 현금을 넘음": 이 계좌의 상태 값은 묶인 돈을 주문마다 적고 쓸 수 있는 돈과 따로 둔다.
// 그래서 묶인 돈을 부풀리면 계좌의 돈이 그만큼 생긴다. 열린 주문이 그 주문 전체에 필요한 돈
// (1주 × 70,000 + 수수료 11 = 70,011)보다 많이 묶고 있으면 되살리지 않는다.
#[test]
fn 열린_주문이_필요한_돈보다_많이_묶고_있으면_되살리지_않는다() {
    let desk = one_open_buy();
    let result = restore_edited(&desk, |value| {
        value["ledger"]["orders"][0]["held_cash"] = json!("999999999999");
    });
    assert_eq!(result.err(), refused(SnapshotReason::HeldCashTooLarge));

    // 1원만 많아도 같다.
    let result = restore_edited(&desk, |value| {
        value["ledger"]["orders"][0]["held_cash"] = json!("70012");
    });
    assert_eq!(result.err(), refused(SnapshotReason::HeldCashTooLarge));
}

// ---------------------------------------------------------------------------------------------
// 11. 끝난 것은 덜어 낼 수 있다
// ---------------------------------------------------------------------------------------------

fn order_error(desk: &Desk, order_id: &OrderId, now: DateTime<Utc>) -> Option<BrokerError> {
    desk.run(desk.as_broker().order(order_id, now)).err()
}

// 검증: N1
#[test]
fn 끝난_주문과_체결_기록을_덜어_내면_돌려받고_계좌에서는_사라진다() {
    let desk = Desk::standard();
    let filled = desk.accepted(&buy(SAM, "2", "70000"), at(0));
    desk.quote(SAM, "70000", at(1));
    let canceled = desk.accepted(&buy(HYN, "1", "30000"), at(2));
    desk.cancel(&canceled, at(3)).unwrap();
    // 오래됐지만 아직 열려 있는 주문.
    let open = desk.accepted(&buy(HYN, "1", "29950"), at(10));
    // 끝났지만 기준 시각(15초)보다 뒤에 접수한 주문.
    let recent = desk.accepted(&buy(SAM, "1", "70000"), at(20));
    desk.quote(SAM, "70000", at(21));

    let closed_before = desk.orders(OrderGroup::Closed);
    let fills_before = desk.broker.fills();
    assert_eq!(closed_before.len(), 3);
    assert_eq!(fills_before.len(), 2);
    let cash_before = desk.krw();
    let holdings_before = desk.holdings();

    let archived = desk.broker.archive_before(at(15));
    assert_eq!(
        archived,
        Archived {
            orders: closed_before[..2].to_vec(),
            fills: fills_before[..1].to_vec(),
        }
    );
    assert_eq!(archived.orders[0].order_id, filled);
    assert_eq!(archived.orders[0].status, OrderStatus::Filled);
    assert_eq!(archived.orders[1].order_id, canceled);
    assert_eq!(archived.fills[0].order_id, filled);

    // 계좌에는 덜어 내지 않은 것만 남는다.
    assert_eq!(desk.orders(OrderGroup::Closed), closed_before[2..].to_vec());
    assert_eq!(desk.orders(OrderGroup::Closed)[0].order_id, recent);
    assert_eq!(desk.broker.fills(), fills_before[1..].to_vec());
    assert_eq!(
        order_error(&desk, &filled, at(21)),
        Some(BrokerError::OrderNotFound)
    );
    assert_eq!(
        order_error(&desk, &canceled, at(21)),
        Some(BrokerError::OrderNotFound)
    );

    // 열린 주문 · 돈 · 보유는 그대로다.
    let still_open = desk.orders(OrderGroup::Open);
    assert_eq!(still_open.len(), 1);
    assert_eq!(still_open[0].order_id, open);
    assert_eq!(desk.krw(), cash_before);
    assert_eq!(desk.holdings(), holdings_before);
    assert_eq!(desk.sellable(SAM), d("3"));

    // 같은 기준으로 다시 부르면 덜어 낼 것이 없다.
    let again = desk.broker.archive_before(at(15));
    assert!(again.orders.is_empty());
    assert!(again.fills.is_empty());

    // 상태 값에서도 사라졌다: 되살려도 덜어 낸 주문은 없다.
    let after = desk.reopen(config());
    assert_eq!(after.orders(OrderGroup::Closed).len(), 1);
    assert_eq!(after.broker.fills().len(), 1);
    assert_eq!(after.krw(), cash_before);
}

/// 주문 · 체결 · 취소가 섞인 앞부분.
fn history(desk: &Desk) {
    desk.own(SAM, "5", "70000", at(0));
    let canceled = desk.accepted(&buy(HYN, "3", "30000"), at(2));
    desk.cancel(&canceled, at(3)).unwrap();
    desk.accepted(&sell(SAM, "2", "70500"), at(4));
    desk.quote(SAM, "70500", at(5));
    // 열어 둔 주문.
    desk.accepted(&buy(HYN, "4", "29900"), at(6));
}

// 검증: N1, V6
#[test]
fn 덜어_낸_뒤에도_같은_입력에_덜어_내지_않은_계좌와_같은_결과가_나온다() {
    let kept = Desk::standard();
    let trimmed = Desk::standard();
    history(&kept);
    history(&trimmed);

    let archived = trimmed.broker.archive_before(at(100));
    assert_eq!(archived.orders.len(), 3, "산 주문 · 취소된 주문 · 판 주문");
    assert_eq!(archived.fills.len(), 2);

    for desk in [&kept, &trimmed] {
        desk.quote(HYN, "30000", at(10));
    }
    let kept_events = kept.quote(HYN, "29900", at(11));
    let trimmed_events = trimmed.quote(HYN, "29900", at(11));
    assert_eq!(kinds(&kept_events), vec![OrderEventKind::Fill]);
    assert_eq!(trimmed_events, kept_events);

    // 새 주문의 식별자도 같다(다음 번호는 덜어 내도 그대로 이어진다).
    let kept_new = kept.accepted(&sell(SAM, "3", "71000"), at(12));
    let trimmed_new = trimmed.accepted(&sell(SAM, "3", "71000"), at(12));
    assert_eq!(trimmed_new, kept_new);
    assert_eq!(
        trimmed.quote(SAM, "71000", at(13)),
        kept.quote(SAM, "71000", at(13))
    );

    assert_eq!(trimmed.krw(), kept.krw());
    assert_eq!(trimmed.holdings(), kept.holdings());
    assert_eq!(
        trimmed.orders(OrderGroup::Open),
        kept.orders(OrderGroup::Open)
    );
    assert_eq!(trimmed.sellable(SAM), kept.sellable(SAM));

    // 덜어 내는 때가 달라도 끝에 다 덜어 내면 상태 값이 같다.
    kept.broker.archive_before(at(100));
    trimmed.broker.archive_before(at(100));
    assert_eq!(trimmed.broker.snapshot(), kept.broker.snapshot());
}

// 검증: N1
#[test]
fn 덜어_내도_살아_있는_멱등성_키는_앞의_결과를_돌려준다() {
    let desk = Desk::standard();
    let req = buy(SAM, "1", "70000").key("order-001");
    let first = desk.place(&req, at(0)).unwrap();
    desk.quote(SAM, "70000", at(1));
    let archived = desk.broker.archive_before(at(5));
    assert_eq!(archived.orders.len(), 1);
    let cash_after = desk.krw();

    // 같은 키 · 같은 내용을 10분 안에 다시 내면 새 주문 없이 앞의 식별자를 돌려준다.
    let again = desk.place(&req, at(6)).unwrap();
    assert_eq!(again, first);
    assert!(desk.orders(OrderGroup::Open).is_empty());
    assert!(desk.orders(OrderGroup::Closed).is_empty());
    assert_eq!(desk.krw(), cash_after);
}

/// 상태 값을 글자로 바꾼 것.
fn snapshot_text(desk: &Desk) -> String {
    serde_json::to_string(&desk.broker.snapshot()).unwrap()
}

// 검증: N1
// 지난 멱등성 키는 덜어 내기를 부르지 않아도 스스로 지워진다. 키 글자가 상태 값에 남는 곳은
// 그 주문의 기록 한 곳뿐이어야 한다.
#[test]
fn 지난_멱등성_키는_스스로_지워진다() {
    let desk = Desk::standard();
    let key = "self-cleaning-key-0001";
    let order_id = desk.accepted(&buy(SAM, "1", "69000").key(key), at(0));
    assert!(snapshot_text(&desk).contains(key));

    // 10분이 되면 그 키는 죽는다. 시각만 알려 줘도 버린다.
    desk.on_time(at(600));
    assert_eq!(
        snapshot_text(&desk).matches(key).count(),
        1,
        "주문 기록에만 남는다"
    );
    assert_eq!(desk.order(&order_id).client_order_id.as_deref(), Some(key));

    // 그 주문까지 끝나서 덜어 내면 키의 자취가 없다.
    desk.cancel(&order_id, at(601)).unwrap();
    desk.broker.archive_before(at(602));
    assert!(!snapshot_text(&desk).contains(key));
}

// 검증: N1
// 나흘 동안 영업일마다 40번씩 사고 · 팔고 · 안 체결되는 주문을 내면서 조금 지난 것을 덜어 낸다.
// 끝난 주문 · 체결 기록 · 멱등성 키가 쌓이지 않아 상태 값의 크기가 날이 가도 늘지 않는다.
// 한 번: 1주를 70,000에 사고(수수료 10) 70,000에 판다(수수료 10, 거래세 140) → 160원이 든다.
#[test]
fn 오래_돌려도_덜어_내면_계좌와_상태_값이_늘지_않는다() {
    const ROUNDS: i64 = 40;
    let days = [
        monday(),
        day(2026, 11, 17),
        day(2026, 11, 18),
        day(2026, 11, 19),
    ];
    let desk = Desk::bare(config());
    let mut archived_orders = 0;
    let mut archived_fills = 0;
    let mut sizes = Vec::new();

    for date in days {
        desk.broker.set_calendar(kr_calendar(date));
        desk.reference(date);
        for round in 0..ROUNDS {
            // 00:10 UTC부터 500초마다 한 번.
            let now = |seconds: i64| {
                utc(date, 0, 10, 0) + chrono::Duration::seconds(round * 500 + seconds)
            };
            let key = |what: &str| format!("{date}-{round}-{what}");

            // 700초보다 오래된 것을 덜어 낸다.
            let archived = desk
                .broker
                .archive_before(now(0) - chrono::Duration::seconds(700));
            archived_orders += archived.orders.len();
            archived_fills += archived.fills.len();

            desk.accepted(&buy(SAM, "1", "70000").key(&key("buy")), now(0));
            desk.quote(SAM, "70000", now(1));
            desk.accepted(&sell(SAM, "1", "70000").key(&key("sell")), now(2));
            desk.quote(SAM, "70000", now(3));
            // 체결되지 않고 1분 뒤 마감이 되는 주문. 다음 번의 호출이 먼저 취소한다.
            desk.accepted(
                &buy(HYN, "1", "29000")
                    .timeout(UnfilledTimeout::SCALP)
                    .key(&key("idle")),
                now(4),
            );

            // 계좌에 남은 것은 몇 번 치뿐이다.
            let closed = desk.orders(OrderGroup::Closed).len();
            let fills = desk.broker.fills().len();
            assert!(closed <= 9, "{date} {round}번째: 끝난 주문 {closed}개");
            assert!(fills <= 6, "{date} {round}번째: 체결 기록 {fills}개");
        }
        sizes.push(snapshot_text(&desk).len());
    }

    // 잃어버린 것이 없다: 덜어 낸 것 + 남은 것 = 낸 것 전부.
    let rounds = usize::try_from(ROUNDS).unwrap() * days.len();
    let left_orders = desk.orders(OrderGroup::Closed).len() + desk.orders(OrderGroup::Open).len();
    assert_eq!(archived_orders + left_orders, rounds * 3);
    assert_eq!(archived_fills + desk.broker.fills().len(), rounds * 2);

    // 돈은 든 만큼만 줄었다: 160번 × 160원 = 25,600원.
    assert_eq!(desk.krw().total(), d("9974400"));
    assert!(desk.holdings().is_empty());

    // 상태 값의 크기는 날이 가도 늘지 않는다(둘째 날부터는 결제 전인 돈의 묶음 수까지 같다).
    // 돈의 자릿수 같은 몇 글자의 차이만 허용한다.
    assert!(
        sizes[3] <= sizes[1] + 16,
        "상태 값이 날마다 커진다: {sizes:?}"
    );
    assert!(
        sizes[2] <= sizes[1] + 16,
        "상태 값이 날마다 커진다: {sizes:?}"
    );
}

// ---------------------------------------------------------------------------------------------
// 12. 체결 알림은 끝없이 쌓이지 않는다
// ---------------------------------------------------------------------------------------------

/// 구독 하나가 이벤트를 `buffer`개까지만 쥐는 계좌.
fn buffered(buffer: usize) -> Desk {
    let mut config = config();
    config.event_buffer = buffer;
    Desk::open(config)
}

/// 주문 하나를 내 접수 알림 하나를 만든다.
fn one_event(desk: &Desk, second: i64) -> OrderId {
    desk.accepted(&buy(SAM, "1", "69000"), at(second))
}

// 검증: N1
#[test]
fn 읽지_않는_구독은_정한_개수까지만_쥐고_그_구독만_끊긴다() {
    let desk = buffered(3);
    let mut idle = desk.as_broker().subscribe();
    let mut reader = desk.as_broker().subscribe();

    // 알림 5개. 한쪽은 올 때마다 읽고, 다른 쪽은 읽지 않는다.
    let mut placed = Vec::new();
    for second in 0..5 {
        let order_id = one_event(&desk, second);
        let event = reader.try_recv().unwrap();
        assert_eq!(event.kind, OrderEventKind::Pending);
        assert_eq!(event.order.order_id, order_id);
        placed.push(order_id);
    }

    // 읽지 않은 쪽: 처음 3개만 쥐고 있고, 그 뒤로는 끊겼다.
    for expected in &placed[..3] {
        assert_eq!(&idle.try_recv().unwrap().order.order_id, expected);
    }
    assert_eq!(idle.try_recv().err(), Some(TryRecvError::Disconnected));

    // 읽는 쪽과 계좌는 그대로다.
    assert_eq!(reader.try_recv().err(), Some(TryRecvError::Empty));
    let next = one_event(&desk, 5);
    assert_eq!(reader.try_recv().unwrap().order.order_id, next);
    assert_eq!(desk.orders(OrderGroup::Open).len(), 6);

    // 끊긴 쪽은 다시 구독하면 그 뒤의 알림을 받는다.
    let mut again = desk.as_broker().subscribe();
    let later = one_event(&desk, 6);
    assert_eq!(again.try_recv().unwrap().order.order_id, later);
    assert_eq!(idle.try_recv().err(), Some(TryRecvError::Disconnected));
}

// 검증: N1
#[test]
fn 읽는_구독은_정한_개수보다_많은_알림이_지나가도_끊기지_않는다() {
    let desk = buffered(3);
    let mut reader = desk.as_broker().subscribe();
    for second in 0..10 {
        let order_id = one_event(&desk, second);
        assert_eq!(reader.try_recv().unwrap().order.order_id, order_id);
    }
    assert_eq!(reader.try_recv().err(), Some(TryRecvError::Empty));
}

// 검증: N1
#[test]
fn 딱_정한_개수까지_밀려도_끊기지_않는다() {
    let desk = buffered(3);
    let mut subscriber = desk.as_broker().subscribe();
    let placed: Vec<OrderId> = (0..3).map(|second| one_event(&desk, second)).collect();

    for expected in &placed {
        assert_eq!(&subscriber.try_recv().unwrap().order.order_id, expected);
    }
    assert_eq!(subscriber.try_recv().err(), Some(TryRecvError::Empty));

    // 비운 뒤에는 다시 3개까지 쥔다.
    let next = one_event(&desk, 3);
    assert_eq!(subscriber.try_recv().unwrap().order.order_id, next);
}

// 검증: N1
// 설정한 개수가 그대로 적용된다: 테스트 설정의 64개.
#[test]
fn 구독이_쥐는_개수는_설정한_값을_따른다() {
    let desk = Desk::standard();
    let mut idle = desk.as_broker().subscribe();
    // 주문 65개의 접수 알림. 값이 낮아 돈은 넉넉하다(1주 × 69,000 + 11).
    for second in 0..65 {
        one_event(&desk, second);
    }
    let mut held = 0;
    let end = loop {
        match idle.try_recv() {
            Ok(_) => held += 1,
            Err(error) => break error,
        }
    };
    assert_eq!(held, 64);
    assert_eq!(end, TryRecvError::Disconnected);
    assert_eq!(desk.usd(), cash("10000", "0", "0"));
    assert_eq!(desk.buying_power(Currency::Usd), d("10000"));
}
