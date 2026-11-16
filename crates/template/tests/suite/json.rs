//! 판 번호가 붙은 JSON(만드는 것 7 · C4 · E5).

use crate::common::d;
use pi_template::{Change, Error, FORMAT_VERSION, Style, Template, TemplateState, adjust, preset};
use rust_decimal::Decimal;
use serde_json::Value;

// 검증: C4, V27
#[test]
fn new_template_holds_the_style_preset() {
    for style in Style::ALL {
        let t = Template::new(style);
        assert_eq!(t.style, style);
        assert_eq!(t.params, preset(style));
    }
}

// 검증: C4
#[test]
fn json_carries_version_and_style() {
    assert_eq!(FORMAT_VERSION, 1);
    let text = Template::new(Style::Swing).to_json();
    let v: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(v["version"], Value::from(1));
    assert_eq!(v["style"], Value::from("swing"));
    assert!(v["params"].is_object());
    let text = Template::new(Style::DayTrade).to_json();
    let v: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(v["style"], Value::from("day_trade"));
    let text = Template::new(Style::LongTerm).to_json();
    let v: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(v["style"], Value::from("long_term"));
}

// 검증: C4, C14
#[test]
fn json_round_trip_keeps_every_value() {
    for style in Style::ALL {
        let t = Template::new(style);
        assert_eq!(Template::from_json(&t.to_json()).unwrap(), t);
    }
    let params = adjust(
        &preset(Style::Swing),
        &[
            Change::RiskPerTradePct(d("1.25")),
            Change::ConvictionWeightPct(Some(d("18"))),
        ],
    )
    .unwrap();
    let t = Template {
        style: Style::Swing,
        params,
    };
    let back = Template::from_json(&t.to_json()).unwrap();
    assert_eq!(back, t);
    assert_eq!(back.params.risk_per_trade_pct, d("1.25"));
}

// 검증: C4
#[test]
fn unknown_version_is_rejected() {
    let text = Template::new(Style::Swing).to_json();
    let mut v: Value = serde_json::from_str(&text).unwrap();
    v["version"] = Value::from(2);
    let err = Template::from_json(&v.to_string()).unwrap_err();
    assert!(
        matches!(err, Error::UnsupportedVersion { found: 2 }),
        "{err:?}"
    );
    let mut v: Value = serde_json::from_str(&text).unwrap();
    v["version"] = Value::from(0);
    let err = Template::from_json(&v.to_string()).unwrap_err();
    assert!(
        matches!(err, Error::UnsupportedVersion { found: 0 }),
        "{err:?}"
    );
}

// 검증: C4
#[test]
fn broken_json_is_rejected() {
    for text in [
        "",
        "{",
        "[]",
        "{\"version\":1}",
        "{\"version\":1,\"style\":\"swing\"}",
    ] {
        let err = Template::from_json(text).unwrap_err();
        assert!(matches!(err, Error::Json(_)), "{text}: {err:?}");
    }
    let text = Template::new(Style::Swing).to_json();
    let mut v: Value = serde_json::from_str(&text).unwrap();
    v["style"] = Value::from("scalping");
    let err = Template::from_json(&v.to_string()).unwrap_err();
    assert!(matches!(err, Error::Json(_)), "{err:?}");
}

// 검증: E5, V28
#[test]
fn over_cap_value_in_json_is_rejected() {
    let text = Template::new(Style::Swing).to_json();
    let mut v: Value = serde_json::from_str(&text).unwrap();
    v["params"]["risk_per_trade_pct"] = Value::from("3");
    let err = Template::from_json(&v.to_string()).unwrap_err();
    match err {
        Error::OverCap(over) => {
            assert_eq!(over.violations.len(), 1);
            assert_eq!(over.violations[0].cap, d("2"));
            assert_eq!(over.violations[0].requested, d("3"));
        }
        other => panic!("상한 오류가 아님: {other:?}"),
    }
}

// 검증: M3
#[test]
fn state_names_serialize_as_snake_case() {
    assert_eq!(
        serde_json::to_value(TemplateState::PendingPromotion).unwrap(),
        Value::from("pending_promotion")
    );
    let back: TemplateState = serde_json::from_value(Value::from("live")).unwrap();
    assert_eq!(back, TemplateState::Live);
}

// 검증: C4, E5 (이번 차수: 옛 JSON 에서 승격 낙폭 칸이 빠져도 읽힘)
#[test]
fn json_without_promotion_drawdown_fills_half_of_the_limit() {
    for style in Style::ALL {
        let t = Template::new(style);
        let mut v: Value = serde_json::from_str(&t.to_json()).unwrap();
        v["params"]["promotion"]
            .as_object_mut()
            .unwrap()
            .remove("max_drawdown_pct");
        let back = Template::from_json(&v.to_string()).unwrap();
        assert_eq!(back, t);
        assert_eq!(
            back.params.promotion.max_drawdown_pct,
            back.params.max_drawdown_pct / d("2")
        );
    }
}

// 검증: C4
#[test]
fn json_with_promotion_drawdown_keeps_the_given_value() {
    let t = Template::new(Style::Swing);
    let mut v: Value = serde_json::from_str(&t.to_json()).unwrap();
    v["params"]["promotion"]["max_drawdown_pct"] = Value::from("3.5");
    let back = Template::from_json(&v.to_string()).unwrap();
    assert_eq!(back.params.promotion.max_drawdown_pct, d("3.5"));
}

// 검증: C4
#[test]
fn json_without_the_drawdown_limit_itself_is_still_an_error() {
    let t = Template::new(Style::Swing);
    let mut v: Value = serde_json::from_str(&t.to_json()).unwrap();
    let params = v["params"].as_object_mut().unwrap();
    params.remove("max_drawdown_pct");
    params["promotion"]
        .as_object_mut()
        .unwrap()
        .remove("max_drawdown_pct");
    assert!(matches!(
        Template::from_json(&v.to_string()),
        Err(Error::Json(_))
    ));
}
