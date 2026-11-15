//! 2차 2번: 실행 때 지문이 달라 무효가 된 승인은 `get()`에서 거절(까닭: 내용 바뀜)로 보인다.
//! 다시 켜도 같다.

use crate::common::{ask, buy, env, order, reopen};
use pi_approval::{
    Answer, ApprovalNumber, Approvals, Decision, RejectReason, Side, SignalKind, Subject,
};
use pi_store::ApprovalStatus;

fn approved(approvals: &Approvals, subject: &Subject) -> ApprovalNumber {
    let r = approvals.request(ask(subject.clone())).unwrap();
    let a = approvals
        .answer(&r.number, r.fingerprint.as_str(), Decision::Approve)
        .unwrap();
    assert_eq!(a, Answer::Approved);
    r.number
}

fn changed() -> Subject {
    let mut o = order("sig-1", Side::Buy, SignalKind::Buy);
    o.quantity = 20;
    Subject::Order(o)
}

// 검증: E7 V44
#[test]
fn approval_voided_at_execution_shows_as_rejected() {
    let e = env();
    let n = approved(&e.approvals, &buy("sig-1"));
    let before = e.approvals.get(&n).unwrap().unwrap();
    assert_eq!(before.status, ApprovalStatus::Approved);
    assert_eq!(before.reject_reason, None);
    assert!(e.approvals.consume(&n, &changed()).is_err());
    let view = e.approvals.get(&n).unwrap().unwrap();
    assert_eq!(view.status, ApprovalStatus::Rejected);
    assert_eq!(view.reject_reason, Some(RejectReason::FingerprintMismatch));
    assert!(!view.consumed);
}

// 검증: E7 M5
#[test]
fn voided_approval_stays_rejected_after_restart() {
    let e = env();
    let n = approved(&e.approvals, &buy("sig-1"));
    assert!(e.approvals.consume(&n, &changed()).is_err());
    drop(e.approvals);
    let (_store, again) = reopen(&e.path, e.clock.clone());
    again.on_restart().unwrap();
    let view = again.get(&n).unwrap().unwrap();
    assert_eq!(view.status, ApprovalStatus::Rejected);
    assert_eq!(view.reject_reason, Some(RejectReason::FingerprintMismatch));
    // 다시 켠 뒤에도 원래 내용으로 쓸 수 없다.
    assert!(again.consume(&n, &buy("sig-1")).is_err());
}

// 검증: E7 V44
#[test]
fn unvoided_approval_has_no_reject_reason() {
    let e = env();
    let n = approved(&e.approvals, &buy("sig-1"));
    e.approvals.consume(&n, &buy("sig-1")).unwrap();
    let view = e.approvals.get(&n).unwrap().unwrap();
    assert_eq!(view.status, ApprovalStatus::Approved);
    assert_eq!(view.reject_reason, None);
    assert!(view.consumed);
}
