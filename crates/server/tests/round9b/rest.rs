//! 승인 길도 쉬는 날(E19) 검사를 한다: 쉬는 날로 정한 뒤에 승인된 사는 주문 · 익절은 내지 않는다.

use pi_paper::Side;
use pi_rules::Purpose;
use pi_store::{TeamState, TradeId};
use price_inha::{APPROVAL_ANSWER_PATH, DayPlan, Placed};
use serde_json::{Value, json};

use crate::common::{Bench, HYNIX, NAVER, buy, kst, monday, post, sell, swing_with};

/// 승인을 기다리게 된 주문 안의 (번호, 지문).
fn awaiting(placed: Placed) -> (String, String) {
    let Placed::AwaitingApproval {
        number,
        fingerprint,
    } = placed
    else {
        panic!("금액 한도를 넘으면 승인을 기다린다: {placed:?}");
    };
    (
        number.as_str().to_string(),
        fingerprint.as_str().to_string(),
    )
}

/// 승인한다. 답하기는 언제나 결과를 돌려준다.
async fn approve(bench: &Bench, (number, fingerprint): &(String, String)) -> Value {
    let body = json!({"number": number, "fingerprint": fingerprint, "answer": "approve"});
    let reply = post(bench.addr(), APPROVAL_ANSWER_PATH, &body).await;
    assert_eq!(
        reply.status, 200,
        "답하기는 결과를 돌려준다: {}",
        reply.body
    );
    reply.body
}

/// 그 종목 · 방향의 주문 수.
async fn count(bench: &Bench, symbol: &str, side: Side) -> usize {
    bench
        .orders()
        .await
        .iter()
        .filter(|o| o.symbol.as_str() == symbol && o.side == side)
        .count()
}

/// 승인을 거쳐 사서 보유중으로 만든다.
async fn hold_approved(bench: &Bench, symbol: &str) -> TradeId {
    let trade = bench.judged(symbol);
    let pending = awaiting(bench.place(buy(trade, symbol, bench.now())).await);
    let answered = approve(bench, &pending).await;
    assert_eq!(answered["result"], json!("approved"), "{answered}");
    bench.fill(symbol, trade, 100_000).await;
    trade
}

// 검증: E19 T14 C12
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 쉬는_날로_정한_뒤에_승인된_사는_주문은_내지_않는다() {
    let bench = Bench::demo(kst(monday(), 10, 0)).await;
    // 자동 주문 금액 한도를 평가액의 2%로 낮춘다: 5주 × 100,000원은 한도를 넘어 승인을 기다린다.
    swing_with(bench.addr(), json!({"auto_order_limit_pct": "2"})).await;
    bench.trade_today();
    bench.quote(HYNIX, 100_000, kst(monday(), 10, 1)).await;
    let hynix = hold_approved(&bench, HYNIX).await;

    // 승인을 기다리는 사는 주문(NAVER)을 둔다.
    let at = kst(monday(), 10, 5);
    bench.quote(NAVER, 100_000, at).await;
    let naver = bench.judged(NAVER);
    let buying = awaiting(bench.place(buy(naver, NAVER, at)).await);

    let reason = "변동성이 커서 오늘은 쉰다";
    let rest = DayPlan::Rest {
        reason: reason.to_string(),
    };
    assert_eq!(
        bench.trading().decide_day(rest).unwrap(),
        TeamState::Resting
    );

    let answered = approve(&bench, &buying).await;
    assert_ne!(
        answered["result"],
        json!("approved"),
        "쉬는 날에는 승인된 사는 주문도 내지 않는다: {answered}"
    );
    assert_eq!(
        count(&bench, NAVER, Side::Buy).await,
        0,
        "NAVER는 사지 않았다"
    );

    // 익절(바로 나가는 파는 주문)도 쉬는 날에는 내지 않는다.
    bench.quote(HYNIX, 104_000, at).await;
    let placed = bench
        .place(sell(hynix, HYNIX, Purpose::TakeProfit, 104_000, at))
        .await;
    assert_eq!(placed, Placed::RestDay(reason.to_string()));
    assert_eq!(
        count(&bench, HYNIX, Side::Sell).await,
        0,
        "익절은 나가지 않았다"
    );
    bench.close().await;
}
