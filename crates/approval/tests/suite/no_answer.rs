//! 답 없음(E22 · FN2 3b): 신호별 기다릴 시간(N2), 사는 주문은 줄이거나(알림 감) 사지 않음(P20),
//! 파는 신호는 팀 판단, 다시 시작 · 규칙 개선 · 실전 승격은 계속 기다림(V42).

use crate::common::{MIN, T0, ask, buy, env, stop_loss, take_profit};
use pi_approval::{
    Answer, ApprovalNumber, Decision, DueAction, Error, Notice, RejectReason, Subject,
};
use pi_store::{ApprovalStatus, ProposalId, TemplateId, UnixMillis};

// 검증: E22 N2 P20
#[test]
fn buy_without_answer_after_notice_reduces_within_limit() {
    let e = env();
    let s = buy("sig-1");
    let r = e.approvals.request(ask(s.clone())).unwrap();
    e.approvals.record_notice(&r.number, Notice::Sent).unwrap();

    assert!(
        e.approvals
            .expire_due(UnixMillis(T0 + 5 * MIN - 1))
            .unwrap()
            .is_empty()
    );
    let due = e.approvals.expire_due(UnixMillis(T0 + 5 * MIN)).unwrap();
    assert_eq!(due.len(), 1);
    assert_eq!(due[0].number, r.number);
    assert_eq!(due[0].subject, s);
    assert_eq!(due[0].status, ApprovalStatus::NoAnswer);
    assert_eq!(due[0].action, DueAction::ReduceWithinLimit);
    assert_eq!(
        e.approvals.get(&r.number).unwrap().unwrap().status,
        ApprovalStatus::NoAnswer
    );
}

// 검증: E22 P20
#[test]
fn buy_without_answer_after_failed_notice_does_not_buy() {
    let e = env();
    let r = e.approvals.request(ask(buy("sig-1"))).unwrap();
    e.approvals
        .record_notice(&r.number, Notice::Failed)
        .unwrap();
    let due = e.approvals.expire_due(UnixMillis(T0 + 5 * MIN)).unwrap();
    assert_eq!(due.len(), 1);
    assert_eq!(due[0].action, DueAction::DoNotBuy);
}

// 검증: E22 P20 P19
#[test]
fn buy_without_any_notice_record_does_not_buy() {
    let e = env();
    e.approvals.request(ask(buy("sig-1"))).unwrap();
    let due = e.approvals.expire_due(UnixMillis(T0 + 5 * MIN)).unwrap();
    assert_eq!(due.len(), 1);
    assert_eq!(due[0].action, DueAction::DoNotBuy);
}

// 검증: E22 N2
#[test]
fn sell_signals_without_answer_are_left_to_team() {
    let e = env();
    let sl = e.approvals.request(ask(stop_loss("sl"))).unwrap();
    let tp = e.approvals.request(ask(take_profit("tp"))).unwrap();
    e.approvals.record_notice(&sl.number, Notice::Sent).unwrap();
    e.approvals.record_notice(&tp.number, Notice::Sent).unwrap();

    // 손절은 1분.
    assert!(
        e.approvals
            .expire_due(UnixMillis(T0 + MIN - 1))
            .unwrap()
            .is_empty()
    );
    let due = e.approvals.expire_due(UnixMillis(T0 + MIN)).unwrap();
    assert_eq!(due.len(), 1);
    assert_eq!(due[0].number, sl.number);
    assert_eq!(due[0].action, DueAction::TeamDecides);
    assert_eq!(due[0].status, ApprovalStatus::NoAnswer);

    // 익절은 5분.
    let due = e.approvals.expire_due(UnixMillis(T0 + 5 * MIN)).unwrap();
    assert_eq!(due.len(), 1);
    assert_eq!(due[0].number, tp.number);
    assert_eq!(due[0].action, DueAction::TeamDecides);
}

// 검증: E22
#[test]
fn closed_requests_are_not_returned_again() {
    let e = env();
    e.approvals.request(ask(buy("sig-1"))).unwrap();
    assert_eq!(
        e.approvals
            .expire_due(UnixMillis(T0 + 5 * MIN))
            .unwrap()
            .len(),
        1
    );
    assert!(
        e.approvals
            .expire_due(UnixMillis(T0 + 6 * MIN))
            .unwrap()
            .is_empty()
    );
}

// 검증: E22 E8 M5
#[test]
fn no_answer_request_cannot_be_approved_or_used() {
    let e = env();
    let s = buy("sig-1");
    let r = e.approvals.request(ask(s.clone())).unwrap();
    e.approvals.expire_due(UnixMillis(T0 + 5 * MIN)).unwrap();
    e.clock.set(T0 + 5 * MIN + 1);
    let a = e
        .approvals
        .answer(&r.number, r.fingerprint.as_str(), Decision::Approve)
        .unwrap();
    assert_eq!(a, Answer::Rejected(RejectReason::AlreadyResolved));
    assert!(matches!(
        e.approvals.consume(&r.number, &s),
        Err(Error::Denied(RejectReason::NotApproved))
    ));
}

// 검증: E22 E8
#[test]
fn past_signal_validity_is_expired_and_dropped() {
    let e = env();
    let r = e.approvals.request(ask(buy("sig-1"))).unwrap();
    e.approvals.record_notice(&r.number, Notice::Sent).unwrap();
    let due = e.approvals.expire_due(UnixMillis(T0 + 30 * MIN)).unwrap();
    assert_eq!(due.len(), 1);
    assert_eq!(due[0].status, ApprovalStatus::Expired);
    assert_eq!(due[0].action, DueAction::Dropped);
    assert_eq!(
        e.approvals.get(&r.number).unwrap().unwrap().status,
        ApprovalStatus::Expired
    );
}

// 검증: V42 E22
#[test]
fn restart_rule_and_promotion_keep_waiting() {
    let e = env();
    let subjects = [
        Subject::Restart,
        Subject::RuleImprovement {
            proposal: ProposalId(1),
        },
        Subject::LivePromotion {
            template: TemplateId(1),
            revision: 1,
        },
    ];
    let numbers: Vec<_> = subjects
        .iter()
        .map(|s| e.approvals.request(ask(s.clone())).unwrap())
        .collect();

    let far = T0 + 7 * 24 * 60 * MIN;
    assert!(e.approvals.expire_due(UnixMillis(far)).unwrap().is_empty());
    e.clock.set(far);
    for (r, s) in numbers.iter().zip(subjects.iter()) {
        assert_eq!(
            e.approvals.get(&r.number).unwrap().unwrap().status,
            ApprovalStatus::Pending
        );
        // 늦게 와도 사람의 답이 오면 진행된다.
        let a = e
            .approvals
            .answer(&r.number, r.fingerprint.as_str(), Decision::Approve)
            .unwrap();
        assert_eq!(a, Answer::Approved);
        e.approvals.consume(&r.number, s).unwrap();
    }
}

// 검증: P20
#[test]
fn notice_for_unknown_number_is_refused() {
    let e = env();
    let err = e
        .approvals
        .record_notice(&ApprovalNumber::new("b".repeat(32)), Notice::Sent)
        .unwrap_err();
    assert!(
        matches!(err, Error::Denied(RejectReason::NotFound)),
        "{err:?}"
    );
}
