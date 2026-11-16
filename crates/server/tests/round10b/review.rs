//! 개선안 승인 때의 위험 다시 판정(V21 · E28 · E35).

use pi_store::{ProposalId, ProposalState, Store, TemplateId};
use price_inha::{DayPlan, DemoTurn, PROPOSAL_ANSWER_PATH, PROPOSALS_PATH};
use rust_decimal::Decimal;
use serde_json::{Value, json};

use crate::common::*;

/// 복기 담당의 답: 그 실수 + 바꿀 규칙.
fn mistake_reply(mistake: &str, change: Value) -> String {
    json!({"mistakes": [mistake], "note": "복기", "change": change}).to_string()
}

/// 지금 쓰는 템플릿의 (한 매매 위험 %, 종목 비중 %).
fn limits(store: &Store, template: i64) -> (Decimal, Decimal) {
    let saved = store.get_template(TemplateId(template)).unwrap();
    let template = pi_template::Template::from_json(&saved.value.to_string()).unwrap();
    (
        template.params.risk_per_trade_pct,
        template.params.symbol_weight_pct,
    )
}

/// 팔린 매매 둘을 같은 실수로 복기해 개선안 하나를 낸다.
async fn propose(bench: &Bench, mistake: &str, change: Value) -> ProposalId {
    bench.sold(HYNIX).await;
    bench.sold(NAVER).await;
    bench.script(DemoTurn::Review, &mistake_reply(mistake, change));
    let reviewed = bench.crew().review_day().await.expect("복기한다");
    assert_eq!(reviewed.proposals.len(), 1, "같은 실수 2건이면 개선안 하나");
    reviewed.proposals[0]
}

/// 다음 거래일 아침으로 옮기고 그날도 매매하기로 한다(하루 매매 횟수 한도가 새로 시작한다).
fn next_trading_day(bench: &Bench) {
    let next = bench.now().date_naive().succ_opt().unwrap();
    bench.set(kst(next, 10, 0));
    bench.trading().decide_day(DayPlan::Trade).unwrap();
}

/// 개선안 목록에서 그 개선안의 승인 번호와 지문.
async fn approval_of(bench: &Bench, id: ProposalId) -> (Value, Value) {
    let reply = get(bench.addr(), PROPOSALS_PATH).await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    let found = reply.body["proposals"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["id"] == json!(id.0))
        .expect("목록에 있다")
        .clone();
    (found["number"].clone(), found["fingerprint"].clone())
}

/// 개선안에 승인으로 답하고 답의 본문을 돌려준다.
async fn approve(bench: &Bench, id: ProposalId) -> Value {
    let (number, fingerprint) = approval_of(bench, id).await;
    let body =
        json!({"id": id.0, "number": number, "fingerprint": fingerprint, "answer": "approve"});
    let reply = post(bench.addr(), PROPOSAL_ANSWER_PATH, &body).await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    reply.body
}

fn state_of(bench: &Bench, id: ProposalId) -> ProposalState {
    bench.store().get_proposal(id).unwrap().state
}

// 검증: V21, E28, E35
#[tokio::test]
async fn 한도를_좁히는_개선안을_먼저_반영한_뒤_다른_개선안을_승인하면_냉각중이고_한도가_넓어지지_않는다()
 {
    let bench = Bench::start(kst(monday(), 10, 0)).await;
    let template = bench.ready().await;
    // 같은 규칙(위험 1 · 종목 비중 10)에서 둘 다 낸다. 둘 다 제안 때는 위험을 키우지 않는다.
    let narrow_risk = propose(&bench, "late_stop", json!({"risk_per_trade_pct": "0.5"})).await;
    next_trading_day(&bench);
    let narrow_weight = propose(&bench, "chased_entry", json!({"symbol_weight_pct": "8"})).await;
    for id in [narrow_risk, narrow_weight] {
        assert!(!bench.store().get_proposal(id).unwrap().increases_risk);
    }
    assert_eq!(limits(&bench.store(), template), (d("1"), d("10")));

    let first = approve(&bench, narrow_risk).await;
    assert_eq!(first["result"], json!("applied"), "{first}");
    assert_eq!(limits(&bench.store(), template), (d("0.5"), d("10")));

    // 둘째의 값은 한 매매 위험이 1이라, 지금 규칙(0.5)보다 넓어진다.
    let second = approve(&bench, narrow_weight).await;
    assert_eq!(second["result"], json!("cooling"), "{second}");
    assert_eq!(state_of(&bench, narrow_weight), ProposalState::Cooling);
    assert_eq!(
        limits(&bench.store(), template),
        (d("0.5"), d("10")),
        "오늘은 한도가 넓어지지 않는다"
    );
    bench.close().await;
}

// 검증: V21, E28, E35
#[tokio::test]
async fn 제안_때부터_위험한_개선안도_냉각으로_가고_한도가_그대로다() {
    let bench = Bench::start(kst(monday(), 10, 0)).await;
    let template = bench.ready().await;
    let id = propose(&bench, "late_stop", json!({"risk_per_trade_pct": "1.5"})).await;
    assert!(bench.store().get_proposal(id).unwrap().increases_risk);

    let body = approve(&bench, id).await;
    assert_eq!(body["result"], json!("cooling"), "{body}");
    assert_eq!(state_of(&bench, id), ProposalState::Cooling);
    assert_eq!(limits(&bench.store(), template), (d("1"), d("10")));
    bench.close().await;
}

// 검증: V21, E35
#[tokio::test]
async fn 지금과_같은_규칙에서_낸_안전한_개선안은_바로_반영된다() {
    let bench = Bench::start(kst(monday(), 10, 0)).await;
    let template = bench.ready().await;
    let id = propose(&bench, "late_stop", json!({"risk_per_trade_pct": "0.5"})).await;

    let body = approve(&bench, id).await;
    assert_eq!(body["result"], json!("applied"), "{body}");
    assert_eq!(state_of(&bench, id), ProposalState::Applied);
    assert_eq!(limits(&bench.store(), template), (d("0.5"), d("10")));
    bench.close().await;
}
