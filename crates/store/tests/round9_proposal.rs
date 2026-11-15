#![cfg(test)]
//! 9차 3번: 규칙 개선안 표 (M4, `spec.md` F5).
//!
//! 제안됨 · 냉각중 · 반영됨 · 거절됨과 반영 시각. 근거 매매는 2건 이상, 반영은 승인 뒤에만,
//! 위험을 키우는 개선안은 냉각중을 거친다.

mod common;

use common::{t, temp_store};
use pi_store::{
    AccountKind, ApprovalId, ApprovalStatus, ApprovalUse, DenyReason, NewApprovalRequest,
    NewProposal, NewTrade, PROPOSAL_MIN_EVIDENCE, Proposal, ProposalId, ProposalState, Store,
    StoreError, TradeId, Transition,
};
use serde_json::{Value, json};

const FP: &str = "sha256:proposal-1";

fn trade(store: &Store) -> TradeId {
    store
        .create_trade(NewTrade {
            symbol: "005930".to_string(),
            account: AccountKind::Paper,
            at: t(0),
        })
        .unwrap()
        .id
}

fn content() -> Value {
    json!({ "규칙": "갭 상승 첫 5분에는 사지 않는다", "바꾸는_값": null })
}

fn new_proposal(evidence: Vec<TradeId>, increases_risk: bool) -> NewProposal {
    NewProposal {
        content: content(),
        evidence,
        increases_risk,
        at: t(10),
    }
}

/// 근거 매매 둘로 낸 개선안.
fn proposed(store: &Store, increases_risk: bool) -> Proposal {
    let evidence = vec![trade(store), trade(store)];
    store
        .create_proposal(new_proposal(evidence, increases_risk))
        .unwrap()
}

fn approved(store: &Store, fingerprint: &str) -> ApprovalId {
    let made = store
        .create_approval(NewApprovalRequest {
            fingerprint: fingerprint.to_string(),
            summary: json!({ "개선안": 1 }),
            created_at: t(0),
            expires_at: t(100_000),
        })
        .unwrap();
    store
        .resolve_approval(made.id, ApprovalStatus::Approved, t(1))
        .unwrap()
        .id
}

fn use_of(id: ApprovalId) -> ApprovalUse {
    ApprovalUse {
        id,
        fingerprint: FP.to_string(),
    }
}

fn denied(err: StoreError, from: ProposalState, to: ProposalState) {
    match err {
        StoreError::TransitionDenied { transition, reason } => {
            assert_eq!(transition, Transition::Proposal { from, to });
            assert_eq!(reason, DenyReason::NotAllowed);
        }
        other => panic!("{from:?} → {to:?}: {other:?}"),
    }
}

// 검증: M4
#[test]
fn 근거_매매_둘로_내면_제안됨이다() {
    assert_eq!(PROPOSAL_MIN_EVIDENCE, 2);
    let (_dir, _path, store) = temp_store();
    let a = trade(&store);
    let b = trade(&store);
    let made = store
        .create_proposal(new_proposal(vec![a, b], false))
        .unwrap();
    assert_eq!(made.id, ProposalId(1));
    assert_eq!(made.state, ProposalState::Proposed);
    assert_eq!(made.content, content());
    assert_eq!(made.evidence, vec![a, b]);
    assert!(!made.increases_risk);
    assert_eq!(made.approval, None);
    assert_eq!(made.created_at, t(10));
    assert_eq!(made.updated_at, t(10));
    assert_eq!(made.applied_at, None);
    assert_eq!(store.get_proposal(made.id).unwrap(), made);
}

// 검증: M4
#[test]
fn 서로_다른_근거_매매가_둘보다_적으면_내지_못한다() {
    let (_dir, _path, store) = temp_store();
    let a = trade(&store);
    for (evidence, count) in [(vec![], 0), (vec![a], 1), (vec![a, a, a], 1)] {
        let err = store
            .create_proposal(new_proposal(evidence.clone(), false))
            .unwrap_err();
        assert!(
            matches!(err, StoreError::ProposalEvidenceTooFew { count: c } if c == count),
            "{evidence:?}: {err:?}"
        );
    }
    // 거부는 번호를 쓰지 않는다.
    let b = trade(&store);
    let made = store
        .create_proposal(new_proposal(vec![a, b], false))
        .unwrap();
    assert_eq!(made.id, ProposalId(1));
}

// 검증: M4
#[test]
fn 겹친_근거는_한_번만_처음_나온_순서로_남는다() {
    let (_dir, _path, store) = temp_store();
    let a = trade(&store);
    let b = trade(&store);
    let made = store
        .create_proposal(new_proposal(vec![b, a, b, a], true))
        .unwrap();
    assert_eq!(made.evidence, vec![b, a]);
}

// 검증: M4, M2
#[test]
fn 없는_매매를_근거로_내지_못한다() {
    let (_dir, _path, store) = temp_store();
    let a = trade(&store);
    let err = store
        .create_proposal(new_proposal(vec![a, TradeId(99)], false))
        .unwrap_err();
    assert!(matches!(err, StoreError::TradeNotFound(TradeId(99))));
    assert!(
        store
            .proposals_in_state(ProposalState::Proposed)
            .unwrap()
            .is_empty()
    );
}

// 검증: M4
#[test]
fn 없는_개선안은_찾을_수_없다() {
    let (_dir, _path, store) = temp_store();
    assert!(matches!(
        store.get_proposal(ProposalId(5)),
        Err(StoreError::ProposalNotFound(ProposalId(5)))
    ));
    assert!(matches!(
        store.reject_proposal(ProposalId(5), t(1)),
        Err(StoreError::ProposalNotFound(ProposalId(5)))
    ));
}

// 검증: M4, M5
#[test]
fn 위험을_키우지_않는_개선안은_승인하면_바로_반영된다() {
    let (_dir, _path, store) = temp_store();
    let made = proposed(&store, false);
    let approval = approved(&store, FP);
    let applied = store
        .approve_proposal(made.id, use_of(approval), t(20))
        .unwrap();
    assert_eq!(applied.state, ProposalState::Applied);
    assert_eq!(applied.applied_at, Some(t(20)));
    assert_eq!(applied.approval, Some(approval));
    assert_eq!(applied.updated_at, t(20));
    assert_eq!(store.get_proposal(made.id).unwrap(), applied);
    assert_eq!(
        store.get_approval(approval).unwrap().consumed_at,
        Some(t(20))
    );
}

// 검증: M4, M5
#[test]
fn 위험을_키우는_개선안은_냉각중을_거쳐_반영된다() {
    let (_dir, _path, store) = temp_store();
    let made = proposed(&store, true);
    let approval = approved(&store, FP);
    let cooling = store
        .approve_proposal(made.id, use_of(approval), t(20))
        .unwrap();
    assert_eq!(cooling.state, ProposalState::Cooling);
    assert_eq!(cooling.applied_at, None);
    assert_eq!(cooling.approval, Some(approval));

    let applied = store.apply_cooled_proposal(made.id, t(90_000)).unwrap();
    assert_eq!(applied.state, ProposalState::Applied);
    assert_eq!(applied.applied_at, Some(t(90_000)));
    assert_eq!(applied.approval, Some(approval));
    assert_eq!(applied.updated_at, t(90_000));
}

// 검증: M4, M5
#[test]
fn 승인을_쓸_수_없으면_반영되지_않는다() {
    let (_dir, _path, store) = temp_store();
    let made = proposed(&store, false);

    let waiting = store
        .create_approval(NewApprovalRequest {
            fingerprint: FP.to_string(),
            summary: json!({}),
            created_at: t(0),
            expires_at: t(100_000),
        })
        .unwrap();
    assert!(matches!(
        store.approve_proposal(made.id, use_of(waiting.id), t(20)),
        Err(StoreError::ApprovalNotUsable {
            status: ApprovalStatus::Pending,
            ..
        })
    ));
    let other = approved(&store, "sha256:another-proposal");
    assert!(matches!(
        store.approve_proposal(made.id, use_of(other), t(21)),
        Err(StoreError::ApprovalFingerprintMismatch { .. })
    ));
    assert!(matches!(
        store.approve_proposal(made.id, use_of(ApprovalId(50)), t(22)),
        Err(StoreError::ApprovalNotFound(ApprovalId(50)))
    ));
    assert_eq!(store.get_proposal(made.id).unwrap(), made);
    assert_eq!(store.get_approval(other).unwrap().consumed_at, None);
}

// 검증: M4
#[test]
fn 제안됨은_거절될_수_있고_거절된_뒤에는_움직이지_않는다() {
    let (_dir, _path, store) = temp_store();
    let made = proposed(&store, false);
    let rejected = store.reject_proposal(made.id, t(30)).unwrap();
    assert_eq!(rejected.state, ProposalState::Rejected);
    assert_eq!(rejected.applied_at, None);
    assert_eq!(rejected.updated_at, t(30));

    let approval = approved(&store, FP);
    denied(
        store
            .approve_proposal(made.id, use_of(approval), t(31))
            .unwrap_err(),
        ProposalState::Rejected,
        ProposalState::Applied,
    );
    denied(
        store.reject_proposal(made.id, t(32)).unwrap_err(),
        ProposalState::Rejected,
        ProposalState::Rejected,
    );
    denied(
        store.apply_cooled_proposal(made.id, t(33)).unwrap_err(),
        ProposalState::Rejected,
        ProposalState::Applied,
    );
    assert_eq!(store.get_proposal(made.id).unwrap(), rejected);
    assert_eq!(store.get_approval(approval).unwrap().consumed_at, None);
}

// 검증: M4
#[test]
fn 제안됨에서_바로_냉각을_끝낼_수_없다() {
    let (_dir, _path, store) = temp_store();
    for risk in [false, true] {
        let made = proposed(&store, risk);
        denied(
            store.apply_cooled_proposal(made.id, t(40)).unwrap_err(),
            ProposalState::Proposed,
            ProposalState::Applied,
        );
        assert_eq!(store.get_proposal(made.id).unwrap(), made);
    }
}

// 검증: M4
#[test]
fn 냉각중과_반영됨에서는_승인_거절이_없다() {
    let (_dir, _path, store) = temp_store();
    let made = proposed(&store, true);
    let first = approved(&store, FP);
    let cooling = store
        .approve_proposal(made.id, use_of(first), t(20))
        .unwrap();
    let second = approved(&store, FP);
    denied(
        store
            .approve_proposal(made.id, use_of(second), t(21))
            .unwrap_err(),
        ProposalState::Cooling,
        ProposalState::Cooling,
    );
    denied(
        store.reject_proposal(made.id, t(22)).unwrap_err(),
        ProposalState::Cooling,
        ProposalState::Rejected,
    );
    assert_eq!(store.get_proposal(made.id).unwrap(), cooling);

    let applied = store.apply_cooled_proposal(made.id, t(23)).unwrap();
    denied(
        store.apply_cooled_proposal(made.id, t(24)).unwrap_err(),
        ProposalState::Applied,
        ProposalState::Applied,
    );
    denied(
        store.reject_proposal(made.id, t(25)).unwrap_err(),
        ProposalState::Applied,
        ProposalState::Rejected,
    );
    assert_eq!(store.get_proposal(made.id).unwrap(), applied);
    assert_eq!(store.get_approval(second).unwrap().consumed_at, None);
}

// 검증: M4
#[test]
fn 상태별로_찾을_수_있고_번호_순서다() {
    let (_dir, _path, store) = temp_store();
    let a = proposed(&store, false);
    let b = proposed(&store, true);
    let c = proposed(&store, false);
    store.reject_proposal(b.id, t(30)).unwrap();
    let ids = |state| -> Vec<ProposalId> {
        store
            .proposals_in_state(state)
            .unwrap()
            .into_iter()
            .map(|p| p.id)
            .collect()
    };
    assert_eq!(ids(ProposalState::Proposed), vec![a.id, c.id]);
    assert_eq!(ids(ProposalState::Rejected), vec![b.id]);
    assert!(ids(ProposalState::Cooling).is_empty());
    assert!(ids(ProposalState::Applied).is_empty());
}

// 검증: M4, D6
#[test]
fn 다시_열어도_개선안이_그대로다() {
    let (_dir, path, store) = temp_store();
    let made = proposed(&store, true);
    let approval = approved(&store, FP);
    let cooling = store
        .approve_proposal(made.id, use_of(approval), t(20))
        .unwrap();
    let other = proposed(&store, false);
    drop(store);
    let store = Store::open(&path).unwrap();
    assert_eq!(store.get_proposal(made.id).unwrap(), cooling);
    assert_eq!(store.get_proposal(other.id).unwrap(), other);
    let applied = store.apply_cooled_proposal(made.id, t(90_000)).unwrap();
    assert_eq!(applied.applied_at, Some(t(90_000)));
}
