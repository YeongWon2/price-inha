//! 2차 1번: 실전 승격 · 규칙 개선을 이 크레이트로만 끝까지(실제 임시 저장소).
//!
//! `promote_live` · `approve_proposal`이 번호 · 전체 정지를 확인하고, 대상의 지금 모습에서 지문을 계산해
//! 저장소의 `promote_template_live` · `approve_proposal`을 부른다. 실전 승격 지문에는 템플릿 판이 들어간다.

use crate::common::{T0, ask, env, full_stop, owner_restart, pending_template, proposed};
use pi_approval::{
    Answer, ApprovalNumber, Approvals, Decision, Error, RejectReason, Subject, fingerprint,
};
use pi_store::{ApprovalStatus, ProposalState, TemplateId, TemplateState, UnixMillis};

fn approved(approvals: &Approvals, subject: &Subject) -> ApprovalNumber {
    let r = approvals.request(ask(subject.clone())).unwrap();
    let a = approvals
        .answer(&r.number, r.fingerprint.as_str(), Decision::Approve)
        .unwrap();
    assert_eq!(a, Answer::Approved);
    r.number
}

fn denied<T: std::fmt::Debug>(result: Result<T, Error>) -> RejectReason {
    match result {
        Err(Error::Denied(reason)) => reason,
        other => panic!("Denied가 아니다: {other:?}"),
    }
}

fn promo(template: TemplateId, revision: u32) -> Subject {
    Subject::LivePromotion { template, revision }
}

// 검증: P18 V44 M5
#[test]
fn approved_promotion_goes_live_through_store() {
    let e = env();
    let t = pending_template(&e.store);
    let n = approved(&e.approvals, &promo(t, 1));
    let live = e.approvals.promote_live(&n, t).unwrap();
    assert_eq!(live.state, TemplateState::Live);
    assert!(live.approval.is_some());
    let stored = e.store.get_template(t).unwrap();
    assert_eq!(stored.state, TemplateState::Live);
    assert_eq!(stored.approval, live.approval);
    let view = e.approvals.get(&n).unwrap().unwrap();
    assert!(view.consumed);
    assert_eq!(view.status, ApprovalStatus::Approved);
    // 같은 승인은 다시 쓸 수 없다.
    assert_eq!(
        denied(e.approvals.consume(&n, &promo(t, 1))),
        RejectReason::AlreadyConsumed
    );
}

// 검증: V42 P18
#[test]
fn promotion_without_approval_does_not_go_live() {
    let e = env();
    let t = pending_template(&e.store);
    let r = e.approvals.request(ask(promo(t, 1))).unwrap();
    assert_eq!(
        denied(e.approvals.promote_live(&r.number, t)),
        RejectReason::NotApproved
    );
    assert_eq!(
        e.store.get_template(t).unwrap().state,
        TemplateState::PromotionPending
    );
}

// 검증: E8 P19
#[test]
fn unknown_number_cannot_promote_or_approve() {
    let e = env();
    let t = pending_template(&e.store);
    let p = proposed(&e.store, false);
    let nobody = ApprovalNumber::new("00000000000000000000000000000000");
    assert_eq!(
        denied(e.approvals.promote_live(&nobody, t)),
        RejectReason::NotFound
    );
    assert_eq!(
        denied(e.approvals.approve_proposal(&nobody, p)),
        RejectReason::NotFound
    );
    assert_eq!(
        e.store.get_template(t).unwrap().state,
        TemplateState::PromotionPending
    );
    assert_eq!(
        e.store.get_proposal(p).unwrap().state,
        ProposalState::Proposed
    );
}

// 검증: P20 V44
#[test]
fn full_stop_blocks_promotion_and_keeps_approval() {
    let e = env();
    let t = pending_template(&e.store);
    let n = approved(&e.approvals, &promo(t, 1));
    full_stop(&e.store);
    assert_eq!(
        denied(e.approvals.promote_live(&n, t)),
        RejectReason::FullStop
    );
    assert_eq!(
        e.store.get_template(t).unwrap().state,
        TemplateState::PromotionPending
    );
    assert!(!e.approvals.get(&n).unwrap().unwrap().consumed);
    // 사람이 다시 시작한 뒤에는 그 승인으로 올린다.
    owner_restart(&e.store);
    let live = e.approvals.promote_live(&n, t).unwrap();
    assert_eq!(live.state, TemplateState::Live);
}

// 검증: E7 V44 M5
#[test]
fn template_revised_after_approval_cannot_go_live() {
    let e = env();
    let t = pending_template(&e.store);
    let n = approved(&e.approvals, &promo(t, 1));
    // 승인 뒤 승격을 되돌리고 값을 고쳐 다시 올렸다: 판 2.
    e.store
        .reject_template_promotion(t, UnixMillis(T0 - 80))
        .unwrap();
    let revised = e
        .store
        .revise_template(t, &serde_json::json!({ "한도": 500 }), UnixMillis(T0 - 70))
        .unwrap();
    assert_eq!(revised.revision, 2);
    e.store
        .submit_template_for_promotion(
            t,
            &serde_json::json!({ "기준": "통과" }),
            UnixMillis(T0 - 60),
        )
        .unwrap();
    assert_eq!(
        denied(e.approvals.promote_live(&n, t)),
        RejectReason::FingerprintMismatch
    );
    let stored = e.store.get_template(t).unwrap();
    assert_eq!(stored.state, TemplateState::PromotionPending);
    assert_eq!(stored.approval, None);
    // 무효가 된 승인은 다시 시도해도 쓸 수 없다.
    assert!(matches!(
        e.approvals.promote_live(&n, t),
        Err(Error::Denied(_))
    ));
    // 판 2를 새로 승인받으면 올린다.
    let n2 = approved(&e.approvals, &promo(t, 2));
    assert_ne!(n2, n);
    let live = e.approvals.promote_live(&n2, t).unwrap();
    assert_eq!(live.state, TemplateState::Live);
    assert_eq!(live.revision, 2);
}

// 검증: E7 M5
#[test]
fn same_template_different_revision_has_different_fingerprint_and_is_refused() {
    let e = env();
    let t = TemplateId(9);
    assert_ne!(fingerprint(&promo(t, 1)), fingerprint(&promo(t, 2)));
    let n = approved(&e.approvals, &promo(t, 1));
    assert_eq!(
        denied(e.approvals.consume(&n, &promo(t, 2))),
        RejectReason::FingerprintMismatch
    );
    assert!(matches!(
        e.approvals.consume(&n, &promo(t, 1)),
        Err(Error::Denied(_))
    ));
}

// 검증: E7 M5
#[test]
fn approval_for_other_target_cannot_promote_or_approve() {
    let e = env();
    let t1 = pending_template(&e.store);
    let t2 = pending_template(&e.store);
    let p = proposed(&e.store, false);
    let for_t1 = approved(&e.approvals, &promo(t1, 1));
    assert_eq!(
        denied(e.approvals.promote_live(&for_t1, t2)),
        RejectReason::FingerprintMismatch
    );
    let for_t2 = approved(&e.approvals, &promo(t2, 1));
    assert_eq!(
        denied(e.approvals.approve_proposal(&for_t2, p)),
        RejectReason::FingerprintMismatch
    );
    let for_p = approved(&e.approvals, &Subject::RuleImprovement { proposal: p });
    assert_eq!(
        denied(e.approvals.promote_live(&for_p, t1)),
        RejectReason::FingerprintMismatch
    );
    assert_eq!(
        e.store.get_template(t1).unwrap().state,
        TemplateState::PromotionPending
    );
    assert_eq!(
        e.store.get_template(t2).unwrap().state,
        TemplateState::PromotionPending
    );
    assert_eq!(
        e.store.get_proposal(p).unwrap().state,
        ProposalState::Proposed
    );
}

// 검증: P18 V44 M5
#[test]
fn approved_proposal_is_applied_or_cooled_through_store() {
    let e = env();
    let safe = proposed(&e.store, false);
    let risky = proposed(&e.store, true);
    let n_safe = approved(&e.approvals, &Subject::RuleImprovement { proposal: safe });
    let n_risky = approved(&e.approvals, &Subject::RuleImprovement { proposal: risky });
    let applied = e.approvals.approve_proposal(&n_safe, safe).unwrap();
    assert_eq!(applied.state, ProposalState::Applied);
    assert!(applied.approval.is_some());
    let cooling = e.approvals.approve_proposal(&n_risky, risky).unwrap();
    assert_eq!(cooling.state, ProposalState::Cooling);
    assert_eq!(
        e.store.get_proposal(risky).unwrap().state,
        ProposalState::Cooling
    );
    assert!(e.approvals.get(&n_safe).unwrap().unwrap().consumed);
    assert!(e.approvals.get(&n_risky).unwrap().unwrap().consumed);
    assert_eq!(
        denied(e.approvals.approve_proposal(&n_safe, safe)),
        RejectReason::AlreadyConsumed
    );
}

// 검증: V42 P18
#[test]
fn proposal_without_approval_is_not_applied() {
    let e = env();
    let p = proposed(&e.store, false);
    let r = e
        .approvals
        .request(ask(Subject::RuleImprovement { proposal: p }))
        .unwrap();
    assert_eq!(
        denied(e.approvals.approve_proposal(&r.number, p)),
        RejectReason::NotApproved
    );
    e.approvals
        .answer(&r.number, r.fingerprint.as_str(), Decision::Reject)
        .unwrap();
    assert_eq!(
        denied(e.approvals.approve_proposal(&r.number, p)),
        RejectReason::NotApproved
    );
    assert_eq!(
        e.store.get_proposal(p).unwrap().state,
        ProposalState::Proposed
    );
}

// 검증: P20
#[test]
fn rule_improvement_works_during_full_stop() {
    let e = env();
    let p = proposed(&e.store, false);
    let n = approved(&e.approvals, &Subject::RuleImprovement { proposal: p });
    full_stop(&e.store);
    let applied = e.approvals.approve_proposal(&n, p).unwrap();
    assert_eq!(applied.state, ProposalState::Applied);
}

// 검증: V45 M5
#[test]
fn store_refusal_does_not_use_up_approval() {
    let e = env();
    let p = proposed(&e.store, false);
    let n = approved(&e.approvals, &Subject::RuleImprovement { proposal: p });
    e.store.reject_proposal(p, UnixMillis(T0 - 40)).unwrap();
    assert!(matches!(
        e.approvals.approve_proposal(&n, p),
        Err(Error::Store(_))
    ));
    assert!(!e.approvals.get(&n).unwrap().unwrap().consumed);
    assert_eq!(
        e.store.get_proposal(p).unwrap().state,
        ProposalState::Rejected
    );

    let t = TemplateId(77);
    let n = approved(&e.approvals, &promo(t, 1));
    assert!(matches!(
        e.approvals.promote_live(&n, t),
        Err(Error::Store(_))
    ));
    assert!(!e.approvals.get(&n).unwrap().unwrap().consumed);
}
