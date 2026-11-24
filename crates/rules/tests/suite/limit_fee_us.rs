//! 사는 주문의 금액 한도에 넣는 수수료는 그 시장 돈의 최소 단위(달러는 1센트)로 올린다(E21, 결정 14).
//! 국내(원)의 경계는 `tests/round5/limit_fee.rs`가 본다.

use crate::common::*;
use pi_rules::Outcome;

// 검증: E21 V25
#[test]
fn a_us_buy_limit_counts_the_fee_rounded_up_to_the_cent() {
    // 66주 × 25.50 = 1,683.00, 수수료 1,683.00 × 0.00015 = 0.25245 → 올림 0.26 → 1,683.26.
    // 한도가 1,683.26이면 통과, 1센트 모자라면 승인대기. 보이는 금액은 1,683.00 그대로.
    let mut s = Scenario::us();
    s.params.auto_order_limit_pct = d("16.8326");
    let passed = expect_pass(s.check(&us_buy()));
    assert_eq!(passed.amount(), d("1683.00"));

    s.params.auto_order_limit_pct = d("16.8325");
    match s.check(&us_buy()) {
        Outcome::NeedsApproval(pending) => {
            assert_eq!(pending.amount(), d("1683.00"));
            assert_eq!(pending.limit(), d("1683.25"));
        }
        other => panic!("승인을 기다려야 한다: {other:?}"),
    }
}
