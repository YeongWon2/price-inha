//! 승인 잇기(T14 · C7 · C12 · E7 · E8): 승인 증표는 승인 모듈이 소비에 성공한 것만 실행에 쓴다.

use pi_approval::{ApprovalNumber, Fingerprint};
use pi_paper::Side;
use pi_store::{BlockKind, TradeId, TradeState};
use price_inha::{
    ANSWER_CONTENT_CHANGED, ANSWER_FINGERPRINT_MISMATCH, ANSWER_NOT_FOUND, APPROVAL_ANSWER_PATH,
    Placed,
};
use rust_decimal::Decimal;
use serde_json::{Value, json};

use crate::common::{Bench, HYNIX, QUANTITY, buy, d, kst, monday, post, swing_with};

/// 자동 주문 금액 한도를 평가액의 2%로 낮춘 스윙 템플릿으로 운용 중인 서버. 5주 × 100,000원은 한도를 넘는다.
async fn over_limit_bench() -> Bench {
    let bench = Bench::demo(kst(monday(), 10, 0)).await;
    swing_with(bench.addr(), json!({"auto_order_limit_pct": "2"})).await;
    bench.trade_today();
    bench.quote(HYNIX, 100_000, kst(monday(), 10, 1)).await;
    bench
}

/// 판단됨 매매 하나를 내 승인 대기로 만든다.
async fn awaiting(bench: &Bench) -> (TradeId, ApprovalNumber, Fingerprint) {
    let trade = bench.judged(HYNIX);
    let placed = bench.place(buy(trade, HYNIX, bench.now())).await;
    let Placed::AwaitingApproval {
        number,
        fingerprint,
    } = placed
    else {
        panic!("금액 한도를 넘으면 승인을 기다린다: {placed:?}");
    };
    assert_eq!(bench.trade_state(trade), TradeState::AwaitingApproval);
    (trade, number, fingerprint)
}

async fn answer(bench: &Bench, number: &str, fingerprint: &str, answer: &str) -> Value {
    let body = json!({"number": number, "fingerprint": fingerprint, "answer": answer});
    let reply = post(bench.addr(), APPROVAL_ANSWER_PATH, Some(&body)).await;
    assert_eq!(
        reply.status, 200,
        "답하기는 언제나 결과를 돌려준다: {}",
        reply.body
    );
    reply.body
}

fn blocked_by_rejection(bench: &Bench, trade: TradeId) -> bool {
    let trade = bench.store().get_trade(trade).unwrap();
    trade.state == TradeState::Blocked
        && trade
            .blocked
            .is_some_and(|b| b.kind == BlockKind::ApprovalRejected)
}

// 검증: T14 C7 C10 C12
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 금액_한도를_넘는_주문은_승인을_기다리고_승인하면_그때_나간다() {
    let bench = over_limit_bench().await;
    let (trade, number, fingerprint) = awaiting(&bench).await;
    assert!(bench.orders().await.is_empty(), "승인 전에는 내지 않는다");

    let result = answer(&bench, number.as_str(), fingerprint.as_str(), "approve").await;
    assert_eq!(result["result"], json!("approved"));
    assert_eq!(bench.trade_state(trade), TradeState::Ordered);
    let orders = bench.orders().await;
    assert_eq!(orders.len(), 1, "승인한 주문 하나만 나간다");
    assert_eq!(orders[0].side, Side::Buy);
    assert_eq!(orders[0].quantity, d(QUANTITY));
    assert_eq!(orders[0].price, Some(Decimal::from(100_000)));
    bench.close().await;
}

// 검증: E7 T14
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 승인한_뒤_실행할_내용이_바뀌면_내지_않고_막힘으로_남는다() {
    let bench = over_limit_bench().await;
    let (trade, number, fingerprint) = awaiting(&bench).await;
    // 시세가 내려 다시 점검한 지정가가 99,000원이 된다(승인한 100,000원과 다르다).
    bench
        .quote(HYNIX, 99_000, bench.now() + chrono::Duration::seconds(1))
        .await;

    let result = answer(&bench, number.as_str(), fingerprint.as_str(), "approve").await;
    assert_eq!(result["result"], json!("rejected"));
    assert_eq!(result["reason"], json!(ANSWER_CONTENT_CHANGED));
    assert!(
        blocked_by_rejection(&bench, trade),
        "매매는 막힘(승인 거절)이다"
    );
    assert!(bench.orders().await.is_empty(), "바뀐 내용은 내지 않는다");
    bench.close().await;
}

// 검증: E7 E8 T14
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 없는_번호나_다른_지문이나_거절한_승인으로는_주문이_나가지_않는다() {
    let bench = over_limit_bench().await;
    let (first, number, fingerprint) = awaiting(&bench).await;

    let result = answer(
        &bench,
        "not-an-approval-number",
        fingerprint.as_str(),
        "approve",
    )
    .await;
    assert_eq!(result["result"], json!("rejected"));
    assert_eq!(result["reason"], json!(ANSWER_NOT_FOUND));
    assert_eq!(
        bench.trade_state(first),
        TradeState::AwaitingApproval,
        "아무것도 바뀌지 않는다"
    );

    // 앞 6자만 보내도 지문이 다른 것이다.
    let result = answer(&bench, number.as_str(), fingerprint.short_code(), "approve").await;
    assert_eq!(result["result"], json!("rejected"));
    assert_eq!(result["reason"], json!(ANSWER_FINGERPRINT_MISMATCH));
    assert!(blocked_by_rejection(&bench, first));

    let result = answer(&bench, number.as_str(), fingerprint.as_str(), "approve").await;
    assert_eq!(
        result["result"],
        json!("rejected"),
        "처리된 번호는 다시 승인되지 않는다"
    );

    let (second, number, fingerprint) = awaiting(&bench).await;
    let result = answer(&bench, number.as_str(), fingerprint.as_str(), "reject").await;
    assert_eq!(result["result"], json!("rejected"));
    assert!(
        blocked_by_rejection(&bench, second),
        "거절한 매매는 막힘이다"
    );

    assert!(bench.orders().await.is_empty(), "어느 것도 나가지 않았다");
    bench.close().await;
}
