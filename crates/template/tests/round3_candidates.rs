//! 3차 후보: 승격 임계값의 아래쪽 경계, 손절 방식이 모두 꺼진 조합 거부(C14 · E5 · M3 · P14).

use pi_template::{
    Change, Error, Invalid, InvalidReason, MaxHolding, OverCap, ParamField, PromotionCriteria,
    RiskParams, StopRule, Style, Template, adjust, preset,
};
use rust_decimal::Decimal;
use serde_json::{Value, json};

fn d(s: &str) -> Decimal {
    s.parse().unwrap()
}

fn rejected(current: &RiskParams, changes: &[Change]) -> OverCap {
    let before = current.clone();
    let err = match adjust(current, changes) {
        Ok(next) => panic!("거부되어야 함: {changes:?} → {next:?}"),
        Err(e) => e,
    };
    assert_eq!(current, &before, "거부된 조정이 지금 값을 바꿈");
    err
}

fn has(err: &OverCap, field: ParamField, reason: InvalidReason) -> bool {
    err.invalid.contains(&Invalid { field, reason })
}

fn all_off() -> StopRule {
    StopRule {
        fixed_loss_pct: None,
        atr: None,
        thesis_break: false,
        flatten_before_close: false,
    }
}

// 검증: C14, E5, P14
#[test]
fn negative_promotion_thresholds_are_rejected() {
    let current = preset(Style::Swing);
    let net = PromotionCriteria {
        net_pnl_above: d("-1"),
        ..current.promotion.clone()
    };
    let err = rejected(&current, &[Change::Promotion(net)]);
    assert!(has(&err, ParamField::Promotion, InvalidReason::NotPositive));

    let oos = PromotionCriteria {
        out_of_sample_pnl_above: Some(d("-0.01")),
        ..current.promotion.clone()
    };
    let err = rejected(&current, &[Change::Promotion(oos)]);
    assert!(has(&err, ParamField::Promotion, InvalidReason::NotPositive));
}

// 검증: C14, P14
#[test]
fn zero_promotion_thresholds_are_accepted() {
    let current = preset(Style::Swing);
    let zero = PromotionCriteria {
        net_pnl_above: Decimal::ZERO,
        out_of_sample_pnl_above: Some(Decimal::ZERO),
        ..current.promotion.clone()
    };
    let next = adjust(&current, &[Change::Promotion(zero.clone())]).unwrap();
    assert_eq!(next.promotion, zero);
}

// 검증: E5
#[test]
fn negative_promotion_threshold_in_json_is_rejected() {
    let text = Template::new(Style::Swing).to_json();
    let mut v: Value = serde_json::from_str(&text).unwrap();
    let promo = v
        .pointer_mut("/params/promotion")
        .expect("promotion 칸이 JSON 에 있어야 함");
    promo["net_pnl_above"] = json!("-5");
    match Template::from_json(&v.to_string()) {
        Err(Error::OverCap(over)) => {
            assert!(has(
                &over,
                ParamField::Promotion,
                InvalidReason::NotPositive
            ));
        }
        other => panic!("거부되지 않음: {other:?}"),
    }
}

// 검증: C14, E5
#[test]
fn all_stop_methods_off_is_rejected() {
    let mut current = preset(Style::Swing);
    current.max_holding = MaxHolding::Unlimited;
    let err = rejected(&current, &[Change::Stop(all_off())]);
    assert!(has(&err, ParamField::Stop, InvalidReason::NoStopMethod));
}

// 검증: C14
#[test]
fn a_time_stop_keeps_the_combination_valid() {
    let mut current = preset(Style::Swing);
    current.max_holding = MaxHolding::Unlimited;
    let changes = [
        Change::Stop(all_off()),
        Change::MaxHolding(MaxHolding::SameDay),
    ];
    let next = adjust(&current, &changes).unwrap();
    assert_eq!(next.stop, all_off());
}

// 검증: C14
#[test]
fn any_single_stop_method_is_enough() {
    let mut current = preset(Style::Swing);
    current.max_holding = MaxHolding::Unlimited;
    for stop in [
        StopRule {
            thesis_break: true,
            ..all_off()
        },
        StopRule {
            flatten_before_close: true,
            ..all_off()
        },
    ] {
        adjust(&current, &[Change::Stop(stop)]).unwrap();
    }
}

// 검증: E5
#[test]
fn all_stop_methods_off_in_json_is_rejected() {
    let mut params = preset(Style::Swing);
    params.max_holding = MaxHolding::Unlimited;
    params.stop = all_off();
    let mut v: Value = serde_json::from_str(&Template::new(Style::Swing).to_json()).unwrap();
    v["params"] = serde_json::to_value(&params).unwrap();
    match Template::from_json(&v.to_string()) {
        Err(Error::OverCap(over)) => {
            assert!(has(&over, ParamField::Stop, InvalidReason::NoStopMethod))
        }
        other => panic!("거부되지 않음: {other:?}"),
    }
}
