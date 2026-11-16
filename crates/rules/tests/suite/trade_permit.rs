//! 후보 일괄 처리(paper.md "다음 차수 후보 (exec 3차에서 넘어옴)"): 점검 통과 표시를 종목 ·
//! 방향만이 아니라 매매 번호에 묶는다(E33). `issue_for_trade`가 낸 표시는 그 매매 번호를
//! 지니고, 내용은 `issue`와 같다. 대조는 주문을 내는 쪽(exec)이 한다.

use crate::common::*;
use pi_paper::{CheckAuthority, InitialCash, PaperBroker, PaperConfig};
use pi_rules::{ApprovalGrant, Outcome, approved as approve, issue, issue_for_trade};

/// 원화 10,000,000원 모의계좌의 표시 권한.
fn authority() -> CheckAuthority {
    let config = PaperConfig::new(
        InitialCash {
            krw: d("10000000"),
            usd: d("0"),
        },
        kr_rules(),
        us_rules(),
    );
    PaperBroker::open(config).unwrap().1
}

// 검증: E33 C9
#[test]
fn a_permit_issued_for_a_trade_carries_that_trade_number() {
    let s = Scenario::kr();
    let passed = expect_pass(s.check(&kr_buy()));
    let want = passed.order().clone();
    let permit = issue_for_trade(&authority(), passed, 42);
    assert_eq!(permit.trade(), Some(42));
    assert_eq!(permit.approved(), &want);
}

// 검증: E33
#[test]
fn a_permit_issued_without_a_trade_has_no_trade_number() {
    let s = Scenario::kr();
    let permit = issue(&authority(), expect_pass(s.check(&kr_buy())));
    assert_eq!(permit.trade(), None);
}

// 검증: E33 E31
#[test]
fn a_stop_loss_permit_is_bound_to_its_trade_too() {
    let mut s = Scenario::kr();
    s.hold("005930", "10", "50000", "48000");
    let passed = expect_pass(s.check(&kr_stop_loss()));
    let want = passed.order().clone();
    let permit = issue_for_trade(&authority(), passed, 9);
    assert_eq!(permit.trade(), Some(9));
    assert_eq!(permit.approved(), &want);
}

// 검증: E21 E33
#[test]
fn an_approved_buy_gets_a_permit_bound_to_its_trade() {
    let mut s = Scenario::kr();
    s.params.auto_order_limit_pct = d("10");
    let Outcome::NeedsApproval(pending) = s.check(&kr_buy()) else {
        panic!("승인을 기다려야 한다");
    };
    let grant = ApprovalGrant::new("apv-test-1", "fp-test-abcdef");
    let passed = approve(pending, &grant, &kr_buy(), &s.ctx()).unwrap();
    let permit = issue_for_trade(&authority(), passed, 3);
    assert_eq!(permit.trade(), Some(3));
}
