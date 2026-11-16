//! 복기 · 실수 기록 · 규칙 개선안 잇기(T15, V19~V21 · E28 · E35).

use pi_journal::{Journal, Mistake};
use pi_store::{ProposalId, ProposalState, Store, TemplateId, TradeState};
use price_inha::{DemoTurn, KIND_PROPOSAL, PROPOSAL_ANSWER_PATH, PROPOSALS_PATH};
use rust_decimal::Decimal;
use serde_json::{Value, json};

use crate::common::*;

/// 복기 담당의 답: 손절이 늦음 + 바꿀 규칙.
fn late_stop(change: Value) -> String {
    json!({"mistakes": ["late_stop"], "note": "손절이 늦었다", "change": change}).to_string()
}

/// 지금 쓰는 템플릿의 한 매매 위험(%).
fn risk_pct(store: &Store, template: i64) -> Decimal {
    let saved = store.get_template(TemplateId(template)).unwrap();
    let template = pi_template::Template::from_json(&saved.value.to_string()).unwrap();
    template.params.risk_per_trade_pct
}

/// 팔린 매매 둘(같은 실수)을 복기해 개선안 하나를 낸다. (템플릿 번호, 개선안 번호)를 돌려준다.
async fn proposed(bench: &Bench, change: Value) -> (i64, ProposalId) {
    let template = bench.ready().await;
    bench.sold(HYNIX).await;
    bench.sold(NAVER).await;
    bench.script(DemoTurn::Review, &late_stop(change));
    let reviewed = bench.crew().review_day().await.expect("복기한다");
    assert_eq!(reviewed.reviews.len(), 2);
    assert_eq!(reviewed.proposals.len(), 1, "같은 실수 2건이면 개선안 하나");
    (template, reviewed.proposals[0])
}

/// 개선안 목록에서 그 개선안의 승인 번호와 지문.
async fn approval_of(bench: &Bench, id: ProposalId) -> (Value, Value) {
    let reply = get(bench.addr(), PROPOSALS_PATH).await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    let list = reply.body["proposals"].as_array().unwrap().clone();
    let found = list
        .iter()
        .find(|p| p["id"] == json!(id.0))
        .expect("목록에 있다")
        .clone();
    (found["number"].clone(), found["fingerprint"].clone())
}

async fn answer(
    bench: &Bench,
    id: ProposalId,
    number: Value,
    fingerprint: Value,
    word: &str,
) -> Value {
    let body = json!({"id": id.0, "number": number, "fingerprint": fingerprint, "answer": word});
    let reply = post(bench.addr(), PROPOSAL_ANSWER_PATH, &body).await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    reply.body
}

// 검증: V19, T15, E17
#[tokio::test]
async fn 팔린_매매는_장_마감_뒤_복기됨으로_바뀌고_실수_기록이_남는다() {
    let bench = Bench::start(kst(monday(), 10, 0)).await;
    bench.ready().await;
    let trade = bench.sold(HYNIX).await;
    bench.script(DemoTurn::Review, &late_stop(Value::Null));

    bench.set(kst(monday(), 16, 1));
    wait_for("마감 뒤에 복기된다", || {
        bench.trade_state(trade) == TradeState::Reviewed
    })
    .await;
    let reviews = Journal::new(bench.store()).reviews().unwrap();
    assert_eq!(reviews.len(), 1);
    assert_eq!(reviews[0].trade, trade);
    assert_eq!(reviews[0].mistakes, vec![Mistake::LateStop]);
    assert_eq!(reviews[0].note, "손절이 늦었다");
    bench.close().await;
}

// 검증: V20, T15
#[tokio::test]
async fn 같은_실수를_두_번_만들면_규칙_개선안이_제안된다() {
    let bench = Bench::start(kst(monday(), 10, 0)).await;
    let (_, id) = proposed(&bench, json!({"risk_per_trade_pct": "0.5"})).await;

    let proposal = bench.store().get_proposal(id).unwrap();
    assert_eq!(proposal.state, ProposalState::Proposed);
    assert_eq!(proposal.evidence.len(), 2, "근거 매매 둘");
    assert!(!proposal.increases_risk, "위험을 줄이는 변경");
    let notices = events(&bench.store(), KIND_PROPOSAL);
    assert_eq!(notices.len(), 1);
    assert_eq!(notices[0]["id"], json!(id.0));
    assert!(notices[0]["number"].is_string() && notices[0]["fingerprint"].is_string());
    bench.close().await;
}

// 검증: V20
#[tokio::test]
async fn 실수가_한_번이면_개선안이_없다() {
    let bench = Bench::start(kst(monday(), 10, 0)).await;
    bench.ready().await;
    bench.sold(HYNIX).await;
    bench.script(
        DemoTurn::Review,
        &late_stop(json!({"risk_per_trade_pct": "0.5"})),
    );

    let reviewed = bench.crew().review_day().await.unwrap();
    assert_eq!(reviewed.reviews.len(), 1);
    assert!(reviewed.proposals.is_empty());
    assert!(events(&bench.store(), KIND_PROPOSAL).is_empty());
    bench.close().await;
}

// 검증: V21
#[tokio::test]
async fn 개선안을_거절하면_규칙이_그대로다() {
    let bench = Bench::start(kst(monday(), 10, 0)).await;
    let (template, id) = proposed(&bench, json!({"risk_per_trade_pct": "0.5"})).await;
    let before = risk_pct(&bench.store(), template);
    let (number, fingerprint) = approval_of(&bench, id).await;

    let body = answer(&bench, id, number, fingerprint, "reject").await;
    assert_eq!(body["result"], json!("rejected"));
    assert_eq!(body["reason"], json!("user_rejected"));
    assert_eq!(
        bench.store().get_proposal(id).unwrap().state,
        ProposalState::Rejected
    );
    assert_eq!(risk_pct(&bench.store(), template), before, "규칙은 그대로");
    bench.close().await;
}

// 검증: V21, E35
#[tokio::test]
async fn 개선안을_승인하면_다음_매매부터_적용된다() {
    let bench = Bench::start(kst(monday(), 10, 0)).await;
    let (template, id) = proposed(&bench, json!({"risk_per_trade_pct": "0.5"})).await;
    let (number, fingerprint) = approval_of(&bench, id).await;

    let wrong = answer(
        &bench,
        id,
        json!("없는-번호"),
        fingerprint.clone(),
        "approve",
    )
    .await;
    assert_eq!(
        wrong["result"],
        json!("rejected"),
        "승인 번호가 맞지 않으면 반영하지 않는다"
    );
    assert_eq!(
        bench.store().get_proposal(id).unwrap().state,
        ProposalState::Proposed
    );
    assert_eq!(risk_pct(&bench.store(), template), d("1"));

    let body = answer(&bench, id, number, fingerprint, "approve").await;
    assert_eq!(body["result"], json!("applied"), "{body}");
    assert_eq!(
        bench.store().get_proposal(id).unwrap().state,
        ProposalState::Applied
    );
    assert_eq!(
        risk_pct(&bench.store(), template),
        d("0.5"),
        "지금 쓰는 템플릿이 개선안의 값이 된다"
    );
    bench.close().await;
}

// 검증: E28, V21
#[tokio::test]
async fn 위험을_키우는_개선안은_승인해도_다음_거래일_시작에_반영된다() {
    let bench = Bench::start(kst(monday(), 10, 0)).await;
    let (template, id) = proposed(&bench, json!({"risk_per_trade_pct": "1.5"})).await;
    let (number, fingerprint) = approval_of(&bench, id).await;

    let body = answer(&bench, id, number, fingerprint, "approve").await;
    assert_eq!(body["result"], json!("cooling"), "{body}");
    assert_eq!(
        bench.store().get_proposal(id).unwrap().state,
        ProposalState::Cooling
    );
    assert_eq!(risk_pct(&bench.store(), template), d("1"), "오늘은 그대로");

    let tuesday = monday().succ_opt().unwrap();
    bench.set(kst(tuesday, 9, 1));
    bench
        .crew()
        .plan_day()
        .await
        .expect("다음 거래일의 하루 판단");
    assert_eq!(
        bench.store().get_proposal(id).unwrap().state,
        ProposalState::Applied
    );
    assert_eq!(
        risk_pct(&bench.store(), template),
        d("1.5"),
        "다음 거래일부터 적용"
    );
    bench.close().await;
}
