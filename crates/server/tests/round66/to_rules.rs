//! 1층으로 가는 지침 후보 → 개선안: 어느 규칙 값을 어떻게 바꾸자는 것인지 글에서 확실히 읽히면 지금의 개선안 길
//! (`Improvement` · 사람 승인)로 보낸다. 확실히 읽히지 않으면 지금처럼 `to_rules` 이벤트만 남는다.

use pi_journal::{Improvement, Mistake};
use pi_store::ProposalState;
use price_inha::{KIND_GUIDELINE, KIND_PROPOSAL};
use rust_decimal::Decimal;
use serde_json::{Value, json};

use crate::common::{Bench, events};

fn to_rules(bench: &Bench) -> Vec<Value> {
    events(&bench.store(), KIND_GUIDELINE)
        .into_iter()
        .filter(|event| event["action"] == json!("to_rules"))
        .collect()
}

// 검증: V98 R7-2 R7-3 R8 (값과 칸이 읽히는 1층 되풀이는 개선안 · 승인 요청이 되고 이벤트가 그 개선안을 가리킨다)
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn 칸과_값이_읽히는_1층_되풀이는_개선안과_승인_요청이_된다() {
    let bench = Bench::start().await;
    let trades = bench.reviewed_with(3, &Mistake::Other("일일 손실 한도를 2%로 줄인다".into()));
    bench.close_day().await;

    let ledger = bench.ledger();
    assert!(
        ledger["guidelines"].as_array().is_none_or(Vec::is_empty),
        "지침이 되지 않는다: {ledger}"
    );
    let proposals = bench
        .store()
        .proposals_in_state(ProposalState::Proposed)
        .unwrap();
    assert_eq!(proposals.len(), 1, "{proposals:?}");
    let proposal = &proposals[0];
    assert_eq!(proposal.evidence, trades, "근거는 복기한 매매들");
    assert!(!proposal.increases_risk, "좁히는 변경");
    let improvement = Improvement::from_proposal(proposal).unwrap();
    assert_eq!(improvement.after.daily_loss_limit_pct, Decimal::from(2));
    assert_eq!(
        improvement.before.daily_loss_limit_pct,
        Decimal::from(3),
        "스윙 템플릿의 지금 값"
    );

    let found = to_rules(&bench);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0]["why"], json!("layer_one"));
    assert_eq!(found[0]["proposal"], json!(proposal.id.0), "{found:?}");
    let asked = events(&bench.store(), KIND_PROPOSAL);
    assert_eq!(asked.len(), 1, "승인 요청 이벤트 하나: {asked:?}");
    assert_eq!(asked[0]["id"], json!(proposal.id.0));
    assert!(asked[0]["number"].as_str().is_some_and(|n| !n.is_empty()));

    bench.close_day().await;
    assert_eq!(
        bench
            .store()
            .proposals_in_state(ProposalState::Proposed)
            .unwrap()
            .len(),
        1,
        "같은 후보로 다시 마감해도 개선안을 또 내지 않는다"
    );
    bench.close().await;
}

// 검증: V98 R7-2 R8 (확실히 읽히지 않는 1층 되풀이는 이벤트만 남고 개선안이 없다)
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn 읽히지_않는_1층_되풀이는_이벤트만_남고_개선안이_없다() {
    let bench = Bench::start().await;
    bench.reviewed_with(3, &Mistake::Other("금지 종목을 늘린다".into()));
    bench.close_day().await;

    let found = to_rules(&bench);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0]["proposal"], json!(null), "{found:?}");
    assert!(
        bench
            .store()
            .proposals_in_state(ProposalState::Proposed)
            .unwrap()
            .is_empty()
    );
    assert!(events(&bench.store(), KIND_PROPOSAL).is_empty());
    bench.close().await;
}
