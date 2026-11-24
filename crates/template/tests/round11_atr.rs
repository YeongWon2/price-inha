//! 11차(2026-11-24): ATR 손절의 봉 단위를 넓히는 것도 위험 증가로 센다(P15 · E5 · E28 · P14).
//! 지시서 docs/dev/modules/template.md "11차 작업". 봉 단위의 순서는 분봉 < 일봉.

use pi_template::{
    AdjustPlan, AtrBar, AtrStop, Change, RiskField, RiskParams, Style, increases_risk, plan_adjust,
    preset, widened_fields,
};
use rust_decimal::Decimal;

fn d(s: &str) -> Decimal {
    s.parse().unwrap()
}

/// 스윙 프리셋에서 ATR 손절만 `multiple` 배 · `bar` 로 바꾼 값.
fn with_atr(multiple: Option<&str>, bar: AtrBar) -> RiskParams {
    let mut p = preset(Style::Swing);
    p.stop.atr = multiple.map(|m| AtrStop {
        multiple: d(m),
        bar,
    });
    p
}

/// `before` → `after` 의 판정이 `widened` 인지: widened_fields · increases_risk · plan_adjust 가 같은 답이어야 한다.
fn assert_verdict(before: &RiskParams, after: &RiskParams, widened: bool, name: &str) {
    let want: Vec<RiskField> = if widened {
        vec![RiskField::AtrStop]
    } else {
        vec![]
    };
    assert_eq!(
        widened_fields(before, after),
        want,
        "widened_fields: {name}"
    );
    assert_eq!(
        increases_risk(before, after),
        widened,
        "increases_risk: {name}"
    );
    let plan = plan_adjust(before, &[Change::Stop(after.stop.clone())])
        .unwrap_or_else(|e| panic!("plan_adjust 가 거부했다 ({name}): {e:?}"));
    if widened {
        assert_eq!(
            plan,
            AdjustPlan::NeedsApproval {
                after: after.clone(),
                widened: vec![RiskField::AtrStop],
            },
            "plan_adjust: {name}"
        );
    } else {
        assert_eq!(
            plan,
            AdjustPlan::ApplyNow(after.clone()),
            "plan_adjust: {name}"
        );
    }
}

// 검증: P15, E28, E5
#[test]
fn minute_to_day_same_multiple_is_widened_and_needs_approval() {
    let before = with_atr(Some("2"), AtrBar::Minute);
    let after = with_atr(Some("2"), AtrBar::Day);
    assert_verdict(&before, &after, true, "분봉 2배 → 일봉 2배");
}

// 검증: P15, E28
#[test]
fn day_to_minute_same_multiple_applies_now() {
    let before = with_atr(Some("2"), AtrBar::Day);
    let after = with_atr(Some("2"), AtrBar::Minute);
    assert_verdict(&before, &after, false, "일봉 2배 → 분봉 2배");
}

// 검증: P15, E28
#[test]
fn same_bar_same_multiple_is_not_widened() {
    for bar in [AtrBar::Minute, AtrBar::Day] {
        let p = with_atr(Some("2"), bar);
        assert_verdict(&p, &p.clone(), false, "봉 · 배수 모두 같음");
    }
}

// 검증: P15, E28
#[test]
fn multiple_up_or_atr_removed_stays_widened_in_either_bar() {
    for bar in [AtrBar::Minute, AtrBar::Day] {
        let before = with_atr(Some("2"), bar);
        assert_verdict(&before, &with_atr(Some("3"), bar), true, "배수 커짐");
        assert_verdict(&before, &with_atr(Some("1.5"), bar), false, "배수 작아짐");
        assert_verdict(&before, &with_atr(None, bar), true, "ATR 손절 없어짐");
    }
    // 없던 ATR 손절을 새로 건다 = 좁힘이다.
    assert_verdict(
        &with_atr(None, AtrBar::Day),
        &with_atr(Some("2"), AtrBar::Minute),
        false,
        "ATR 새로 걺",
    );
}

// 검증: P15, E28
#[test]
fn narrowing_bar_while_raising_multiple_is_widened() {
    // 일봉 → 분봉(좁힘)이어도 배수가 커지면 넓힘이다.
    let before = with_atr(Some("2"), AtrBar::Day);
    let after = with_atr(Some("3"), AtrBar::Minute);
    assert_verdict(&before, &after, true, "일봉 2배 → 분봉 3배");
}

// 검증: P15, E28
#[test]
fn widening_bar_while_lowering_multiple_is_still_widened() {
    // 분봉 → 일봉(넓힘)은 배수를 내려도 봉 단위가 넓어졌으므로 넓힘이다.
    let before = with_atr(Some("2"), AtrBar::Minute);
    let after = with_atr(Some("1.5"), AtrBar::Day);
    assert_verdict(&before, &after, true, "분봉 2배 → 일봉 1.5배");
}

// 검증: P15, E28, P14
#[test]
fn bar_widening_mixed_with_other_narrowing_still_needs_approval_for_whole_bundle() {
    let before = with_atr(Some("2"), AtrBar::Minute);
    let mut after = with_atr(Some("2"), AtrBar::Day);
    after.risk_per_trade_pct = d("0.5"); // 줄임
    let plan = plan_adjust(
        &before,
        &[
            Change::RiskPerTradePct(after.risk_per_trade_pct),
            Change::Stop(after.stop.clone()),
        ],
    )
    .expect("상한 안의 조정");
    assert_eq!(
        plan,
        AdjustPlan::NeedsApproval {
            after: after.clone(),
            widened: vec![RiskField::AtrStop],
        }
    );
    // 거부가 아니므로 지금 값은 그대로다.
    assert_eq!(before.stop.atr.map(|a| a.bar), Some(AtrBar::Minute));
}
