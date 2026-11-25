//! 35차 5번: 규칙 개선안의 승인 요청도 다른 승인과 같게 `office.approval`(agent `review`, state `waiting`)을
//! 낸다. 개선안 이벤트(`rule_proposal`)는 그대로 낸다.

use price_inha::{KIND_OFFICE_APPROVAL, KIND_PROPOSAL, TEMPLATE_ADJUST_PATH};
use serde_json::{Value, json};

use crate::common::{Bench, events, kst, monday, post, wait_for};

/// 그 승인 번호의 사무실 승인 이벤트들(난 차례대로).
fn office_cards(bench: &Bench, number: &str) -> Vec<Value> {
    events(&bench.store(), KIND_OFFICE_APPROVAL)
        .into_iter()
        .filter(|card| card["number"] == number)
        .collect()
}

// 검증: V62 C27 R5-10 E28
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 규칙_개선안의_승인_요청에도_복기_담당의_기다림_승인_이벤트가_한_번_난다() {
    let bench = Bench::start(kst(monday(), 10, 0)).await;
    let id = bench.live_template().await;
    let changes = json!({"id": id.0, "changes": {"risk_per_trade_pct": "1.5"}});
    let reply = post(bench.addr(), TEMPLATE_ADJUST_PATH, &changes).await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    assert_eq!(
        reply.body["state"],
        json!("needs_approval"),
        "{}",
        reply.text
    );
    let number = reply.body["number"]
        .as_str()
        .expect("승인 번호 글자")
        .to_string();

    wait_for("개선안 승인 이벤트", || {
        !office_cards(&bench, &number).is_empty()
    })
    .await;
    let cards = office_cards(&bench, &number);
    let card = &cards[0];
    assert_eq!(card["agent"], json!("review"), "개선안은 복기 담당: {card}");
    assert_eq!(card["state"], json!("waiting"), "{card}");
    assert_eq!(card["status"], json!("pending"), "옛 칸 그대로: {card}");
    assert_eq!(cards.len(), 1, "같은 요청에 두 번 내지 않는다: {cards:?}");
    assert!(
        events(&bench.store(), KIND_PROPOSAL)
            .iter()
            .any(|notice| notice["number"] == json!(number)),
        "개선안 이벤트도 그대로 난다"
    );
    bench.close().await;
}
