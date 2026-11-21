//! `record_due_handling` · `due_handling`(E22 "보이는 것"): 답 없음 · 만료로 닫힌 주문 요청만,
//! 할 일과 맞는 기록만, 한 번만 남는다.

use crate::common::{MIN, QTY, T0, ask, buy, close_due_at, env, requested, stop_loss, take_profit};
use pi_approval::{
    ApprovalNumber, Decision, DueAction, DueHandling, Error, Notice, RejectReason, Subject,
};
use pi_store::{ApprovalStatus, ProposalId, TemplateId, UnixMillis};

fn blocked() -> DueHandling {
    DueHandling::Blocked {
        reason: "다시 점검: 하루 손실 한도에 걸림".to_string(),
    }
}

// 검증: E22 N2 P20
#[test]
fn reduce_flow_records_once_and_leaves_unhandled_empty() {
    let e = env();
    let r = requested(&e, buy("sig-1"), Some(Notice::Sent));
    let due = close_due_at(&e, T0 + 5 * MIN);
    assert_eq!(due.len(), 1);
    assert_eq!(due[0].action, DueAction::ReduceWithinLimit);

    let left = e.approvals.unhandled_due().unwrap();
    assert_eq!(left.len(), 1);
    assert_eq!(left[0].number, r.number);
    assert_eq!(left[0].action, DueAction::ReduceWithinLimit);
    assert_eq!(left[0].status, ApprovalStatus::NoAnswer);
    assert_eq!(e.approvals.due_handling(&r.number).unwrap(), None);

    e.approvals
        .record_due_handling(&r.number, DueHandling::Reduced { quantity: 3 })
        .unwrap();
    assert!(e.approvals.unhandled_due().unwrap().is_empty());
    assert_eq!(
        e.approvals.due_handling(&r.number).unwrap(),
        Some(DueHandling::Reduced { quantity: 3 })
    );

    // 두 번째 기록은 AlreadyHandled이고 처음 기록이 그대로다.
    let err = e
        .approvals
        .record_due_handling(&r.number, DueHandling::Dropped)
        .unwrap_err();
    assert!(matches!(err, Error::AlreadyHandled), "{err:?}");
    let err = e
        .approvals
        .record_due_handling(&r.number, DueHandling::Reduced { quantity: 3 })
        .unwrap_err();
    assert!(matches!(err, Error::AlreadyHandled), "{err:?}");
    assert_eq!(
        e.approvals.due_handling(&r.number).unwrap(),
        Some(DueHandling::Reduced { quantity: 3 })
    );
}

// 검증: E22 P20
#[test]
fn reduce_action_also_accepts_blocked_and_dropped() {
    let e = env();
    let a = requested(&e, buy("sig-a"), Some(Notice::Sent));
    let b = requested(&e, buy("sig-b"), Some(Notice::Sent));
    let due = close_due_at(&e, T0 + 5 * MIN);
    assert_eq!(due.len(), 2);

    e.approvals
        .record_due_handling(&a.number, blocked())
        .unwrap();
    e.approvals
        .record_due_handling(&b.number, DueHandling::Dropped)
        .unwrap();
    assert_eq!(
        e.approvals.due_handling(&a.number).unwrap(),
        Some(blocked())
    );
    assert_eq!(
        e.approvals.due_handling(&b.number).unwrap(),
        Some(DueHandling::Dropped)
    );
    assert!(e.approvals.unhandled_due().unwrap().is_empty());
}

// 검증: E22 P20
#[test]
fn reduce_action_refuses_not_bought_and_team_choices() {
    let e = env();
    let r = requested(&e, buy("sig-1"), Some(Notice::Sent));
    close_due_at(&e, T0 + 5 * MIN);
    for wrong in [
        DueHandling::NotBought,
        DueHandling::TeamSold,
        DueHandling::TeamKept,
    ] {
        let err = e
            .approvals
            .record_due_handling(&r.number, wrong.clone())
            .unwrap_err();
        assert!(matches!(err, Error::NotDue), "{wrong:?}: {err:?}");
    }
    // 거부된 기록은 남지 않는다.
    assert_eq!(e.approvals.due_handling(&r.number).unwrap(), None);
    assert_eq!(e.approvals.unhandled_due().unwrap().len(), 1);
}

// 검증: E22 P20
#[test]
fn do_not_buy_accepts_only_not_bought() {
    let e = env();
    // 알림 실패와 알림 기록 없음 둘 다 DoNotBuy.
    let failed = requested(&e, buy("sig-f"), Some(Notice::Failed));
    let silent = requested(&e, buy("sig-s"), None);
    let due = close_due_at(&e, T0 + 5 * MIN);
    assert!(due.iter().all(|d| d.action == DueAction::DoNotBuy));

    for wrong in [
        DueHandling::Reduced { quantity: 3 },
        DueHandling::TeamSold,
        DueHandling::TeamKept,
        blocked(),
        DueHandling::Dropped,
    ] {
        let err = e
            .approvals
            .record_due_handling(&failed.number, wrong.clone())
            .unwrap_err();
        assert!(matches!(err, Error::NotDue), "{wrong:?}: {err:?}");
    }
    assert_eq!(e.approvals.due_handling(&failed.number).unwrap(), None);

    e.approvals
        .record_due_handling(&failed.number, DueHandling::NotBought)
        .unwrap();
    e.approvals
        .record_due_handling(&silent.number, DueHandling::NotBought)
        .unwrap();
    assert_eq!(
        e.approvals.due_handling(&failed.number).unwrap(),
        Some(DueHandling::NotBought)
    );
    assert_eq!(
        e.approvals.due_handling(&silent.number).unwrap(),
        Some(DueHandling::NotBought)
    );
}

// 검증: E22 N2
#[test]
fn team_decides_accepts_team_choices_blocked_and_dropped() {
    let e = env();
    let subjects = [
        stop_loss("sl-1"),
        stop_loss("sl-2"),
        take_profit("tp-1"),
        take_profit("tp-2"),
    ];
    let rs: Vec<_> = subjects
        .iter()
        .map(|s| requested(&e, s.clone(), Some(Notice::Sent)))
        .collect();
    let due = close_due_at(&e, T0 + 5 * MIN);
    assert_eq!(due.len(), 4);
    assert!(due.iter().all(|d| d.action == DueAction::TeamDecides));

    // 팔기에 사기 쪽 기록은 맞지 않는다.
    for wrong in [DueHandling::NotBought, DueHandling::Reduced { quantity: 3 }] {
        let err = e
            .approvals
            .record_due_handling(&rs[0].number, wrong.clone())
            .unwrap_err();
        assert!(matches!(err, Error::NotDue), "{wrong:?}: {err:?}");
    }

    let handlings = [
        DueHandling::TeamSold,
        DueHandling::TeamKept,
        blocked(),
        DueHandling::Dropped,
    ];
    for (r, h) in rs.iter().zip(handlings.iter()) {
        e.approvals
            .record_due_handling(&r.number, h.clone())
            .unwrap();
    }
    for (r, h) in rs.iter().zip(handlings.iter()) {
        assert_eq!(
            e.approvals.due_handling(&r.number).unwrap(),
            Some(h.clone())
        );
    }
    assert!(e.approvals.unhandled_due().unwrap().is_empty());
}

// 검증: E22 E8
#[test]
fn expired_order_accepts_only_dropped() {
    let e = env();
    let b = requested(&e, buy("sig-b"), Some(Notice::Sent));
    let s = requested(&e, stop_loss("sig-s"), Some(Notice::Sent));
    let due = close_due_at(&e, T0 + 30 * MIN);
    assert_eq!(due.len(), 2);
    assert!(due.iter().all(|d| d.status == ApprovalStatus::Expired));
    assert!(due.iter().all(|d| d.action == DueAction::Dropped));

    for r in [&b, &s] {
        for wrong in [
            DueHandling::Reduced { quantity: 3 },
            DueHandling::NotBought,
            DueHandling::TeamSold,
            DueHandling::TeamKept,
            blocked(),
        ] {
            let err = e
                .approvals
                .record_due_handling(&r.number, wrong.clone())
                .unwrap_err();
            assert!(matches!(err, Error::NotDue), "{wrong:?}: {err:?}");
        }
        e.approvals
            .record_due_handling(&r.number, DueHandling::Dropped)
            .unwrap();
        assert_eq!(
            e.approvals.due_handling(&r.number).unwrap(),
            Some(DueHandling::Dropped)
        );
    }
}

// 검증: E22 P20
#[test]
fn reduced_quantity_must_be_positive_and_within_request() {
    let e = env();
    let r = requested(&e, buy("sig-1"), Some(Notice::Sent));
    close_due_at(&e, T0 + 5 * MIN);
    for wrong in [0, QTY + 1, u64::MAX] {
        let err = e
            .approvals
            .record_due_handling(&r.number, DueHandling::Reduced { quantity: wrong })
            .unwrap_err();
        assert!(matches!(err, Error::NotDue), "{wrong}: {err:?}");
    }
    assert_eq!(e.approvals.due_handling(&r.number).unwrap(), None);
    // 경계: 요청 수량과 같으면 받는다.
    e.approvals
        .record_due_handling(&r.number, DueHandling::Reduced { quantity: QTY })
        .unwrap();
    assert_eq!(
        e.approvals.due_handling(&r.number).unwrap(),
        Some(DueHandling::Reduced { quantity: QTY })
    );
}

// 검증: E22 V42 V43 C7
#[test]
fn open_approved_and_rejected_requests_are_not_due() {
    let e = env();
    let pending = requested(&e, buy("sig-p"), Some(Notice::Sent));
    let approved = requested(&e, buy("sig-a"), Some(Notice::Sent));
    let rejected = requested(&e, buy("sig-r"), Some(Notice::Sent));
    e.approvals
        .answer(
            &approved.number,
            approved.fingerprint.as_str(),
            Decision::Approve,
        )
        .unwrap();
    e.approvals
        .answer(
            &rejected.number,
            rejected.fingerprint.as_str(),
            Decision::Reject,
        )
        .unwrap();

    for r in [&pending, &approved, &rejected] {
        for h in [
            DueHandling::Reduced { quantity: 3 },
            DueHandling::NotBought,
            DueHandling::Dropped,
        ] {
            let err = e
                .approvals
                .record_due_handling(&r.number, h.clone())
                .unwrap_err();
            assert!(matches!(err, Error::NotDue), "{h:?}: {err:?}");
        }
        assert_eq!(e.approvals.due_handling(&r.number).unwrap(), None);
    }
    assert!(e.approvals.unhandled_due().unwrap().is_empty());
    assert_eq!(
        e.approvals.get(&pending.number).unwrap().unwrap().status,
        ApprovalStatus::Pending
    );
}

// 검증: E22 V42
#[test]
fn restart_rule_and_promotion_requests_are_not_due() {
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
    let rs: Vec<_> = subjects
        .iter()
        .map(|s| e.approvals.request(ask(s.clone())).unwrap())
        .collect();
    // 오래 지나도 계속 기다린다(V42).
    let far = T0 + 7 * 24 * 60 * MIN;
    assert!(close_due_at(&e, far).is_empty());
    for r in &rs {
        for h in [DueHandling::Dropped, DueHandling::NotBought, blocked()] {
            let err = e
                .approvals
                .record_due_handling(&r.number, h.clone())
                .unwrap_err();
            assert!(matches!(err, Error::NotDue), "{h:?}: {err:?}");
        }
        assert_eq!(
            e.approvals.get(&r.number).unwrap().unwrap().status,
            ApprovalStatus::Pending
        );
    }
    assert!(e.approvals.unhandled_due().unwrap().is_empty());
}

// 검증: E22 E8
#[test]
fn unknown_number_is_not_due() {
    let e = env();
    let err = e
        .approvals
        .record_due_handling(&ApprovalNumber::new("c".repeat(32)), DueHandling::Dropped)
        .unwrap_err();
    assert!(matches!(err, Error::NotDue), "{err:?}");
}

// 검증: E22 M5 V44 P19
#[test]
fn handled_no_answer_request_still_cannot_be_used_or_reclosed() {
    let e = env();
    let s = buy("sig-1");
    let r = requested(&e, s.clone(), Some(Notice::Sent));
    close_due_at(&e, T0 + 5 * MIN);
    e.approvals
        .record_due_handling(&r.number, DueHandling::Reduced { quantity: 3 })
        .unwrap();

    // 기록은 새 승인 요청을 만들지 않는다: 신호 유효 시간 끝 직전까지 다시 닫을 기다림이 없다.
    assert!(
        e.approvals
            .expire_due(UnixMillis(T0 + 29 * MIN))
            .unwrap()
            .is_empty()
    );
    // 답 없음으로 닫힌 요청은 여전히 쓸 수 없다.
    assert!(matches!(
        e.approvals.consume(&r.number, &s),
        Err(Error::Denied(RejectReason::NotApproved))
    ));
    let view = e.approvals.get(&r.number).unwrap().unwrap();
    assert_eq!(view.status, ApprovalStatus::NoAnswer);
    assert!(!view.consumed);
}
