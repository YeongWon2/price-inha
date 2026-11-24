//! 3. 승인 증표로 지난 주문의 점검 통과 표시(E33, 결정 14).
//!
//! `issue_for_trade`는 증표가 있는 `Passed`(승인으로 지난 주문)를 `granted() == true`로 낸다.
//! 증표가 없으면 `granted() == false`. 매매에 묶이지 않은 `issue`는 증표가 있어도 `granted() ==
//! false`인 표시를 낸다(시그니처는 그대로 — 받는 쪽은 `trade()`가 `None`인 표시를 승인 길로 쓰지 않는다).

use pi_paper::{CheckAuthority, InitialCash, PaperBroker, PaperConfig};
use pi_rules::{
    ApprovalGrant, Outcome, Passed, approved as approve, issue, issue_for_trade,
    reduce_within_limit,
};

use crate::common::{Scenario, d, expect_pass, kr_buy, kr_rules_at};

fn authority() -> CheckAuthority {
    let config = PaperConfig::new(
        InitialCash {
            krw: d("10000000"),
            usd: d("0"),
        },
        kr_rules_at(d("0.00015")),
        kr_rules_at(d("0.00015")),
    );
    PaperBroker::open(config).unwrap().1
}

/// 한도 100만 장면에서 승인을 기다리던 33주 사기가 승인되어 지난 `Passed`와 그 장면.
fn approved_buy() -> Passed {
    let mut s = Scenario::kr();
    s.set_limit_won(d("1000000"));
    let Outcome::NeedsApproval(pending) = s.check(&kr_buy()) else {
        panic!("승인을 기다려야 한다");
    };
    let grant = ApprovalGrant::new("apv-test-1", "fp-test-abcdef");
    let passed = approve(pending, &grant, &kr_buy(), &s.ctx()).unwrap();
    assert!(passed.approval().is_some());
    passed
}

// 검증: E33 E21
#[test]
fn an_approved_order_gets_a_granted_permit_bound_to_its_trade() {
    let passed = approved_buy();
    let want = passed.order().clone();
    let permit = issue_for_trade(&authority(), passed, 7);
    assert!(permit.granted());
    assert_eq!(permit.trade(), Some(7));
    assert_eq!(permit.approved(), &want);
}

// 검증: E33
#[test]
fn an_order_that_passed_on_its_own_gets_a_permit_that_is_not_granted() {
    let s = Scenario::kr();
    let passed = expect_pass(s.check(&kr_buy()));
    assert!(passed.approval().is_none());
    let permit = issue_for_trade(&authority(), passed, 8);
    assert!(!permit.granted());
    assert_eq!(permit.trade(), Some(8));
}

// 검증: E33 E22
#[test]
fn a_reduced_unanswered_buy_gets_a_permit_that_is_not_granted() {
    // 답 없음 줄임은 승인 증표가 없다 → 표시도 granted가 아니다(exec 줄임 길이 받는 표시).
    let mut s = Scenario::kr();
    s.set_limit_won(d("1000000"));
    let Outcome::NeedsApproval(pending) = s.check(&kr_buy()) else {
        panic!("승인을 기다려야 한다");
    };
    let passed = reduce_within_limit(pending, &kr_buy(), &s.ctx()).unwrap();
    let permit = issue_for_trade(&authority(), passed, 9);
    assert!(!permit.granted());
    assert_eq!(permit.trade(), Some(9));
}

// 검증: E33 V26
#[test]
fn an_approved_order_issued_without_a_trade_is_never_granted() {
    // `issue`는 시그니처를 바꾸지 않는다: 증표가 있어도 매매에 묶이지 않은 표시는 granted가 아니다.
    let passed = approved_buy();
    let want = passed.order().clone();
    let permit = issue(&authority(), passed);
    assert!(!permit.granted());
    assert_eq!(permit.trade(), None);
    assert_eq!(permit.approved(), &want);
}
