//! 5번 답 없음(approval 4차 · rules 4차, FN2 3b · E22): 아무도 부르지 않아도 뒤에서 기다릴 시간이 지난 승인
//! 요청을 닫고(`expire_due` → `unhandled_due`), 사는 주문은 금액 한도 안으로 줄여 다시 점검해 내고, 처리 기록을
//! 한 번만 남긴다(`record_due_handling`). 신호 유효 시간이 지난 것은 `Dropped`만 남기고 아무것도 내지 않는다.
//!
//! 사는 주문의 기다릴 시간은 5분(N2), 신호 유효 시간의 끝은 그날 장 마감이다. 파는 주문은 금액 한도로 승인을
//! 기다리지 않으므로(규칙 엔진은 사는 주문만 승인으로 보낸다) 팔기 갈래는 여기서 만들 수 없다.

use chrono::{DateTime, Utc};
use pi_approval::{ApprovalNumber, DueHandling};
use pi_paper::{Order, OrderGroup, Side};
use pi_rules::Purpose;
use pi_store::{ApprovalStatus, TradeId, TradeState};
use price_inha::{APPROVAL_ANSWER_PATH, Placed};
use serde_json::json;

use crate::common::{
    Bench, ENTRY, HYNIX, d, kst, monday, order, plus, post, settle, subscribe, wait_for,
};

/// 승인을 기다리는 사는 주문 하나.
struct Waiting {
    trade: TradeId,
    number: ApprovalNumber,
    fingerprint: String,
    /// 승인을 요청한 시각.
    asked: DateTime<Utc>,
}

/// 자동 주문 금액 한도를 평가액(1,000만 원)의 2.5%(25만 원)로 낮추고, 5주 × 10만 원(50만 원)을 내 승인 대기로
/// 만든다. 한도 안에 드는 수량은 2주다.
async fn awaiting(bench: &Bench) -> Waiting {
    bench
        .ready_with(json!({"auto_order_limit_pct": "2.5"}))
        .await;
    let at = plus(bench.now(), 1);
    bench.quote(HYNIX, ENTRY, at).await;
    let trade = bench.judged(HYNIX);
    let placed = bench
        .place(order(trade, HYNIX, Purpose::NewEntry, ENTRY, at))
        .await;
    let Placed::AwaitingApproval {
        number,
        fingerprint,
    } = placed
    else {
        panic!("금액 한도를 넘으면 승인을 기다린다: {placed:?}");
    };
    Waiting {
        trade,
        number,
        fingerprint: fingerprint.as_str().to_string(),
        asked: at,
    }
}

/// 폰을 구독해 두고 승인 대기를 만든 뒤, 알림이 간 것까지 기다린다.
async fn awaiting_notified(bench: &Bench) -> Waiting {
    subscribe(bench.addr()).await;
    let waiting = awaiting(bench).await;
    let deadline = std::time::Instant::now() + crate::common::STEP;
    while bench.pushes().await == 0 {
        assert!(
            std::time::Instant::now() < deadline,
            "승인 요청 알림이 가야 한다"
        );
        settle().await;
    }
    settle().await;
    waiting
}

/// 기다릴 시간(요청한 때부터 5분)을 넘긴다. 다시 점검에 쓸 시세가 늦지 않게 4분 50초에 한 번, 5분 1초에 한 번 넣는다.
async fn pass_wait(bench: &Bench, asked: DateTime<Utc>) {
    bench.quote(HYNIX, ENTRY, plus(asked, 290)).await;
    bench.quote(HYNIX, ENTRY, plus(asked, 301)).await;
}

/// 모의계좌에 낸 [`HYNIX`] 사는 주문들(걸린 것과 끝난 것).
async fn buys(bench: &Bench) -> Vec<Order> {
    let mut orders = bench.orders(OrderGroup::Open).await;
    orders.extend(bench.orders(OrderGroup::Closed).await);
    orders.retain(|order| order.side == Side::Buy && order.symbol.as_str() == HYNIX);
    orders
}

fn handling(bench: &Bench, number: &ApprovalNumber) -> Option<DueHandling> {
    bench.approvals().due_handling(number).unwrap()
}

async fn wait_handled(bench: &Bench, number: &ApprovalNumber) -> DueHandling {
    wait_for("답 없음 처리 기록", || {
        handling(bench, number).is_some()
    })
    .await;
    handling(bench, number).unwrap()
}

// 검증: E22 N2 V25 V45 P20
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 알림이_간_사기에_답이_없으면_한도_안으로_줄여_내고_한_번만_기록한다() {
    let bench = Bench::with_push(kst(monday(), 10, 0)).await;
    let Waiting {
        trade,
        number,
        fingerprint,
        asked,
    } = awaiting_notified(&bench).await;

    // 기다릴 시간 안에는 아무것도 하지 않는다(V25).
    bench.quote(HYNIX, ENTRY, plus(asked, 20)).await;
    settle().await;
    assert!(buys(&bench).await.is_empty(), "승인 전에는 내지 않는다");
    assert_eq!(handling(&bench, &number), None);
    assert_eq!(bench.trade_state(trade), TradeState::AwaitingApproval);

    pass_wait(&bench, asked).await;
    let done = wait_handled(&bench, &number).await;
    assert_eq!(
        done,
        DueHandling::Reduced { quantity: 2 },
        "한도(25만 원) 안의 2주로 줄여 냈다고 남긴다"
    );
    let view = bench.approvals().get(&number).unwrap().expect("승인 요청");
    assert_eq!(view.status, ApprovalStatus::NoAnswer, "답 없음으로 닫혔다");
    assert!(!view.consumed, "답 없는 요청은 승인으로 쓰이지 않는다");
    let sent = buys(&bench).await;
    assert_eq!(sent.len(), 1, "줄인 주문 하나가 나간다: {sent:?}");
    assert_eq!(sent[0].quantity, d("2"));
    assert_eq!(bench.trade_state(trade), TradeState::Ordered);

    // 뒤에서 몇 바퀴 더 돌아도 다시 내거나 다시 남기지 않는다.
    bench.set(plus(bench.now(), 5));
    settle().await;
    assert_eq!(buys(&bench).await.len(), 1, "한 번만 낸다");
    assert_eq!(handling(&bench, &number), Some(done));

    // 늦게 온 승인은 거절로 남고 더 사지 않는다(V45).
    let late = json!({"number": number.as_str(), "fingerprint": fingerprint, "answer": "approve"});
    let reply = post(bench.addr(), APPROVAL_ANSWER_PATH, &late).await;
    assert_eq!(reply.body["result"], json!("rejected"), "{}", reply.body);
    assert_eq!(buys(&bench).await.len(), 1);
    bench.close().await;
}

// 검증: E22 N2 P20
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 알림이_가지_않은_사기에_답이_없으면_사지_않았다고_남긴다() {
    let bench = Bench::start(kst(monday(), 10, 0)).await;
    let Waiting {
        trade,
        number,
        asked,
        ..
    } = awaiting(&bench).await;
    settle().await;

    pass_wait(&bench, asked).await;
    let done = wait_handled(&bench, &number).await;
    assert_eq!(
        done,
        DueHandling::NotBought,
        "승인 경로가 막혔으면 줄여서도 사지 않는다(P20)"
    );
    settle().await;
    assert!(buys(&bench).await.is_empty(), "사는 주문이 0건이다");
    assert_ne!(bench.trade_state(trade), TradeState::Ordered);
    bench.close().await;
}

// 검증: E22 N2 V25
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 신호_유효_시간이_지난_답_없음은_버렸다고만_남기고_늦게_사지_않는다() {
    let bench = Bench::with_push(kst(monday(), 10, 0)).await;
    let Waiting { trade, number, .. } = awaiting_notified(&bench).await;

    // 기다릴 시간도 신호 유효 시간(장 마감)도 지난 뒤에야 본다(꺼졌다 켜진 것과 같다).
    bench.set(kst(monday(), 15, 31));
    let done = wait_handled(&bench, &number).await;
    assert_eq!(done, DueHandling::Dropped, "늦은 것은 버리기만 한다");
    settle().await;
    assert!(buys(&bench).await.is_empty(), "줄여서 늦게 사지 않는다");
    assert_ne!(bench.trade_state(trade), TradeState::Ordered);
    bench.close().await;
}

// 검증: E22 N2 P20
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 알림이_가지_않았고_신호도_지났으면_버렸다고만_남긴다() {
    let bench = Bench::start(kst(monday(), 10, 0)).await;
    let Waiting { trade, number, .. } = awaiting(&bench).await;
    settle().await;

    bench.set(kst(monday(), 15, 31));
    let done = wait_handled(&bench, &number).await;
    assert_eq!(
        done,
        DueHandling::Dropped,
        "신호가 지난 '사지 않음'도 버리기로 남긴다"
    );
    settle().await;
    assert!(buys(&bench).await.is_empty());
    assert_ne!(bench.trade_state(trade), TradeState::Ordered);
    bench.close().await;
}
