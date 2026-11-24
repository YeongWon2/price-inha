//! 4. `reduce_within_limit` 속성 시험(E22 · E21, 결정 14).
//!
//! 아무 한도 · 값 · 수수료율에서 결과 수량의 금액(`수량 × 값 + 올림(수량 × 값 × 수수료율)`)은 한도 이하이고,
//! 한 주 더하면 한도를 넘는다. 줄일 수 없으면(한 주도 못 사면) 한 주의 금액이 한도를 넘는다.
//! 같은 표본으로 `check`의 갈림(통과 ↔ 승인대기)도 같은 셈인지 본다.

use pi_rules::{Outcome, Reason, Violation, reduce_within_limit};
use proptest::prelude::{ProptestConfig, prop_assert, prop_assert_eq, prop_assume, proptest};
use rust_decimal::Decimal;

use crate::common::{Scenario, d, kr_buy, kr_rules_at};
use pi_rules::{OrderDraft, Rationale};

/// 한도와 비교하는 사는 금액: 수량 × 값 + 올림(수량 × 값 × 수수료율). 원은 1원 단위로 올린다.
fn cost(quantity: Decimal, price: Decimal, rate: Decimal) -> Decimal {
    let notional = quantity * price;
    notional + (notional * rate).ceil()
}

/// 값 `price`(호가 단위에 맞는 원)로 사는 안: 손절 6% 아래 · 목표 12% 위 · 기대값 5%.
fn draft_at(price: Decimal) -> OrderDraft {
    OrderDraft {
        limit_price: Some(price),
        rationale: Rationale {
            entry: Some(price),
            stop: Some(price * d("0.94")),
            target: Some(price * d("1.12")),
            expected_value: Some(price * d("0.05")),
            ..kr_buy().rationale
        },
        ..kr_buy()
    }
}

proptest! {
    // 실패한 씨앗을 테스트 폴더에 파일로 남기지 않는다.
    #![proptest_config(ProptestConfig {
        cases: 256,
        failure_persistence: None,
        ..ProptestConfig::default()
    })]

    // 검증: E22 E21 P12
    #[test]
    fn the_reduced_quantity_is_the_largest_one_whose_cost_with_fee_fits_the_limit(
        thousand in 10_i64..=100,   // 값 = thousand × 1,000원
        shares in 1_i64..=15,       // 한도의 바탕 = shares × 값
        offset in -400_i64..=400,   // 한도 = 바탕 + offset(원). 경계 가까이를 자주 뽑는다.
        rate_e5 in 0_i64..=500,     // 수수료율 = rate_e5 × 0.00001 (0 ~ 0.5%)
    ) {
        let price = Decimal::new(thousand * 1000, 0);
        let rate = Decimal::new(rate_e5, 5);
        let limit = price * Decimal::new(shares, 0) + Decimal::new(offset, 0);
        prop_assume!(limit > Decimal::ZERO);

        let mut s = Scenario::kr();
        s.quote.price = price;
        s.rules = kr_rules_at(rate);
        s.set_limit_won(limit);
        let draft = draft_at(price);

        // 승인을 기다리는 주문만 줄임의 대상이다(한도 안이면 통과, 막히면 다른 시험의 몫).
        let Outcome::NeedsApproval(pending) = s.check(&draft) else {
            prop_assume!(false);
            return Ok(());
        };
        // 점검의 갈림도 같은 셈: 기다리는 주문은 수수료를 넣은 금액이 한도를 넘는다.
        prop_assert!(cost(pending.order().quantity, price, rate) > limit);
        prop_assert_eq!(pending.amount(), pending.order().quantity * price);

        match reduce_within_limit(pending, &draft, &s.ctx()) {
            Ok(passed) => {
                let q = passed.order().quantity;
                prop_assert!(q >= Decimal::ONE);
                prop_assert!(cost(q, price, rate) <= limit, "q={q} 한도={limit} 값={price} 율={rate}");
                prop_assert!(
                    cost(q + Decimal::ONE, price, rate) > limit,
                    "한 주 더해도 한도 안: q={q} 한도={limit} 값={price} 율={rate}"
                );
                // 보이는 금액은 수량 × 값 그대로, 증표 없음.
                prop_assert_eq!(passed.amount(), q * price);
                prop_assert!(passed.approval().is_none());
            }
            Err(blocked) => {
                // 줄일 수 없는 경우는 한 주도 한도 안에 못 드는 때뿐이다.
                prop_assert_eq!(
                    blocked.reason,
                    Reason::Violations(vec![Violation::NothingWithinLimit])
                );
                prop_assert!(cost(Decimal::ONE, price, rate) > limit);
            }
        }
    }

    // 검증: E21 V25
    #[test]
    fn check_passes_exactly_when_the_cost_with_fee_is_within_the_limit(
        thousand in 10_i64..=100,
        shares in 1_i64..=15,
        offset in -400_i64..=400,
        rate_e5 in 0_i64..=500,
        ai_quantity in 1_i64..=15,
    ) {
        let price = Decimal::new(thousand * 1000, 0);
        let rate = Decimal::new(rate_e5, 5);
        let limit = price * Decimal::new(shares, 0) + Decimal::new(offset, 0);
        prop_assume!(limit > Decimal::ZERO);

        let mut s = Scenario::kr();
        s.quote.price = price;
        s.rules = kr_rules_at(rate);
        s.set_limit_won(limit);
        let draft = OrderDraft {
            ai_quantity: Some(Decimal::new(ai_quantity, 0)),
            ..draft_at(price)
        };
        match s.check(&draft) {
            Outcome::Pass(passed) => {
                let q = passed.order().quantity;
                prop_assert!(cost(q, price, rate) <= limit, "q={q} 한도={limit} 값={price} 율={rate}");
                prop_assert_eq!(passed.amount(), q * price);
            }
            Outcome::NeedsApproval(pending) => {
                let q = pending.order().quantity;
                prop_assert!(cost(q, price, rate) > limit, "q={q} 한도={limit} 값={price} 율={rate}");
                prop_assert_eq!(pending.amount(), q * price);
                prop_assert_eq!(pending.limit(), limit);
            }
            Outcome::Blocked(_) => prop_assume!(false),
        }
    }
}
