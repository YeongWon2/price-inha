//! 승인을 실행에 쓰기: 한 번만 · 지문 다시 비교 · 다른 대상에 못 씀 · 전체 정지 중 실전 승격 막기.

use std::sync::{Arc, Barrier};

use crate::common::{MIN, T0, ask, buy, env, full_stop, order, owner_restart};
use pi_approval::{
    Answer, ApprovalNumber, Approvals, Decision, Error, RejectReason, Side, SignalKind, Subject,
    WaitTimes,
};
use pi_store::{ApprovalStatus, ProposalId, TemplateId};

fn approved(approvals: &Approvals, subject: &Subject) -> ApprovalNumber {
    let r = approvals.request(ask(subject.clone())).unwrap();
    let a = approvals
        .answer(&r.number, r.fingerprint.as_str(), Decision::Approve)
        .unwrap();
    assert_eq!(a, Answer::Approved);
    r.number
}

fn denied(result: Result<(), Error>) -> RejectReason {
    match result {
        Err(Error::Denied(reason)) => reason,
        other => panic!("Denied가 아니다: {other:?}"),
    }
}

// 검증: V44 M5
#[test]
fn approved_same_subject_passes_once() {
    let e = env();
    let s = buy("sig-1");
    let n = approved(&e.approvals, &s);
    e.approvals.consume(&n, &s).unwrap();
    let view = e.approvals.get(&n).unwrap().unwrap();
    assert!(view.consumed);
    assert_eq!(view.status, ApprovalStatus::Approved);
    assert_eq!(
        denied(e.approvals.consume(&n, &s)),
        RejectReason::AlreadyConsumed
    );
}

// 검증: V42
#[test]
fn pending_request_does_not_proceed() {
    let e = env();
    for subject in [
        Subject::Restart,
        Subject::RuleImprovement {
            proposal: ProposalId(4),
        },
        Subject::LivePromotion {
            template: TemplateId(4),
            revision: 1,
        },
        buy("sig-1"),
    ] {
        let r = e.approvals.request(ask(subject.clone())).unwrap();
        assert_eq!(
            denied(e.approvals.consume(&r.number, &subject)),
            RejectReason::NotApproved
        );
        e.approvals
            .answer(&r.number, r.fingerprint.as_str(), Decision::Approve)
            .unwrap();
        e.approvals.consume(&r.number, &subject).unwrap();
    }
}

// 검증: E7 V44
#[test]
fn changed_order_at_execution_is_refused_and_voided() {
    let e = env();
    let s = buy("sig-1");
    let n = approved(&e.approvals, &s);
    let mut changed = order("sig-1", Side::Buy, SignalKind::Buy);
    changed.quantity = 20;
    assert_eq!(
        denied(e.approvals.consume(&n, &Subject::Order(changed))),
        RejectReason::FingerprintMismatch
    );
    // 거절로 바뀐 것으로 다룬다: 원래 내용으로도 이제 쓸 수 없다.
    assert!(matches!(e.approvals.consume(&n, &s), Err(Error::Denied(_))));
}

// 검증: E7 M5
#[test]
fn approval_for_one_target_cannot_be_used_for_another() {
    let e = env();
    let promo1 = Subject::LivePromotion {
        template: TemplateId(1),
        revision: 1,
    };
    let n = approved(&e.approvals, &promo1);
    for other in [
        Subject::LivePromotion {
            template: TemplateId(2),
            revision: 1,
        },
        Subject::RuleImprovement {
            proposal: ProposalId(1),
        },
        Subject::Restart,
        buy("sig-1"),
    ] {
        assert_eq!(
            denied(e.approvals.consume(&n, &other)),
            RejectReason::FingerprintMismatch,
            "{other:?}"
        );
    }

    let e = env();
    let n = approved(&e.approvals, &buy("sig-1"));
    assert_eq!(
        denied(e.approvals.consume(&n, &promo1)),
        RejectReason::FingerprintMismatch
    );
}

// 검증: E8
#[test]
fn unknown_number_cannot_be_consumed() {
    let e = env();
    assert_eq!(
        denied(
            e.approvals
                .consume(&ApprovalNumber::new("a".repeat(32)), &buy("x"))
        ),
        RejectReason::NotFound
    );
}

// 검증: E8 M5
#[test]
fn approval_past_signal_validity_cannot_be_consumed() {
    let e = env();
    let s = buy("sig-1");
    let n = approved(&e.approvals, &s);
    e.clock.set(T0 + 30 * MIN);
    assert_eq!(denied(e.approvals.consume(&n, &s)), RejectReason::Expired);
}

// 검증: V44 M5
#[test]
fn concurrent_consume_passes_only_once() {
    let e = env();
    let s = buy("sig-1");
    let n = approved(&e.approvals, &s);
    let threads = 8;
    let barrier = Barrier::new(threads);
    let oks = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..threads)
            .map(|_| {
                let approvals = Arc::clone(&e.approvals);
                let barrier = &barrier;
                let n = n.clone();
                let s = s.clone();
                scope.spawn(move || {
                    barrier.wait();
                    approvals.consume(&n, &s).is_ok()
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|h| h.join().unwrap())
            .filter(|ok| *ok)
            .count()
    });
    assert_eq!(oks, 1);
}

// 검증: V44 M5
#[test]
fn concurrent_consume_from_two_managers_passes_only_once() {
    let e = env();
    let s = buy("sig-1");
    let n = approved(&e.approvals, &s);
    let second = Approvals::new(e.store.clone(), e.clock.clone(), WaitTimes::default());
    let barrier = Barrier::new(2);
    let (a, b) = std::thread::scope(|scope| {
        let h1 = scope.spawn(|| {
            barrier.wait();
            e.approvals.consume(&n, &s).is_ok()
        });
        let h2 = scope.spawn(|| {
            barrier.wait();
            second.consume(&n, &s).is_ok()
        });
        (h1.join().unwrap(), h2.join().unwrap())
    });
    assert!(a ^ b, "한 번만 통과해야 한다: {a} {b}");
}

// 검증: P18 V42
#[test]
fn live_promotion_is_not_consumed_during_full_stop() {
    let e = env();
    let promo = Subject::LivePromotion {
        template: TemplateId(1),
        revision: 1,
    };
    let n = approved(&e.approvals, &promo);
    full_stop(&e.store);
    assert_eq!(
        denied(e.approvals.consume(&n, &promo)),
        RejectReason::FullStop
    );
    // 사람이 다시 시작한 뒤에는 그 승인을 쓸 수 있다(막힌 시도는 소비하지 않았다).
    owner_restart(&e.store);
    e.approvals.consume(&n, &promo).unwrap();
}

// 검증: V42 P18
#[test]
fn restart_and_rule_improvement_work_during_full_stop() {
    let e = env();
    full_stop(&e.store);
    let n = approved(&e.approvals, &Subject::Restart);
    e.approvals.consume(&n, &Subject::Restart).unwrap();
    let rule = Subject::RuleImprovement {
        proposal: ProposalId(2),
    };
    let n = approved(&e.approvals, &rule);
    e.approvals.consume(&n, &rule).unwrap();
}
