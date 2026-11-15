//! 승인 요청 만들기(C12): 번호 · 기다릴 시간(N2) · 합치기 · 지정가 · 전체 정지.

use std::collections::HashSet;
use std::time::Duration;

use crate::common::{MIN, T0, ask, buy, env, env_with, full_stop, order, stop_loss, take_profit};
use pi_approval::{Decision, Error, OrderType, Side, SignalKind, Subject, WaitTimes, fingerprint};
use pi_store::{ApprovalStatus, ProposalId, TemplateId, UnixMillis};
use rust_decimal::Decimal;

// 검증: N2
#[test]
fn default_wait_times_are_n2() {
    assert_eq!(
        WaitTimes::default(),
        WaitTimes {
            stop_loss: Duration::from_secs(60),
            take_profit: Duration::from_secs(300),
            buy: Duration::from_secs(300),
        }
    );
}

// 검증: C12 M5
#[test]
fn request_returns_number_fingerprint_and_short_code() {
    let e = env();
    let subject = buy("sig-1");
    let r = e.approvals.request(ask(subject.clone())).unwrap();
    assert_eq!(r.fingerprint, fingerprint(&subject));
    assert_eq!(r.short_code, r.fingerprint.as_str()[..6]);
    assert_eq!(r.repeat, 1);

    let view = e.approvals.get(&r.number).unwrap().unwrap();
    assert_eq!(view.status, ApprovalStatus::Pending);
    assert_eq!(view.subject, subject);
    assert_eq!(view.reason, "금액 한도를 넘는 주문");
    assert_eq!(view.fingerprint, r.fingerprint);
    assert!(!view.consumed);
}

// 검증: C12 M5
#[test]
fn numbers_are_random_hex_and_never_reused() {
    let e = env();
    let mut seen = HashSet::new();
    for i in 0..30 {
        let r = e.approvals.request(ask(buy(&format!("sig-{i}")))).unwrap();
        let n = r.number.as_str().to_string();
        assert_eq!(n.len(), 32, "{n}");
        assert!(
            n.chars()
                .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c)),
            "{n}"
        );
        assert!(seen.insert(n));
    }
    // 저장소의 줄 번호(1부터)를 그대로 내보이지 않는다.
    assert!(!seen.contains("1"));
}

// 검증: N2 C12
#[test]
fn order_wait_uses_signal_kind_and_expiry_is_signal_validity() {
    let e = env();
    let b = e.approvals.request(ask(buy("b"))).unwrap();
    assert_eq!(b.wait_until, Some(UnixMillis(T0 + 5 * MIN)));
    assert_eq!(b.expires_at, Some(UnixMillis(T0 + 30 * MIN)));
    let s = e.approvals.request(ask(stop_loss("s"))).unwrap();
    assert_eq!(s.wait_until, Some(UnixMillis(T0 + MIN)));
    let t = e.approvals.request(ask(take_profit("t"))).unwrap();
    assert_eq!(t.wait_until, Some(UnixMillis(T0 + 5 * MIN)));
}

// 검증: N2
#[test]
fn wait_times_come_from_template_values() {
    let e = env_with(WaitTimes {
        stop_loss: Duration::from_secs(30),
        take_profit: Duration::from_secs(90),
        buy: Duration::from_secs(120),
    });
    let s = e.approvals.request(ask(stop_loss("s"))).unwrap();
    assert_eq!(s.wait_until, Some(UnixMillis(T0 + 30_000)));
    let t = e.approvals.request(ask(take_profit("t"))).unwrap();
    assert_eq!(t.wait_until, Some(UnixMillis(T0 + 90_000)));
    let b = e.approvals.request(ask(buy("b"))).unwrap();
    assert_eq!(b.wait_until, Some(UnixMillis(T0 + 120_000)));
}

// 검증: V42 C12
#[test]
fn non_order_subjects_have_no_deadline() {
    let e = env();
    for subject in [
        Subject::Restart,
        Subject::RuleImprovement {
            proposal: ProposalId(1),
        },
        Subject::LivePromotion {
            template: TemplateId(1),
            revision: 1,
        },
    ] {
        let r = e.approvals.request(ask(subject)).unwrap();
        assert_eq!(r.wait_until, None);
        assert_eq!(r.expires_at, None);
    }
}

// 검증: C12 M5
#[test]
fn same_pending_subject_merges_and_counts() {
    let e = env();
    let a = e.approvals.request(ask(buy("sig-1"))).unwrap();
    e.clock.set(T0 + 1_000);
    let b = e.approvals.request(ask(buy("sig-1"))).unwrap();
    let c = e.approvals.request(ask(buy("sig-1"))).unwrap();
    assert_eq!(a.number, b.number);
    assert_eq!(b.number, c.number);
    assert_eq!((a.repeat, b.repeat, c.repeat), (1, 2, 3));
    assert_eq!(
        e.store
            .approvals_in_status(ApprovalStatus::Pending)
            .unwrap()
            .len(),
        1
    );
}

// 검증: C12 M5
#[test]
fn different_content_for_same_signal_is_new_request() {
    let e = env();
    let a = e.approvals.request(ask(buy("sig-1"))).unwrap();
    let mut changed = order("sig-1", Side::Buy, SignalKind::Buy);
    changed.price = Decimal::new(70_900, 0);
    let b = e.approvals.request(ask(Subject::Order(changed))).unwrap();
    assert_ne!(a.number, b.number);
    assert_ne!(a.fingerprint, b.fingerprint);
    assert_eq!(b.repeat, 1);
}

// 검증: C12 M5
#[test]
fn resolved_request_is_not_merged() {
    let e = env();
    let a = e.approvals.request(ask(buy("sig-1"))).unwrap();
    e.approvals
        .answer(&a.number, a.fingerprint.as_str(), Decision::Reject)
        .unwrap();
    let b = e.approvals.request(ask(buy("sig-1"))).unwrap();
    assert_ne!(a.number, b.number);
    assert_eq!(b.repeat, 1);
}

// 검증: C12 E7
#[test]
fn market_order_cannot_be_requested() {
    let e = env();
    let mut o = order("sig-1", Side::Buy, SignalKind::Buy);
    o.order_type = OrderType::Market;
    let err = e.approvals.request(ask(Subject::Order(o))).unwrap_err();
    assert!(matches!(err, Error::LimitOrderRequired), "{err:?}");
    assert!(
        e.store
            .approvals_in_status(ApprovalStatus::Pending)
            .unwrap()
            .is_empty()
    );
}

// 검증: C12
#[test]
fn side_and_signal_kind_must_match() {
    let e = env();
    for (side, kind) in [
        (Side::Buy, SignalKind::StopLoss),
        (Side::Buy, SignalKind::TakeProfit),
        (Side::Sell, SignalKind::Buy),
    ] {
        let err = e
            .approvals
            .request(ask(Subject::Order(order("x", side, kind))))
            .unwrap_err();
        assert!(matches!(err, Error::InvalidSubject(_)), "{err:?}");
    }
}

// 검증: C12 E8
#[test]
fn signal_already_expired_is_refused() {
    let e = env();
    e.clock.set(T0 + 30 * MIN);
    let err = e.approvals.request(ask(buy("late"))).unwrap_err();
    assert!(matches!(err, Error::SignalAlreadyExpired), "{err:?}");
}

// 검증: V42 P18
#[test]
fn live_promotion_is_refused_during_full_stop() {
    let e = env();
    full_stop(&e.store);
    let err = e
        .approvals
        .request(ask(Subject::LivePromotion {
            template: TemplateId(1),
            revision: 1,
        }))
        .unwrap_err();
    assert!(matches!(err, Error::FullStop), "{err:?}");
    // 다시 시작과 규칙 개선은 전체 정지 중에도 요청할 수 있다.
    e.approvals.request(ask(Subject::Restart)).unwrap();
    e.approvals
        .request(ask(Subject::RuleImprovement {
            proposal: ProposalId(1),
        }))
        .unwrap();
}

// 검증: C12
#[test]
fn unknown_number_has_no_view() {
    let e = env();
    let none = e
        .approvals
        .get(&pi_approval::ApprovalNumber::new("0".repeat(32)))
        .unwrap();
    assert!(none.is_none());
}
