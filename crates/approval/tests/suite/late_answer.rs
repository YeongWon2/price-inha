//! 2차 3번: 기다릴 시간(`wait_until`)이 지난 뒤 온 답은 만료로 닫는다(원본 3.2-2).
//! 기한이 지난 기다림에는 합치지 않는다.

use crate::common::{MIN, T0, ask, buy, env, stop_loss};
use pi_approval::{Answer, Decision, RejectReason, Subject};
use pi_store::{ApprovalStatus, TemplateId, UnixMillis};

// 검증: E8 V45 N2
#[test]
fn approve_after_wait_until_is_closed_as_expired() {
    let e = env();
    let r = e.approvals.request(ask(buy("sig-1"))).unwrap();
    assert_eq!(r.wait_until, Some(UnixMillis(T0 + 5 * MIN)));
    // 기다릴 시간은 지났지만 신호 유효 시간(30분) 안이고, 답 없음 처리는 아직 돌지 않았다.
    e.clock.set(T0 + 6 * MIN);
    let a = e
        .approvals
        .answer(&r.number, r.fingerprint.as_str(), Decision::Approve)
        .unwrap();
    assert_eq!(a, Answer::Rejected(RejectReason::Expired));
    let view = e.approvals.get(&r.number).unwrap().unwrap();
    assert_eq!(view.status, ApprovalStatus::Expired);
    assert!(matches!(
        e.approvals.consume(&r.number, &buy("sig-1")),
        Err(pi_approval::Error::Denied(_))
    ));
    // 닫힌 요청은 답 없음 처리가 다시 돌려주지 않는다.
    let due = e.approvals.expire_due(UnixMillis(T0 + 7 * MIN)).unwrap();
    assert!(due.iter().all(|d| d.number != r.number));
}

// 검증: E8 N2
#[test]
fn answer_exactly_at_wait_until_is_expired() {
    let e = env();
    let r = e.approvals.request(ask(stop_loss("sig-1"))).unwrap();
    assert_eq!(r.wait_until, Some(UnixMillis(T0 + MIN)));
    e.clock.set(T0 + MIN);
    assert_eq!(
        e.approvals
            .answer(&r.number, r.fingerprint.as_str(), Decision::Approve)
            .unwrap(),
        Answer::Rejected(RejectReason::Expired)
    );
    assert_eq!(
        e.approvals.get(&r.number).unwrap().unwrap().status,
        ApprovalStatus::Expired
    );
}

// 검증: E8 N2 C7
#[test]
fn answer_just_before_wait_until_is_approved() {
    let e = env();
    let r = e.approvals.request(ask(stop_loss("sig-1"))).unwrap();
    e.clock.set(T0 + MIN - 1);
    assert_eq!(
        e.approvals
            .answer(&r.number, r.fingerprint.as_str(), Decision::Approve)
            .unwrap(),
        Answer::Approved
    );
}

// 검증: E8 P19
#[test]
fn reject_after_wait_until_is_also_closed_as_expired() {
    let e = env();
    let r = e.approvals.request(ask(buy("sig-1"))).unwrap();
    e.clock.set(T0 + 6 * MIN);
    let a = e
        .approvals
        .answer(&r.number, r.fingerprint.as_str(), Decision::Reject)
        .unwrap();
    assert_eq!(a, Answer::Rejected(RejectReason::Expired));
    assert_eq!(
        e.approvals.get(&r.number).unwrap().unwrap().status,
        ApprovalStatus::Expired
    );
}

// 검증: V42
#[test]
fn subjects_without_wait_until_still_accept_late_answers() {
    let e = env();
    let r = e
        .approvals
        .request(ask(Subject::LivePromotion {
            template: TemplateId(1),
            revision: 1,
        }))
        .unwrap();
    e.clock.set(T0 + 24 * 60 * MIN);
    assert_eq!(
        e.approvals
            .answer(&r.number, r.fingerprint.as_str(), Decision::Approve)
            .unwrap(),
        Answer::Approved
    );
}

// 검증: C12 M5 N2
#[test]
fn request_after_wait_until_is_not_merged() {
    let e = env();
    let a = e.approvals.request(ask(buy("sig-1"))).unwrap();
    e.clock.set(T0 + 4 * MIN);
    let b = e.approvals.request(ask(buy("sig-1"))).unwrap();
    assert_eq!(a.number, b.number);
    assert_eq!(b.repeat, 2);
    e.clock.set(T0 + 5 * MIN);
    let c = e.approvals.request(ask(buy("sig-1"))).unwrap();
    assert_ne!(c.number, a.number);
    assert_eq!(c.repeat, 1);
    assert_eq!(c.wait_until, Some(UnixMillis(T0 + 10 * MIN)));
}
