//! 승인 답하기(C7): 승인 · 거절 · 지문 다름(E7) · 확인 실패(E8) · 모르는 답(P19).

mod common;

use common::{MIN, T0, ask, buy};
use pi_approval::{Answer, ApprovalNumber, Decision, Error, RejectReason};
use pi_store::ApprovalStatus;

// 검증: C7 M5
#[test]
fn approve_with_matching_fingerprint() {
    let e = common::env();
    let r = e.approvals.request(ask(buy("sig-1"))).unwrap();
    let a = e
        .approvals
        .answer(&r.number, r.fingerprint.as_str(), Decision::Approve)
        .unwrap();
    assert_eq!(a, Answer::Approved);
    let view = e.approvals.get(&r.number).unwrap().unwrap();
    assert_eq!(view.status, ApprovalStatus::Approved);
    assert!(!view.consumed);
}

// 검증: C7 V43 P18
#[test]
fn reject_is_immediate() {
    let e = common::env();
    let r = e.approvals.request(ask(buy("sig-1"))).unwrap();
    let a = e
        .approvals
        .answer(&r.number, r.fingerprint.as_str(), Decision::Reject)
        .unwrap();
    assert_eq!(a, Answer::Rejected(RejectReason::UserRejected));
    let view = e.approvals.get(&r.number).unwrap().unwrap();
    assert_eq!(view.status, ApprovalStatus::Rejected);
}

// 검증: V43 P18
#[test]
fn reject_does_not_need_matching_fingerprint() {
    let e = common::env();
    let r = e.approvals.request(ask(buy("sig-1"))).unwrap();
    let a = e
        .approvals
        .answer(&r.number, "stale-screen", Decision::Reject)
        .unwrap();
    assert_eq!(a, Answer::Rejected(RejectReason::UserRejected));
    assert_eq!(
        e.approvals.get(&r.number).unwrap().unwrap().status,
        ApprovalStatus::Rejected
    );
}

// 검증: E7 V44 C7
#[test]
fn approve_with_changed_content_is_rejected() {
    let e = common::env();
    let r = e.approvals.request(ask(buy("sig-1"))).unwrap();
    let other = pi_approval::fingerprint(&buy("sig-2"));
    let a = e
        .approvals
        .answer(&r.number, other.as_str(), Decision::Approve)
        .unwrap();
    assert_eq!(a, Answer::Rejected(RejectReason::FingerprintMismatch));
    assert_eq!(
        e.approvals.get(&r.number).unwrap().unwrap().status,
        ApprovalStatus::Rejected
    );
    // 주문이 나가지 않는다.
    let err = e.approvals.consume(&r.number, &buy("sig-1")).unwrap_err();
    assert!(
        matches!(err, Error::Denied(RejectReason::NotApproved)),
        "{err:?}"
    );
}

// 검증: E7 C7
#[test]
fn short_code_alone_is_not_a_match() {
    let e = common::env();
    let r = e.approvals.request(ask(buy("sig-1"))).unwrap();
    let a = e
        .approvals
        .answer(&r.number, &r.short_code, Decision::Approve)
        .unwrap();
    assert_eq!(a, Answer::Rejected(RejectReason::FingerprintMismatch));
}

// 검증: E8 P19
#[test]
fn unknown_number_is_rejected() {
    let e = common::env();
    let r = e.approvals.request(ask(buy("sig-1"))).unwrap();
    let unknown = ApprovalNumber::new("f".repeat(32));
    for decision in [Decision::Approve, Decision::Reject] {
        let a = e
            .approvals
            .answer(&unknown, r.fingerprint.as_str(), decision)
            .unwrap();
        assert_eq!(a, Answer::Rejected(RejectReason::NotFound));
    }
    // 다른 요청은 그대로다.
    assert_eq!(
        e.approvals.get(&r.number).unwrap().unwrap().status,
        ApprovalStatus::Pending
    );
}

// 검증: E8 V45 M5
#[test]
fn second_answer_cannot_change_first() {
    let e = common::env();
    let r = e.approvals.request(ask(buy("sig-1"))).unwrap();
    e.approvals
        .answer(&r.number, r.fingerprint.as_str(), Decision::Reject)
        .unwrap();
    let a = e
        .approvals
        .answer(&r.number, r.fingerprint.as_str(), Decision::Approve)
        .unwrap();
    assert_eq!(a, Answer::Rejected(RejectReason::AlreadyResolved));
    assert_eq!(
        e.approvals.get(&r.number).unwrap().unwrap().status,
        ApprovalStatus::Rejected
    );
    let err = e.approvals.consume(&r.number, &buy("sig-1")).unwrap_err();
    assert!(
        matches!(err, Error::Denied(RejectReason::NotApproved)),
        "{err:?}"
    );
}

// 검증: V45 E8 M5
#[test]
fn reapproving_a_used_number_does_not_order_again() {
    let e = common::env();
    let r = e.approvals.request(ask(buy("sig-1"))).unwrap();
    e.approvals
        .answer(&r.number, r.fingerprint.as_str(), Decision::Approve)
        .unwrap();
    e.approvals.consume(&r.number, &buy("sig-1")).unwrap();

    let a = e
        .approvals
        .answer(&r.number, r.fingerprint.as_str(), Decision::Approve)
        .unwrap();
    assert_eq!(a, Answer::Rejected(RejectReason::AlreadyResolved));
    let err = e.approvals.consume(&r.number, &buy("sig-1")).unwrap_err();
    assert!(
        matches!(err, Error::Denied(RejectReason::AlreadyConsumed)),
        "{err:?}"
    );
}

// 검증: E8 C7
#[test]
fn answer_after_signal_validity_is_expired() {
    let e = common::env();
    let r = e.approvals.request(ask(buy("sig-1"))).unwrap();
    e.clock.set(T0 + 30 * MIN);
    let a = e
        .approvals
        .answer(&r.number, r.fingerprint.as_str(), Decision::Approve)
        .unwrap();
    assert_eq!(a, Answer::Rejected(RejectReason::Expired));
    assert_eq!(
        e.approvals.get(&r.number).unwrap().unwrap().status,
        ApprovalStatus::Expired
    );
}

// 검증: P19
#[test]
fn unknown_decision_text_is_reject() {
    assert_eq!(Decision::from_wire("approve"), Decision::Approve);
    for text in [
        "reject", "Approve", "APPROVE", "approve ", "", "yes", "allow",
    ] {
        assert_eq!(Decision::from_wire(text), Decision::Reject, "{text:?}");
    }
}
