//! 3차: 개선안 승인을 journal 판정에 맡긴다(`with_proposal_approval`, 실제 임시 저장소 · 실제 journal).
//!
//! 승인 확인과 소비 범위는 이 크레이트가, 반영 판정(지금 규칙과 비교한 위험 재판정 → 냉각)은 journal이 한다.

use std::cell::Cell;
use std::sync::Arc;
use std::thread;

use crate::common::{T0, ask, env, full_stop, proposed};
use pi_approval::{
    Answer, ApprovalNumber, Approvals, Decision, Error, RejectReason, Subject, fingerprint,
};
use pi_journal::{Improvement, Journal, Mistake, RuleOutcome};
use pi_store::{ApprovalStatus, NewProposal, ProposalId, ProposalState, Store, UnixMillis};
use pi_template::{RiskParams, Style, preset};
use rust_decimal::Decimal;

/// 시험용 오류: 이 크레이트의 오류와 journal의 오류를 함께 담는다.
#[derive(Debug)]
enum Fail {
    Approval(Error),
    Journal(pi_journal::Error),
}

impl From<Error> for Fail {
    fn from(err: Error) -> Fail {
        Fail::Approval(err)
    }
}

impl From<pi_journal::Error> for Fail {
    fn from(err: pi_journal::Error) -> Fail {
        Fail::Journal(err)
    }
}

fn current() -> RiskParams {
    preset(Style::DayTrade)
}

/// 일일 손실 한도를 바꾼 개선안을 `제안됨`으로 저장한다. 저장된 `increases_risk`는 일부러 거짓이다
/// (제안 때가 아니라 승인할 때의 규칙으로 다시 판정하는지 본다).
fn improvement(store: &Store, daily_loss_limit_pct: i64) -> ProposalId {
    let base = proposed(store, false);
    let evidence = store.get_proposal(base).unwrap().evidence;
    let mut after = current();
    after.daily_loss_limit_pct = Decimal::new(daily_loss_limit_pct, 0);
    let content = serde_json::to_value(Improvement {
        mistake: Mistake::PanicExit,
        before: current(),
        after,
    })
    .unwrap();
    store
        .create_proposal(NewProposal {
            content,
            evidence,
            increases_risk: false,
            at: UnixMillis(T0 - 40),
        })
        .unwrap()
        .id
}

/// 위험을 키우는 개선안(한도 1.5% → 3%).
fn widening(store: &Store) -> ProposalId {
    improvement(store, 3)
}

/// 위험을 키우지 않는 개선안(한도 1.5% → 1%).
fn tightening(store: &Store) -> ProposalId {
    improvement(store, 1)
}

fn subject(proposal: ProposalId) -> Subject {
    Subject::RuleImprovement { proposal }
}

fn approved(approvals: &Approvals, proposal: ProposalId) -> ApprovalNumber {
    let r = approvals.request(ask(subject(proposal))).unwrap();
    let a = approvals
        .answer(&r.number, r.fingerprint.as_str(), Decision::Approve)
        .unwrap();
    assert_eq!(a, Answer::Approved);
    r.number
}

/// 불리면 journal에 승인을 넘겨 개선안을 승인한다.
fn through_journal(
    approvals: &Approvals,
    journal: &Journal,
    number: &ApprovalNumber,
    proposal: ProposalId,
) -> Result<RuleOutcome, Fail> {
    approvals.with_proposal_approval(number, proposal, |using, at| {
        Ok(journal.approve(proposal, Some(using), &current(), at)?)
    })
}

fn denied<T: std::fmt::Debug>(result: Result<T, Fail>) -> RejectReason {
    match result {
        Err(Fail::Approval(Error::Denied(reason))) => reason,
        other => panic!("Denied가 아니다: {other:?}"),
    }
}

/// `f`가 불렸는지 세면서 부른다.
fn counted(
    approvals: &Approvals,
    number: &ApprovalNumber,
    proposal: ProposalId,
) -> (Result<(), Fail>, u32) {
    let calls = Cell::new(0);
    let result = approvals.with_proposal_approval(number, proposal, |_, _| {
        calls.set(calls.get() + 1);
        Ok::<(), Fail>(())
    });
    (result, calls.get())
}

// 검증: P18 V44 E7
#[test]
fn f_gets_the_approval_of_this_proposal_with_the_clock_time() {
    let e = env();
    let p = widening(&e.store);
    let n = approved(&e.approvals, p);
    let view = e.approvals.get(&n).unwrap().unwrap();
    e.clock.set(T0 + 1234);
    let seen = e
        .approvals
        .with_proposal_approval(&n, p, |using, at| Ok::<_, Fail>((using, at)))
        .unwrap();
    assert_eq!(seen.0.fingerprint, fingerprint(&subject(p)).as_str());
    assert_eq!(seen.0.id, e.store.get_approval(seen.0.id).unwrap().id);
    assert_eq!(seen.1, UnixMillis(T0 + 1234));
    // f가 승인을 쓰지 않았으니 저장소에서는 아직 소비되지 않았다(소비는 f가 부르는 쪽이 한다).
    assert!(!e.approvals.get(&n).unwrap().unwrap().consumed);
    assert_eq!(view.status, ApprovalStatus::Approved);
}

// 검증: E28 M5 V44
#[test]
fn risky_proposal_becomes_cooling_through_journal_judgment() {
    let e = env();
    let journal = Journal::new(e.store.clone());
    let p = widening(&e.store);
    let n = approved(&e.approvals, p);
    let out = through_journal(&e.approvals, &journal, &n, p).unwrap();
    assert_eq!(out.proposal.state, ProposalState::Cooling);
    assert_eq!(out.rules, current());
    // 제안 때 안전했던 개선안은 journal이 위험 표시를 단 같은 내용의 개선안으로 바꿔 냉각에 올린다.
    assert_eq!(
        e.store.get_proposal(out.proposal.id).unwrap().state,
        ProposalState::Cooling
    );
    assert!(e.approvals.get(&n).unwrap().unwrap().consumed);
}

// 검증: V44 M5
#[test]
fn safe_proposal_is_applied_through_journal_judgment() {
    let e = env();
    let journal = Journal::new(e.store.clone());
    let p = tightening(&e.store);
    let n = approved(&e.approvals, p);
    let out = through_journal(&e.approvals, &journal, &n, p).unwrap();
    assert_eq!(out.proposal.state, ProposalState::Applied);
    assert_eq!(out.rules.daily_loss_limit_pct, Decimal::new(1, 0));
    assert!(e.approvals.get(&n).unwrap().unwrap().consumed);
}

// 검증: V42 P18
#[test]
fn full_stop_does_not_block_proposal_approval() {
    let e = env();
    full_stop(&e.store);
    let p = tightening(&e.store);
    let n = approved(&e.approvals, p);
    let (result, calls) = counted(&e.approvals, &n, p);
    result.unwrap();
    assert_eq!(calls, 1);
}

// 검증: E7 V44 P19
#[test]
fn approval_of_another_proposal_never_calls_f_and_voids_the_number() {
    let e = env();
    let a = widening(&e.store);
    let b = tightening(&e.store);
    let n = approved(&e.approvals, a);
    let (result, calls) = counted(&e.approvals, &n, b);
    assert_eq!(denied(result), RejectReason::FingerprintMismatch);
    assert_eq!(calls, 0);
    // 지문이 달라 거부된 번호는 원래 개선안으로도 쓸 수 없다(approve_proposal과 같다).
    let (again, calls) = counted(&e.approvals, &n, a);
    assert_eq!(denied(again), RejectReason::FingerprintMismatch);
    assert_eq!(calls, 0);
    assert_eq!(
        e.store.get_proposal(a).unwrap().state,
        ProposalState::Proposed
    );
}

// 검증: E8 V45
#[test]
fn used_approval_never_calls_f_again() {
    let e = env();
    let journal = Journal::new(e.store.clone());
    let p = widening(&e.store);
    let n = approved(&e.approvals, p);
    through_journal(&e.approvals, &journal, &n, p).unwrap();
    let (result, calls) = counted(&e.approvals, &n, p);
    assert_eq!(denied(result), RejectReason::AlreadyConsumed);
    assert_eq!(calls, 0);
}

// 검증: V43 P19
#[test]
fn rejected_approval_never_calls_f() {
    let e = env();
    let p = tightening(&e.store);
    let r = e.approvals.request(ask(subject(p))).unwrap();
    e.approvals
        .answer(&r.number, r.fingerprint.as_str(), Decision::Reject)
        .unwrap();
    let (result, calls) = counted(&e.approvals, &r.number, p);
    assert_eq!(denied(result), RejectReason::NotApproved);
    assert_eq!(calls, 0);
    assert_eq!(
        e.store.get_proposal(p).unwrap().state,
        ProposalState::Proposed
    );
}

// 검증: V42 E8 P19
#[test]
fn waiting_or_unknown_approval_never_calls_f() {
    let e = env();
    let p = tightening(&e.store);
    let waiting = e.approvals.request(ask(subject(p))).unwrap();
    let (result, calls) = counted(&e.approvals, &waiting.number, p);
    assert_eq!(denied(result), RejectReason::NotApproved);
    assert_eq!(calls, 0);
    let (result, calls) = counted(&e.approvals, &ApprovalNumber::new("a".repeat(32)), p);
    assert_eq!(denied(result), RejectReason::NotFound);
    assert_eq!(calls, 0);
}

// 검증: E28 V44
#[test]
fn error_from_f_comes_back_and_the_used_approval_is_not_given_back() {
    let e = env();
    let journal = Journal::new(e.store.clone());
    let p = widening(&e.store);
    let n = approved(&e.approvals, p);
    let missing = ProposalId(p.0 + 1_000);
    let result = e.approvals.with_proposal_approval(&n, p, |using, at| {
        Ok::<_, Fail>(journal.approve(missing, Some(using), &current(), at)?)
    });
    let Err(Fail::Journal(err)) = result else {
        panic!("journal 오류가 아니다: {result:?}");
    };
    assert!(matches!(err, pi_journal::Error::Store(_)), "{err:?}");
    assert_eq!(
        e.store.get_proposal(p).unwrap().state,
        ProposalState::Proposed
    );
    // f가 승인을 쓰지 못했으면 승인은 그대로라 같은 개선안으로 다시 쓸 수 있다.
    let out = through_journal(&e.approvals, &journal, &n, p).unwrap();
    assert_eq!(out.proposal.state, ProposalState::Cooling);
}

// 검증: V45 E8
#[test]
fn concurrent_proposal_approvals_pass_only_once() {
    let e = env();
    let journal = Arc::new(Journal::new(e.store.clone()));
    let p = widening(&e.store);
    let n = approved(&e.approvals, p);
    let handles: Vec<_> = (0..4)
        .map(|_| {
            let approvals = e.approvals.clone();
            let journal = journal.clone();
            let n = n.clone();
            thread::spawn(move || {
                through_journal(&approvals, &journal, &n, p)
                    .ok()
                    .map(|out| out.proposal.id)
            })
        })
        .collect();
    let passed: Vec<_> = handles
        .into_iter()
        .filter_map(|h| h.join().unwrap())
        .collect();
    assert_eq!(passed.len(), 1);
    assert_eq!(
        e.store.get_proposal(passed[0]).unwrap().state,
        ProposalState::Cooling
    );
}
