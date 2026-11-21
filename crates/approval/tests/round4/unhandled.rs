//! `unhandled_due`: 처리 기록이 없는 답 없음 · 만료 주문 요청을 닫힌 순서로, 읽기만 하고,
//! 다시 켠 뒤에도 같게 돌려준다. 신호 유효 시간이 지나면 `Dropped`(결정 6번과 같은 생각).

use crate::common::{
    MIN, T0, VALID_UNTIL, buy, close_due_at, env, reopen, requested, stop_loss, take_profit,
};
use pi_approval::{DueAction, DueHandling, Notice};
use pi_store::{ApprovalStatus, UnixMillis};

// 검증: E22 M5
#[test]
fn unhandled_survives_restart_until_recorded() {
    let e = env();
    let r = requested(&e, buy("sig-1"), Some(Notice::Sent));
    let due = close_due_at(&e, T0 + 5 * MIN);
    assert_eq!(due.len(), 1);

    // 기록 전에 다시 켜도 같은 것을 준다.
    let again = reopen(&e.path, e.clock.clone());
    let left = again.unhandled_due().unwrap();
    assert_eq!(left, e.approvals.unhandled_due().unwrap());
    assert_eq!(left.len(), 1);
    assert_eq!(left[0].number, r.number);
    assert_eq!(left[0].subject, due[0].subject);
    assert_eq!(left[0].status, ApprovalStatus::NoAnswer);
    assert_eq!(left[0].action, DueAction::ReduceWithinLimit);

    // 다시 켠 쪽에서 기록한다.
    again
        .record_due_handling(&r.number, DueHandling::Reduced { quantity: 4 })
        .unwrap();
    drop(again);

    // 기록 뒤에 다시 켜면 비어 있고 기록이 남아 있다.
    let third = reopen(&e.path, e.clock.clone());
    assert!(third.unhandled_due().unwrap().is_empty());
    assert_eq!(
        third.due_handling(&r.number).unwrap(),
        Some(DueHandling::Reduced { quantity: 4 })
    );
    // 다시 켠 뒤에도 한 번만 남는다.
    assert!(matches!(
        third.record_due_handling(&r.number, DueHandling::Dropped),
        Err(pi_approval::Error::AlreadyHandled)
    ));
}

// 검증: E22 M5
#[test]
fn restart_with_on_restart_keeps_unhandled_within_validity() {
    let e = env();
    let r = requested(&e, stop_loss("sl"), Some(Notice::Sent));
    close_due_at(&e, T0 + MIN);
    // 유효 시간 안에 다시 켠다: on_restart는 닫힌 요청을 건드리지 않는다.
    let again = reopen(&e.path, e.clock.clone());
    assert!(again.on_restart().unwrap().is_empty());
    let left = again.unhandled_due().unwrap();
    assert_eq!(left.len(), 1);
    assert_eq!(left[0].number, r.number);
    assert_eq!(left[0].action, DueAction::TeamDecides);
}

// 검증: E22 N2 P20 M5
#[test]
fn after_signal_validity_unhandled_actions_become_dropped() {
    let e = env();
    let b = requested(&e, buy("sig-b"), Some(Notice::Sent));
    let s = requested(&e, take_profit("sig-s"), Some(Notice::Sent));
    let f = requested(&e, buy("sig-f"), Some(Notice::Failed));
    let due = close_due_at(&e, T0 + 5 * MIN);
    assert_eq!(due.len(), 3);

    // 경계: 유효 시간 끝 1ms 전에는 원래 할 일.
    e.clock.set(VALID_UNTIL - 1);
    let left = e.approvals.unhandled_due().unwrap();
    let action_of = |list: &[pi_approval::DueOutcome], n| {
        list.iter()
            .find(|d| &d.number == n)
            .map(|d| d.action)
            .unwrap()
    };
    assert_eq!(action_of(&left, &b.number), DueAction::ReduceWithinLimit);
    assert_eq!(action_of(&left, &s.number), DueAction::TeamDecides);
    assert_eq!(action_of(&left, &f.number), DueAction::DoNotBuy);

    // 꺼졌다 켜진 사이 유효 시간이 지났다: 줄여서 사거나 팀이 팔지 않는다.
    e.clock.set(VALID_UNTIL);
    let again = reopen(&e.path, e.clock.clone());
    let left = again.unhandled_due().unwrap();
    assert_eq!(left.len(), 3);
    assert!(
        left.iter().all(|d| d.action == DueAction::Dropped),
        "{left:?}"
    );
    // 결과는 바뀌지 않는다(읽기만 한다).
    assert!(left.iter().all(|d| d.status == ApprovalStatus::NoAnswer));
    assert_eq!(
        again.get(&b.number).unwrap().unwrap().status,
        ApprovalStatus::NoAnswer
    );

    // Dropped로 기록할 수 있다.
    again
        .record_due_handling(&b.number, DueHandling::Dropped)
        .unwrap();
    again
        .record_due_handling(&s.number, DueHandling::Dropped)
        .unwrap();
    let left = again.unhandled_due().unwrap();
    assert_eq!(left.len(), 1);
    assert_eq!(left[0].number, f.number);
    assert_eq!(left[0].action, DueAction::Dropped);
}

// 검증: E22 M5
#[test]
fn order_expired_by_restart_is_unhandled_and_dropped() {
    let e = env();
    let r = requested(&e, buy("sig-1"), Some(Notice::Sent));
    // 기다리는 사이 꺼졌다가 신호 유효 시간이 지난 뒤 켰다.
    e.clock.set(VALID_UNTIL + MIN);
    let again = reopen(&e.path, e.clock.clone());
    assert_eq!(again.on_restart().unwrap(), vec![r.number.clone()]);
    let left = again.unhandled_due().unwrap();
    assert_eq!(left.len(), 1);
    assert_eq!(left[0].number, r.number);
    assert_eq!(left[0].status, ApprovalStatus::Expired);
    assert_eq!(left[0].action, DueAction::Dropped);
    again
        .record_due_handling(&r.number, DueHandling::Dropped)
        .unwrap();
    assert!(again.unhandled_due().unwrap().is_empty());
}

// 검증: E22
#[test]
fn unhandled_is_in_closing_order() {
    let e = env();
    // 사기를 먼저 요청했지만(기다림 5분) 손절(기다림 1분)이 먼저 닫힌다.
    let b = requested(&e, buy("sig-b"), Some(Notice::Sent));
    e.clock.set(T0 + MIN);
    let s = requested(&e, stop_loss("sig-s"), Some(Notice::Sent));
    let first = close_due_at(&e, T0 + 2 * MIN);
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].number, s.number);
    let second = close_due_at(&e, T0 + 5 * MIN);
    assert_eq!(second.len(), 1);
    assert_eq!(second[0].number, b.number);

    let left = e.approvals.unhandled_due().unwrap();
    let numbers: Vec<_> = left.iter().map(|d| d.number.clone()).collect();
    assert_eq!(numbers, vec![s.number.clone(), b.number.clone()]);
    assert_eq!(left[0].action, DueAction::TeamDecides);
    assert_eq!(left[1].action, DueAction::ReduceWithinLimit);
}

// 검증: E22 V42
#[test]
fn unhandled_only_reads_and_skips_open_requests() {
    let e = env();
    let closed = requested(&e, buy("sig-c"), Some(Notice::Sent));
    close_due_at(&e, T0 + 5 * MIN);
    // 기한은 지났지만 아직 expire_due가 닫지 않은 기다림.
    let open = requested(&e, stop_loss("sig-o"), Some(Notice::Sent));
    e.clock.set(T0 + 10 * MIN);

    let first = e.approvals.unhandled_due().unwrap();
    let second = e.approvals.unhandled_due().unwrap();
    assert_eq!(first, second);
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].number, closed.number);
    assert_eq!(
        e.approvals.get(&open.number).unwrap().unwrap().status,
        ApprovalStatus::Pending
    );
    assert_eq!(e.approvals.due_handling(&closed.number).unwrap(), None);

    // expire_due의 반환은 그대로다: 이미 닫은 요청은 다시 돌려주지 않고, 남은 기다림만 닫는다.
    let due = e.approvals.expire_due(UnixMillis(T0 + 10 * MIN)).unwrap();
    assert_eq!(due.len(), 1);
    assert_eq!(due[0].number, open.number);
    assert_eq!(due[0].action, DueAction::TeamDecides);
    assert_eq!(e.approvals.unhandled_due().unwrap().len(), 2);
}
