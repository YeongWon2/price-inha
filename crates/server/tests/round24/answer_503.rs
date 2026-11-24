//! 21차 4번: 승인 답의 오류를 버리지 않는다. 개선안 답(폰 · 사무실이 같은 경로를 부른다)에서
//! - 쓰는 템플릿 값을 읽을 수 없으면(템플릿을 고르지 않았다) 승인을 쓰지 않은 채 "rejected/not_found"로 답하지 않고
//!   503(지금 처리할 수 없음)으로 답한다. 승인 요청은 기다림 그대로, 개선안은 제안됨 그대로다.
//! - 거절을 저장하다 실패하면 "이미 닫힘"으로 답하지 않고 5xx로 답한다.
//!
//! 개선안과 승인 요청은 저장소 · 승인 모듈의 공개 길로 직접 만든다(근거 매매 2건).

use pi_approval::{Ask, Subject};
use pi_store::{
    AccountKind, ApprovalStatus, NewProposal, NewTrade, ProposalId, ProposalState, TradeId,
};
use price_inha::PROPOSAL_ANSWER_PATH;
use serde_json::{Value, json};

use crate::common::{Bench, Plan, kst, monday, ms, post, sql};

/// 근거 매매 두 건으로 제안됨 개선안 하나와 그 승인 요청(번호 · 지문)을 만든다.
fn pending_proposal(bench: &Bench) -> (ProposalId, String, String) {
    let store = bench.store();
    let at = ms(bench.now());
    let mut evidence: Vec<TradeId> = Vec::new();
    for _ in 0..2 {
        let trade = store
            .create_trade(NewTrade {
                symbol: "000660".to_string(),
                account: AccountKind::Paper,
                at,
            })
            .unwrap();
        evidence.push(trade.id);
    }
    let proposal = store
        .create_proposal(NewProposal {
            content: json!({"시험": "개선안"}),
            evidence,
            increases_risk: false,
            at,
        })
        .unwrap();
    let asked = bench
        .approvals()
        .request(Ask {
            subject: Subject::RuleImprovement {
                proposal: proposal.id,
            },
            reason: "시험용 개선안".to_string(),
        })
        .unwrap();
    (
        proposal.id,
        asked.number.as_str().to_string(),
        asked.fingerprint.as_str().to_string(),
    )
}

fn answer_body(id: ProposalId, number: &str, fingerprint: &str, answer: &str) -> Value {
    json!({"id": id.0, "number": number, "fingerprint": fingerprint, "answer": answer})
}

fn still_waiting(bench: &Bench, id: ProposalId, number: &str) {
    let view = bench
        .approvals()
        .get(&pi_approval::ApprovalNumber::new(number))
        .unwrap()
        .expect("승인 요청이 있다");
    assert_eq!(
        view.status,
        ApprovalStatus::Pending,
        "승인 요청은 기다림 그대로다(쓰지 않았다)"
    );
    assert_eq!(
        bench.store().get_proposal(id).unwrap().state,
        ProposalState::Proposed,
        "개선안은 제안됨 그대로다"
    );
}

// 검증: V21 E35 E28
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 쓰는_템플릿이_없어_승인할_수_없으면_503이고_승인은_그대로_남는다() {
    let bench = Plan::demo().start(kst(monday(), 10, 0)).await;
    let (id, number, fingerprint) = pending_proposal(&bench);

    let reply = post(
        bench.addr(),
        PROPOSAL_ANSWER_PATH,
        &answer_body(id, &number, &fingerprint, "approve"),
    )
    .await;
    assert_eq!(
        reply.status, 503,
        "지금 처리할 수 없음이다(승인을 쓰지 않은 채 rejected/not_found로 답하지 않는다): {}",
        reply.text
    );
    still_waiting(&bench, id, &number);
    bench.close().await;
}

// 검증: V21 E35 E28
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 쓰는_템플릿이_없어_거절을_처리할_수_없으면_503이고_승인은_그대로_남는다() {
    let bench = Plan::demo().start(kst(monday(), 10, 0)).await;
    let (id, number, fingerprint) = pending_proposal(&bench);

    let reply = post(
        bench.addr(),
        PROPOSAL_ANSWER_PATH,
        &answer_body(id, &number, &fingerprint, "reject"),
    )
    .await;
    assert_eq!(
        reply.status, 503,
        "거절도 지금 처리할 수 없으면 503이다: {}",
        reply.text
    );
    still_waiting(&bench, id, &number);
    bench.close().await;
}

// 검증: V21 E35
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 거절을_저장하지_못하면_이미_닫힘이라고_하지_않고_실패로_답한다() {
    let bench = Plan::demo().start(kst(monday(), 10, 0)).await;
    bench.ready().await;
    let (id, number, fingerprint) = pending_proposal(&bench);
    // 개선안 표의 고침을 막아 거절 저장이 실패하게 한다.
    sql(
        bench.home(),
        "CREATE TRIGGER block_proposal_update BEFORE UPDATE ON proposals \
         BEGIN SELECT RAISE(ABORT, '시험: 저장 실패'); END;",
    );

    let reply = post(
        bench.addr(),
        PROPOSAL_ANSWER_PATH,
        &answer_body(id, &number, &fingerprint, "reject"),
    )
    .await;
    assert!(
        reply.status >= 500,
        "저장 실패는 실패로 답한다(200 already_resolved가 아니다): {} {}",
        reply.status,
        reply.text
    );
    sql(bench.home(), "DROP TRIGGER block_proposal_update;");
    assert_eq!(
        bench.store().get_proposal(id).unwrap().state,
        ProposalState::Proposed,
        "저장하지 못했으니 개선안은 제안됨 그대로다"
    );
    bench.close().await;
}
