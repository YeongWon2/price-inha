//! 34차 1번(C27): 사무실 승인 카드 이벤트에 ✋를 띄울 담당(`agent`)과 상태(`state`)가 더해진다.

use price_inha::KIND_OFFICE_APPROVAL;
use serde_json::{Value, json};

use crate::common::{Bench, buy_order, events, kst, monday, post, wait_for};

/// 그 승인 번호의 사무실 승인 이벤트들(난 차례대로).
fn office_cards(bench: &Bench, number: &str) -> Vec<Value> {
    events(&bench.store(), KIND_OFFICE_APPROVAL)
        .into_iter()
        .filter(|card| card["number"] == number)
        .collect()
}

// 검증: V62 C27 R5-10
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 승인을_기다리면_이벤트에_담당과_기다림이_실리고_답하면_답함이_실린다() {
    let bench = Bench::start(kst(monday(), 10, 0)).await;
    bench.ready_for_approval().await;
    let (number, fingerprint) = bench.awaiting().await;
    let number = number.as_str().to_string();
    wait_for("기다림 이벤트", || {
        !office_cards(&bench, &number).is_empty()
    })
    .await;
    let waiting = office_cards(&bench, &number)[0].clone();
    assert_eq!(
        waiting["status"],
        json!("pending"),
        "옛 칸 그대로: {waiting}"
    );
    assert_eq!(
        waiting["agent"],
        json!("trader"),
        "주문은 매매 담당: {waiting}"
    );
    assert_eq!(waiting["state"], json!("waiting"), "{waiting}");
    assert!(
        !waiting.to_string().contains(fingerprint.as_str()),
        "전체 지문은 싣지 않는다"
    );

    let path = format!("/api/approvals/{number}/approve");
    let reply = post(
        bench.addr(),
        &path,
        &json!({"fingerprint": fingerprint.as_str()}),
    )
    .await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    wait_for("답 이벤트", || {
        office_cards(&bench, &number)
            .iter()
            .any(|card| card["status"] == "approved")
    })
    .await;
    let answered = office_cards(&bench, &number)
        .into_iter()
        .find(|card| card["status"] == "approved")
        .unwrap();
    assert_eq!(answered["agent"], json!("trader"), "{answered}");
    assert_eq!(answered["state"], json!("answered"), "{answered}");
    bench.close().await;
}

// 검증: V62 C27 E49
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 만료로_닫히면_같은_번호로_담당과_만료가_실린_이벤트가_난다() {
    let bench = Bench::start(kst(monday(), 10, 0)).await;
    let number = bench.ask(
        buy_order(&bench, "signal-round36-c", 30),
        "곧 끝나는 신호의 주문",
    );
    let fingerprint = bench.view(&number).fingerprint.as_str().to_string();

    bench.advance(60).await;
    bench.wait_closed(&number).await;
    wait_for("만료 이벤트", || {
        office_cards(&bench, &number)
            .iter()
            .any(|card| card["status"] != "pending")
    })
    .await;
    let closed = office_cards(&bench, &number)
        .into_iter()
        .find(|card| card["status"] != "pending")
        .unwrap();
    assert_eq!(closed["agent"], json!("trader"), "{closed}");
    assert_eq!(closed["state"], json!("expired"), "{closed}");
    assert!(
        !closed.to_string().contains(&fingerprint),
        "전체 지문은 싣지 않는다"
    );
    bench.close().await;
}
