//! 14번 사무실 승인 카드(FN2 "폰과 사무실로"): 화면은 Vite 차수에서 만들고, 여기서는 경로와 이벤트만 본다.
//! 주문이 승인을 기다리면 사무실 이벤트([`KIND_OFFICE_APPROVAL`])에 카드 정보(번호 · 가린 내용 · 지문 앞 6자리)가
//! 실리고, 폰과 같은 경로 · 같은 지문 대조로 답하면 같은 번호의 이벤트로 결과가 실린다.

use pi_store::TradeState;
use price_inha::KIND_OFFICE_APPROVAL;
use serde_json::{Value, json};

use crate::common::{Bench, events, get, kst, monday, post, wait_for};

/// 그 승인 번호의 사무실 승인 이벤트들(난 차례대로).
fn office_cards(bench: &Bench, number: &str) -> Vec<Value> {
    events(&bench.store(), KIND_OFFICE_APPROVAL)
        .into_iter()
        .filter(|card| card["number"] == number)
        .collect()
}

async fn ready_for_approval(bench: &Bench) {
    bench
        .ready_with(json!({"symbol_weight_pct": "10", "auto_order_limit_pct": "2"}))
        .await;
}

// 검증: V10 C7 E3
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 승인을_기다리면_사무실_이벤트에_카드가_실리고_같은_경로로_승인한다() {
    let bench = Bench::start(kst(monday(), 10, 0)).await;
    ready_for_approval(&bench).await;
    let (trade, number, fingerprint) = bench.awaiting().await;
    let number = number.as_str().to_string();
    wait_for("사무실 승인 카드 이벤트", || {
        !office_cards(&bench, &number).is_empty()
    })
    .await;
    let card = office_cards(&bench, &number)[0].clone();
    assert_eq!(card["status"], "pending", "{card}");
    assert_eq!(
        card["short_code"],
        fingerprint.short_code(),
        "지문 앞 6자리"
    );
    assert_eq!(fingerprint.short_code().len(), 6);
    let shown = get(bench.addr(), &format!("/api/approvals/{number}")).await;
    assert_eq!(shown.status, 200, "{}", shown.text);
    assert_eq!(
        card["summary"], shown.body["summary"],
        "폰 카드와 같은 가린 내용"
    );
    assert!(card["summary"].as_str().is_some_and(|s| !s.is_empty()));

    let path = format!("/api/approvals/{number}/approve");
    let reply = post(
        bench.addr(),
        &path,
        &json!({"fingerprint": fingerprint.as_str()}),
    )
    .await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    assert_eq!(reply.body["result"], "approved");
    assert_eq!(bench.trade_state(trade), TradeState::Ordered);
    wait_for("승인 결과가 사무실 이벤트로 간다", || {
        office_cards(&bench, &number)
            .iter()
            .any(|c| c["status"] == "approved")
    })
    .await;
    bench.close().await;
}

// 검증: V10 C7 E7
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 지문이_다르면_승인되지_않고_거절은_사무실_이벤트로_간다() {
    let bench = Bench::start(kst(monday(), 10, 0)).await;
    ready_for_approval(&bench).await;
    let (trade, number, _) = bench.awaiting().await;
    let number = number.as_str().to_string();
    wait_for("사무실 승인 카드 이벤트", || {
        !office_cards(&bench, &number).is_empty()
    })
    .await;

    let wrong = post(
        bench.addr(),
        &format!("/api/approvals/{number}/approve"),
        &json!({"fingerprint": "0".repeat(64)}),
    )
    .await;
    assert_eq!(wrong.status, 409, "{}", wrong.text);
    assert_eq!(wrong.body["reason"], "fingerprint_mismatch");
    assert_ne!(bench.trade_state(trade), TradeState::Ordered);
    assert!(
        office_cards(&bench, &number)
            .iter()
            .all(|c| c["status"] != "approved"),
        "지문이 다르면 승인 이벤트가 없다"
    );

    // 지문이 달라 막힌 카드는 닫힌다. 새 카드를 만들어 거절해 본다.
    bench.set(bench.now() + chrono::Duration::seconds(5));
    let (second, number, fingerprint) = bench.awaiting().await;
    let number = number.as_str().to_string();
    wait_for("두 번째 카드 이벤트", || {
        !office_cards(&bench, &number).is_empty()
    })
    .await;
    let reply = post(
        bench.addr(),
        &format!("/api/approvals/{number}/reject"),
        &json!({"fingerprint": fingerprint.as_str()}),
    )
    .await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    assert_eq!(reply.body["result"], "rejected");
    assert_eq!(bench.trade_state(second), TradeState::Blocked);
    wait_for("거절 결과가 사무실 이벤트로 간다", || {
        office_cards(&bench, &number)
            .iter()
            .any(|c| c["status"] == "rejected")
    })
    .await;
    bench.close().await;
}
