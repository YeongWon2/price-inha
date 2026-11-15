//! 다시 켠 뒤(결정 6번 · M5): 신호 유효 시간이 지난 기다림은 만료, 안이면 유지.

mod common;

use common::{FakeClock, MIN, T0, ask, buy, env, order, reopen};
use pi_approval::{Answer, Decision, Error, RejectReason, Side, SignalKind, Subject};
use pi_store::{ApprovalStatus, ProposalId, UnixMillis};

// 검증: M5 E8
#[test]
fn reopen_keeps_valid_and_expires_stale_requests() {
    let e = env();
    let keep = buy("keep");
    let mut short = order("short", Side::Sell, SignalKind::StopLoss);
    short.signal_valid_until = UnixMillis(T0 + 2 * MIN);
    let short = Subject::Order(short);
    let rule = Subject::RuleImprovement {
        proposal: ProposalId(9),
    };

    let r_keep = e.approvals.request(ask(keep.clone())).unwrap();
    let r_short = e.approvals.request(ask(short)).unwrap();
    let r_rule = e.approvals.request(ask(rule.clone())).unwrap();
    let path = e.path.clone();
    drop(e.approvals);
    drop(e.store);

    let clock = FakeClock::at(T0 + 3 * MIN);
    let (_store, approvals) = reopen(&path, clock.clone());
    let expired = approvals.on_restart().unwrap();
    assert_eq!(expired, vec![r_short.number.clone()]);

    assert_eq!(
        approvals.get(&r_short.number).unwrap().unwrap().status,
        ApprovalStatus::Expired
    );
    assert_eq!(
        approvals.get(&r_keep.number).unwrap().unwrap().status,
        ApprovalStatus::Pending
    );
    assert_eq!(
        approvals.get(&r_rule.number).unwrap().unwrap().status,
        ApprovalStatus::Pending
    );

    // 유지된 요청은 다시 켠 뒤에도 같은 번호 · 지문으로 답하고 쓸 수 있다.
    let a = approvals
        .answer(
            &r_keep.number,
            r_keep.fingerprint.as_str(),
            Decision::Approve,
        )
        .unwrap();
    assert_eq!(a, Answer::Approved);
    approvals.consume(&r_keep.number, &keep).unwrap();

    // 만료된 요청은 답해도 거절로 남는다.
    let a = approvals
        .answer(
            &r_short.number,
            r_short.fingerprint.as_str(),
            Decision::Approve,
        )
        .unwrap();
    assert_eq!(a, Answer::Rejected(RejectReason::AlreadyResolved));
}

// 검증: M5 V45
#[test]
fn consumed_approval_stays_consumed_after_reopen() {
    let e = env();
    let s = buy("sig-1");
    let r = e.approvals.request(ask(s.clone())).unwrap();
    e.approvals
        .answer(&r.number, r.fingerprint.as_str(), Decision::Approve)
        .unwrap();
    e.approvals.consume(&r.number, &s).unwrap();
    let path = e.path.clone();
    drop(e.approvals);
    drop(e.store);

    let (_store, approvals) = reopen(&path, FakeClock::at(T0 + MIN));
    approvals.on_restart().unwrap();
    let view = approvals.get(&r.number).unwrap().unwrap();
    assert!(view.consumed);
    assert!(matches!(
        approvals.consume(&r.number, &s),
        Err(Error::Denied(RejectReason::AlreadyConsumed))
    ));
}

// 검증: M5 C12
#[test]
fn numbers_after_reopen_are_new() {
    let e = env();
    let first = e.approvals.request(ask(buy("a"))).unwrap();
    let path = e.path.clone();
    drop(e.approvals);
    drop(e.store);

    let (_store, approvals) = reopen(&path, FakeClock::at(T0 + MIN));
    let second = approvals.request(ask(buy("b"))).unwrap();
    assert_ne!(first.number, second.number);
    assert!(approvals.get(&first.number).unwrap().is_some());
}
