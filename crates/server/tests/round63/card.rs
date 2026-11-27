//! 사무실 승인 카드로 규칙 개선안을 승인한다(백로그 F 끝 확인): 카드가 그리는 사무실 승인 이벤트
//! (`office.approval` — 번호 · 지문 앞 6자 · 복기 담당 · 기다림)에서 시작해, 카드가 하는 그대로 승인 내용을 읽고
//! (`GET /api/approvals/{번호}`) 지문 앞 6자를 대조한 뒤 전체 지문으로 `POST /api/approvals/{번호}/approve`를 보낸다.
//! 개선안은 개선안 답하기와 같게 반영되고(위험을 키우지 않으면 반영됨 · 키우면 냉각중), 같은 번호의 카드는
//! 답함으로 다시 난다. 개선안 목록은 뒤에 생긴 개선안의 승인 번호도 싣는다.

use std::time::{Duration, Instant};

use pi_store::{ProposalState, Store};
use price_inha::{KIND_OFFICE_APPROVAL, PROPOSALS_PATH};
use serde_json::{Value, json};

use crate::common::{
    Bench, approval_of, card, d, get, kst, listed, monday, next_trading_day, proposal_state,
    proposed, risk_pct,
};

/// 그 승인 번호의 사무실 승인 이벤트들(난 차례대로).
fn office_cards(store: &Store, number: &str) -> Vec<Value> {
    let mut found = Vec::new();
    let mut after = 0;
    loop {
        let events = store.read_after(after, 500).unwrap();
        let Some(last) = events.last() else {
            return found;
        };
        after = last.seq;
        found.extend(
            events
                .into_iter()
                .filter(|event| event.kind == KIND_OFFICE_APPROVAL)
                .map(|event| event.payload)
                .filter(|card| card["number"] == json!(number)),
        );
    }
}

/// 그 번호의 사무실 카드 가운데 `state`인 것이 날 때까지 기다린다.
async fn card_in(store: &Store, number: &str, state: &str) -> Value {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if let Some(card) = office_cards(store, number)
            .into_iter()
            .find(|card| card["state"] == json!(state))
        {
            return card;
        }
        assert!(
            Instant::now() < deadline,
            "{state} 카드가 나야 한다: {:?}",
            office_cards(store, number)
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// 사무실 카드가 하는 그대로 승인한다: 카드의 지문 앞 6자와 읽은 승인 내용의 지문을 대조하고 전체 지문을 보낸다.
async fn approve_like_office(bench: &Bench, number: &str) -> Value {
    let waiting = card_in(&bench.store(), number, "waiting").await;
    assert_eq!(
        waiting["agent"],
        json!("review"),
        "개선안은 복기 담당: {waiting}"
    );
    let code = waiting["short_code"]
        .as_str()
        .expect("지문 앞 6자")
        .to_string();
    assert_eq!(code.chars().count(), 6, "{waiting}");

    let view = get(bench.addr(), &format!("/api/approvals/{number}")).await;
    assert_eq!(view.status, 200, "카드가 승인 내용을 읽는다: {}", view.body);
    let fingerprint = view.body["fingerprint"].as_str().expect("지문").to_string();
    assert!(
        fingerprint.starts_with(&code),
        "카드의 지문 앞 6자와 같다: {code} · {fingerprint}"
    );
    assert_eq!(view.body["status"], json!("pending"), "{}", view.body);

    let reply = card(bench, number, "approve", &fingerprint).await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    reply.body
}

// 검증: V82 V21 C27 E35
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 사무실_카드로_위험을_키우지_않는_개선안을_승인하면_반영되고_카드는_답함이_된다() {
    let bench = Bench::start(kst(monday(), 10, 0)).await;
    let (template, id) = proposed(&bench, json!({"risk_per_trade_pct": "0.5"})).await;
    let (number, listed_fingerprint) = approval_of(&bench, id).await;

    let body = approve_like_office(&bench, &number).await;
    assert_eq!(body["result"], json!("approved"), "{body}");
    assert_eq!(proposal_state(&bench, id), ProposalState::Applied, "반영됨");
    assert!(
        bench.store().get_proposal(id).unwrap().approval.is_some(),
        "승인을 소비해 개선안에 붙였다"
    );
    assert_eq!(risk_pct(&bench.store(), template), d("0.5"), "값이 바뀐다");

    let answered = card_in(&bench.store(), &number, "answered").await;
    assert_eq!(answered["status"], json!("approved"), "{answered}");
    let shown = listed(&bench, id).await;
    assert_eq!(shown["fingerprint"], json!(listed_fingerprint), "{shown}");
    bench.close().await;
}

// 검증: V82 V21 E35
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 사무실_카드로_위험을_키우는_개선안을_승인하면_냉각중이고_다음_거래일에_반영된다() {
    let bench = Bench::start(kst(monday(), 10, 0)).await;
    let (template, id) = proposed(&bench, json!({"risk_per_trade_pct": "1.5"})).await;
    let (number, _) = approval_of(&bench, id).await;
    let before = risk_pct(&bench.store(), template);

    let body = approve_like_office(&bench, &number).await;
    assert_eq!(body["result"], json!("approved"), "{body}");
    assert_eq!(proposal_state(&bench, id), ProposalState::Cooling, "냉각중");
    assert_eq!(risk_pct(&bench.store(), template), before, "아직 그대로");

    next_trading_day(&bench).await;
    assert_eq!(proposal_state(&bench, id), ProposalState::Applied);
    assert_eq!(
        risk_pct(&bench.store(), template),
        d("1.5"),
        "다음 거래일에 반영"
    );
    bench.close().await;
}

// 검증: V82 E35
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 개선안_목록은_목록을_읽은_뒤에_생긴_개선안의_승인_번호도_싣는다() {
    let bench = Bench::start(kst(monday(), 10, 0)).await;
    // 개선안이 생기기 전에 목록을 한 번 읽어 둔다(그때까지의 이벤트를 읽었다).
    let empty = get(bench.addr(), PROPOSALS_PATH).await;
    assert_eq!(empty.status, 200, "{}", empty.body);
    assert_eq!(empty.body["proposals"], json!([]), "{}", empty.body);

    let (_, id) = proposed(&bench, json!({"risk_per_trade_pct": "0.5"})).await;
    let (number, fingerprint) = approval_of(&bench, id).await;
    assert!(!number.is_empty() && !fingerprint.is_empty());
    // 다시 읽어도 같다(두 번 싣지 않는다).
    assert_eq!(approval_of(&bench, id).await, (number.clone(), fingerprint));
    let shown = get(bench.addr(), PROPOSALS_PATH).await;
    let list = shown.body["proposals"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert_eq!(list.len(), 1, "{}", shown.body);
    assert_eq!(list[0]["number"], json!(number), "{}", shown.body);
    bench.close().await;
}
