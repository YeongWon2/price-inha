//! 16차 4번: 손절 · 청산 다시 내기가 거부될 만큼 대기가 낡으면(일부 체결 등으로 남은 수량이 줄었다) 그 대기를 지우고,
//! 그 매매의 지금 남은 수량으로 같은 목적의 파는 주문을 새로 내 낸다. 남은 수량이 0이면 대기만 지운다.
//!
//! 낡는 길: 모의계좌가 주문마다 시세 한 번에 2주씩만 체결하게 하고(일부 체결), 익절 지정가를 열어 둔 채 증권사가
//! 주문을 받지 않는 동안 손절이 막힌다(취소도 받지 않아 익절이 열린 채다). 계좌가 돌아온 뒤 시세가 올라 열린 익절이
//! 일부(또는 전부) 체결되면 5주 손절 대기는 남은 수량보다 많다.

use std::time::Instant;

use pi_paper::{Fault, OrderType};
use pi_rules::Purpose;
use pi_store::{TradeId, TradeState};
use price_inha::{Placed, TRADE_ROOM_PREFIX};
use serde_json::json;

use crate::common::{Bench, HYNIX, Plan, STEP, STOP, d, kst, monday, order, plus};

/// 열어 둘 익절 지정가(이 값 이상의 시세에서 체결된다).
const PROFIT_PRICE: i64 = 101_000;

/// 손절 대기가 낡은 상태를 만든다: 5주 보유 · 열린 익절 5주 · 막힌 손절 5주 · 계좌 복구 · 시세가 익절값으로 올라
/// 한 번 체결(2주). 매매를 돌려준다. 이때 남은 수량은 3주다.
async fn stale_stop(bench: &Bench) -> TradeId {
    bench.ready().await;
    let trade = bench.holding(HYNIX).await;
    let now = bench.now();
    let placed = bench
        .place(order(trade, HYNIX, Purpose::TakeProfit, PROFIT_PRICE, now))
        .await;
    assert!(
        matches!(placed, Placed::Ordered(_)),
        "익절을 열어 둔다: {placed:?}"
    );

    bench.trading().paper().set_fault(Some(Fault::RejectOrders));
    bench.quote(HYNIX, STOP - 1_000, plus(bench.now(), 1)).await;
    let pending = bench.pending_sells(trade);
    assert_eq!(pending.len(), 1, "손절이 막혀 기다린다: {pending:?}");
    assert_eq!(pending[0]["purpose"], json!("stop_loss"));
    assert_eq!(pending[0]["order"]["quantity"], json!("5"));

    // 시세를 넣는 동안 뒤에서 도는 다시 내기가 끼어 새로 내지 않게(시험 순서를 결정적으로) 증권사가 새 주문을
    // 받지 않는 채로 익절을 체결시키고, 그 뒤에 푼다. 시계는 시세를 넣을 때만 가므로 푼 뒤에는 다음 걸음까지 안 돈다.
    bench.quote(HYNIX, PROFIT_PRICE, plus(bench.now(), 1)).await;
    bench.trading().paper().set_fault(None);
    assert_eq!(bench.trade_state(trade), TradeState::Holding);
    let open_sell = bench.sells(HYNIX).await;
    assert_eq!(open_sell.len(), 1, "열려 있던 익절 하나: {open_sell:?}");
    assert_eq!(
        open_sell[0].execution.filled_quantity,
        d("2"),
        "익절이 일부(2주) 체결돼 남은 수량이 줄었다"
    );
    trade
}

/// 청산 대기가 낡은 상태를 만든다(손절 대신 사용자 승인 청산 길: 지정가 [`PROFIT_PRICE`]로 5주). 다른 것은
/// [`stale_stop`]과 같다. 남은 수량은 3주다.
async fn stale_liquidation(bench: &Bench) -> TradeId {
    bench.ready().await;
    let trade = bench.holding(HYNIX).await;
    let now = bench.now();
    let placed = bench
        .place(order(trade, HYNIX, Purpose::TakeProfit, PROFIT_PRICE, now))
        .await;
    assert!(
        matches!(placed, Placed::Ordered(_)),
        "익절을 열어 둔다: {placed:?}"
    );

    bench.trading().paper().set_fault(Some(Fault::RejectOrders));
    let at = bench.now();
    // 증권사가 받지 않아 실패한다(오류로 돌아와도 대기는 남는다).
    let _failed = bench
        .trading()
        .place(order(trade, HYNIX, Purpose::Liquidation, PROFIT_PRICE, at))
        .await;
    let pending = bench.pending_sells(trade);
    assert_eq!(pending.len(), 1, "청산이 막혀 기다린다: {pending:?}");
    assert_eq!(pending[0]["order"]["quantity"], json!("5"));
    assert_eq!(pending[0]["order"]["order_type"], json!("limit"));

    // 시세를 넣는 동안 뒤에서 도는 다시 내기가 끼어 새로 내지 않게(시험 순서를 결정적으로) 증권사가 새 주문을
    // 받지 않는 채로 익절을 체결시키고, 그 뒤에 푼다. 시계는 시세를 넣을 때만 가므로 푼 뒤에는 다음 걸음까지 안 돈다.
    bench.quote(HYNIX, PROFIT_PRICE, plus(bench.now(), 1)).await;
    bench.trading().paper().set_fault(None);
    assert_eq!(bench.trade_state(trade), TradeState::Holding);
    trade
}

/// 그 매매의 방에 남은 줄 가운데 `needle`이 든 것의 수.
fn room_lines(bench: &Bench, trade: TradeId, needle: &str) -> usize {
    let room = format!("{TRADE_ROOM_PREFIX}{}", trade.0);
    bench
        .store()
        .room_events(&room, 0, 1000)
        .unwrap()
        .iter()
        .filter(|event| event.payload.to_string().contains(needle))
        .count()
}

/// 대기가 사라질 때까지 20초씩 시계를 옮긴다(가장 길게 [`STEP`]).
async fn until_cleared(bench: &Bench, trade: TradeId, what: &str) {
    let deadline = Instant::now() + STEP;
    while !bench.pending_sells(trade).is_empty() {
        assert!(
            Instant::now() < deadline,
            "{what}: {:?}",
            bench.pending_sells(trade)
        );
        bench.advance(20).await;
    }
}

// 검증: E30 E34 F4
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 일부_체결로_낡은_손절_대기는_지워지고_남은_수량으로_새로_나가며_방_기록_한_줄이_남는다() {
    let bench = Plan::demo().max_fill(2).start(kst(monday(), 10, 0)).await;
    let trade = stale_stop(&bench).await;

    until_cleared(&bench, trade, "낡은 손절 대기는 지우고 새로 내야 한다").await;
    let sells = bench.sells(HYNIX).await;
    let fresh: Vec<_> = sells
        .iter()
        .filter(|sell| sell.order_type == OrderType::Market)
        .collect();
    assert_eq!(
        fresh.len(),
        1,
        "남은 수량으로 새 손절(시장가) 하나가 나간다: {sells:?}"
    );
    assert_eq!(
        fresh[0].quantity,
        d("3"),
        "새 손절은 지금 남은 수량(3주)이다"
    );

    // 이어지는 시세로 다 체결되면 팔림이다. 같은 대기가 되살아나지 않는다.
    for step in 1..=3 {
        bench
            .quote(HYNIX, PROFIT_PRICE, plus(bench.now(), step))
            .await;
    }
    assert_eq!(
        bench.trade_state(trade),
        TradeState::Sold,
        "남은 3주가 팔렸다"
    );
    assert!(bench.pending_sells(trade).is_empty());
    assert_eq!(
        room_lines(&bench, trade, "남은 수량"),
        1,
        "남은 수량으로 새로 냈다는 줄이 그 매매의 방에 한 줄 남는다"
    );
    bench.close().await;
}

// 검증: E30 E34
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 남은_수량이_0이_된_낡은_손절_대기는_새로_내지_않고_대기만_지워진다() {
    let bench = Plan::demo().max_fill(2).start(kst(monday(), 10, 0)).await;
    let trade = stale_stop(&bench).await;

    // 열린 익절이 나머지도 다 체결돼 남은 수량이 0이 됐다(팔림). 그동안 뒤에서 도는 다시 내기가 먼저
    // 새로 내지 않게(시험 순서를 결정적으로) 증권사가 새 주문을 받지 않는 채로 채우고, 단언 뒤에 푼다.
    bench.trading().paper().set_fault(Some(Fault::RejectOrders));
    for step in 1..=2 {
        bench
            .quote(HYNIX, PROFIT_PRICE, plus(bench.now(), step))
            .await;
    }
    assert_eq!(bench.trade_state(trade), TradeState::Sold);
    bench.trading().paper().set_fault(None);

    until_cleared(&bench, trade, "남은 수량이 0이면 낡은 대기는 지워져야 한다").await;
    let sells = bench.sells(HYNIX).await;
    assert_eq!(
        sells.len(),
        1,
        "새 주문은 나가지 않는다(익절 하나뿐): {sells:?}"
    );
    assert_eq!(sells[0].order_type, OrderType::Limit);
    bench.close().await;
}

// 검증: E30 E34 F4
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 새로_내는_주문이_나가지_못하면_낡은_대기는_남고_방에_새로_냈다는_줄이_없다() {
    let bench = Plan::demo().max_fill(2).start(kst(monday(), 10, 0)).await;
    let trade = stale_stop(&bench).await;

    // 새로 낼 때 증권사가 받지 않는다(규칙에 막힌 것과 같이 "나가지 못함"). 지우고 낸 줄을 먼저 남기면 안 된다.
    bench.trading().paper().set_fault(Some(Fault::RejectOrders));
    bench.advance(120).await;
    assert!(
        !bench.pending_sells(trade).is_empty(),
        "나가지 못했으니 대기는 그대로 남는다"
    );
    assert_eq!(
        room_lines(&bench, trade, "남은 수량"),
        0,
        "나가기 전에는 새로 냈다는 줄을 남기지 않는다"
    );
    assert!(
        bench
            .sells(HYNIX)
            .await
            .iter()
            .all(|sell| sell.order_type == OrderType::Limit),
        "새 주문(시장가)은 나가지 못했다"
    );
    bench.close().await;
}

// 검증: E30 E34
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 지정가_청산_대기가_낡으면_지정가_같은_값으로_남은_수량만큼_새로_나간다() {
    let bench = Plan::demo().max_fill(2).start(kst(monday(), 10, 0)).await;
    let trade = stale_liquidation(&bench).await;

    until_cleared(&bench, trade, "낡은 청산 대기는 새로 나가고 지워져야 한다").await;
    let sells = bench.sells(HYNIX).await;
    assert!(
        sells.iter().all(|sell| sell.order_type == OrderType::Limit),
        "청산은 처음 주문 모양(지정가)을 지킨다 — 시장가가 나가면 안 된다: {sells:?}"
    );
    let fresh: Vec<_> = sells
        .iter()
        .filter(|sell| sell.quantity == d("3"))
        .collect();
    assert_eq!(fresh.len(), 1, "남은 3주의 새 청산 하나: {sells:?}");
    assert_eq!(
        fresh[0].price,
        Some(d(&PROFIT_PRICE.to_string())),
        "같은 가격(호가 단위에 맞춘)이다"
    );
    assert_eq!(
        room_lines(&bench, trade, "남은 수량"),
        1,
        "나간 뒤 방에 한 줄"
    );
    bench.close().await;
}

/// 새로 낸 주문이 나가지 못했다는 방 한 줄(19차 2번). "남은 수량" · "새로 냈"을 쓰지 않는다.
const RENEW_FAILED_LINE: &str = "새로 낸 주문이 나가지 못해 대기를 3주로 바꿔 다시 냅니다";

// 검증: E30 E34 F4
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 낡은_손절을_새로_냈는데_나가지_못하면_방에_대기를_남은_수량으로_바꿔_다시_낸다는_한_줄이_남는다()
 {
    let bench = Plan::demo().max_fill(2).start(kst(monday(), 10, 0)).await;
    let trade = stale_stop(&bench).await;

    // 새로 낼 때 증권사가 받지 않는다. 시도마다 한 줄이 남을 때까지 시계를 옮긴다(가장 길게 STEP).
    bench.trading().paper().set_fault(Some(Fault::RejectOrders));
    let deadline = Instant::now() + STEP;
    while room_lines(&bench, trade, RENEW_FAILED_LINE) == 0 {
        assert!(
            Instant::now() < deadline,
            "새로 낸 주문이 나가지 못하면 방에 한 줄이 남아야 한다"
        );
        bench.advance(20).await;
    }
    let pending = bench.pending_sells(trade);
    assert_eq!(pending.len(), 1, "대기는 남는다: {pending:?}");
    assert_eq!(
        pending[0]["order"]["quantity"],
        json!("3"),
        "대기 수량은 지금 남은 수량(3주)으로 바뀐다"
    );
    assert_eq!(
        room_lines(&bench, trade, "새로 냈"),
        0,
        "나가기 전에는 새로 냈다는 줄이 없다"
    );
    assert_eq!(
        room_lines(&bench, trade, "남은 수량"),
        0,
        "나가기 전에는 남은 수량 줄도 없다"
    );
    bench.close().await;
}
