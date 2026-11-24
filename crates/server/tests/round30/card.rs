//! 28차 3번(V82 · V21 · E28 · E35): 폰 · 사무실의 승인 카드 길(`/api/approvals/{번호}/approve` · `/reject`,
//! `/api/approvals/answer`)로 규칙 개선안의 승인 번호에 답해도 `/api/proposals/answer`와 같은 결과가 난다.
//! 승인은 한 번만 소비되고, 이미 답한 번호는 지금 규칙대로 거절된다.

use pi_store::ProposalState;
use price_inha::APPROVAL_ANSWER_PATH;
use serde_json::{Value, json};

use crate::common::*;

// 검증: V82 V21 E35
#[tokio::test]
async fn 승인_카드로_위험을_키우지_않는_개선안을_승인하면_반영되고_규칙_화면에_바뀐_값이_보인다() {
    let bench = Bench::start(kst(monday(), 10, 0)).await;
    let (template, id) = proposed(&bench, json!({"risk_per_trade_pct": "0.5"})).await;
    let (number, fingerprint) = approval_of(&bench, id).await;

    let reply = card(&bench, &number, "approve", &fingerprint).await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert_eq!(reply.body["result"], json!("approved"), "{}", reply.body);
    assert_eq!(reply.body["reason"], Value::Null);
    assert_eq!(proposal_state(&bench, id), ProposalState::Applied, "반영됨");
    let stored = bench.store().get_proposal(id).unwrap();
    assert!(stored.approval.is_some(), "승인을 소비해 개선안에 붙였다");
    assert_eq!(
        risk_pct(&bench.store(), template),
        d("0.5"),
        "다음 매매부터 새 값"
    );

    let view = rules(&bench).await;
    assert_eq!(num(&field(&view, "risk_per_trade_pct")["value"]), d("0.5"));
    assert_eq!(view["recent"][0]["proposal"], json!(id.0), "{view}");
    bench.close().await;
}

// 검증: V82 E28 V21
#[tokio::test]
async fn 승인_카드로_위험을_키우는_개선안을_승인하면_냉각중이고_다음_거래일에_반영된다() {
    let bench = Bench::start(kst(monday(), 10, 0)).await;
    let (template, id) = proposed(&bench, json!({"risk_per_trade_pct": "1.5"})).await;
    let (number, fingerprint) = approval_of(&bench, id).await;

    let reply = card(&bench, &number, "approve", &fingerprint).await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert_eq!(reply.body["result"], json!("approved"), "{}", reply.body);
    assert_eq!(proposal_state(&bench, id), ProposalState::Cooling, "냉각중");
    assert!(bench.store().get_proposal(id).unwrap().approval.is_some());
    assert_eq!(risk_pct(&bench.store(), template), d("1"), "오늘은 그대로");
    let view = rules(&bench).await;
    assert_eq!(view["cooling"][0]["proposal"], json!(id.0), "{view}");

    next_trading_day(&bench).await;
    assert_eq!(proposal_state(&bench, id), ProposalState::Applied);
    assert_eq!(
        risk_pct(&bench.store(), template),
        d("1.5"),
        "다음 거래일부터"
    );
    bench.close().await;
}

// 검증: V82 V21
#[tokio::test]
async fn 승인_카드로_개선안을_거절하면_거절됨이고_규칙은_그대로다() {
    let bench = Bench::start(kst(monday(), 10, 0)).await;
    let (template, id) = proposed(&bench, json!({"risk_per_trade_pct": "0.5"})).await;
    let (number, fingerprint) = approval_of(&bench, id).await;

    let reply = card(&bench, &number, "reject", &fingerprint).await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert_eq!(reply.body["result"], json!("rejected"));
    assert_eq!(reply.body["reason"], json!("user_rejected"));
    assert_eq!(
        proposal_state(&bench, id),
        ProposalState::Rejected,
        "거절됨"
    );
    assert_eq!(risk_pct(&bench.store(), template), d("1"), "규칙은 그대로");
    bench.close().await;
}

// 검증: V82 V21 E35
#[tokio::test]
async fn 사무실_승인_답_경로로_개선안을_승인해도_반영된다() {
    let bench = Bench::start(kst(monday(), 10, 0)).await;
    let (template, id) = proposed(&bench, json!({"risk_per_trade_pct": "0.5"})).await;
    let (number, fingerprint) = approval_of(&bench, id).await;

    let body = json!({"number": number, "fingerprint": fingerprint, "answer": "approve"});
    let reply = post(bench.addr(), APPROVAL_ANSWER_PATH, &body).await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert_eq!(reply.body["result"], json!("approved"), "{}", reply.body);
    assert_eq!(proposal_state(&bench, id), ProposalState::Applied);
    assert_eq!(risk_pct(&bench.store(), template), d("0.5"));
    bench.close().await;
}

// 검증: V82 V21 E35
#[tokio::test]
async fn 승인_카드로_승인한_번호로_개선안_답하기를_다시_하면_이미_답함이고_한_번만_반영된다() {
    let bench = Bench::start(kst(monday(), 10, 0)).await;
    let (template, id) = proposed(&bench, json!({"risk_per_trade_pct": "0.5"})).await;
    let (number, fingerprint) = approval_of(&bench, id).await;
    let reply = card(&bench, &number, "approve", &fingerprint).await;
    assert_eq!(reply.body["result"], json!("approved"), "{}", reply.body);

    let again = answer_proposal(&bench, id, &number, &fingerprint, "approve").await;
    assert_eq!(again["result"], json!("rejected"), "{again}");
    assert_eq!(again["reason"], json!("already_resolved"), "{again}");
    assert_eq!(
        proposal_state(&bench, id),
        ProposalState::Applied,
        "반영됨 그대로"
    );
    assert_eq!(risk_pct(&bench.store(), template), d("0.5"));
    bench.close().await;
}

// 검증: V82 V21 E35
#[tokio::test]
async fn 개선안_답하기로_이미_답한_번호를_승인_카드로_다시_답하면_지금_규칙대로_거절된다() {
    let bench = Bench::start(kst(monday(), 10, 0)).await;
    let (template, id) = proposed(&bench, json!({"risk_per_trade_pct": "0.5"})).await;
    let (number, fingerprint) = approval_of(&bench, id).await;
    let body = answer_proposal(&bench, id, &number, &fingerprint, "approve").await;
    assert_eq!(body["result"], json!("applied"), "{body}");

    let approve = card(&bench, &number, "approve", &fingerprint).await;
    assert_eq!(approve.status, 409, "{}", approve.body);
    assert_eq!(approve.body["result"], json!("rejected"));
    assert_eq!(approve.body["reason"], json!("already_resolved"));
    let reject = card(&bench, &number, "reject", &fingerprint).await;
    assert_eq!(reject.body["result"], json!("rejected"), "{}", reject.body);
    assert_eq!(reject.body["reason"], json!("already_resolved"));
    assert_eq!(
        proposal_state(&bench, id),
        ProposalState::Applied,
        "반영됨 그대로"
    );
    assert_eq!(risk_pct(&bench.store(), template), d("0.5"), "한 번만 반영");
    bench.close().await;
}

// 검증: V82 E35
#[tokio::test]
async fn 승인_카드의_지문이_다르면_개선안은_제안됨이고_규칙은_그대로다() {
    let bench = Bench::start(kst(monday(), 10, 0)).await;
    let (template, id) = proposed(&bench, json!({"risk_per_trade_pct": "0.5"})).await;
    let (number, _) = approval_of(&bench, id).await;

    let reply = card(&bench, &number, "approve", &"0".repeat(64)).await;
    assert_eq!(reply.status, 409, "{}", reply.body);
    assert_eq!(reply.body["result"], json!("rejected"));
    assert_eq!(
        proposal_state(&bench, id),
        ProposalState::Proposed,
        "제안됨 그대로"
    );
    assert!(bench.store().get_proposal(id).unwrap().approval.is_none());
    assert_eq!(risk_pct(&bench.store(), template), d("1"), "규칙은 그대로");
    bench.close().await;
}
