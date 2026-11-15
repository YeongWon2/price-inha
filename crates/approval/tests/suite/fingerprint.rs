//! 내용 지문: 정규 JSON의 SHA-256, 앞 6자 표시(결정 4번 · 7번).

use crate::common::{T0, buy, order};
use pi_approval::{OrderType, Side, SignalKind, Subject, canonical_json, fingerprint};
use pi_store::{AccountKind, ProposalId, TemplateId, UnixMillis};
use rust_decimal::Decimal;
use sha2::{Digest, Sha256};

fn sha256_hex(text: &str) -> String {
    Sha256::digest(text.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

// 검증: M5 E7
#[test]
fn order_canonical_json_has_fixed_field_order() {
    let mut o = order("sig-1", Side::Buy, SignalKind::Buy);
    o.price = Decimal::new(7_100_050, 2);
    let subject = Subject::Order(o);
    assert_eq!(
        canonical_json(&subject),
        r#"{"kind":"order","account":"paper","symbol":"005930","side":"buy","quantity":10,"price":"71000.5","order_type":"limit","signal_id":"sig-1"}"#
    );
}

// 검증: M5 E7
#[test]
fn fingerprint_is_sha256_hex_of_canonical_json() {
    let subject = buy("sig-1");
    let fp = fingerprint(&subject);
    assert_eq!(fp.as_str(), sha256_hex(&canonical_json(&subject)));
    assert_eq!(fp.as_str().len(), 64);
    assert!(
        fp.as_str()
            .chars()
            .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c))
    );
}

// 검증: C12
#[test]
fn short_code_is_first_six_chars() {
    let fp = fingerprint(&buy("sig-1"));
    assert_eq!(fp.short_code(), &fp.as_str()[..6]);
}

// 검증: M5 E7 V44
#[test]
fn every_order_field_changes_fingerprint() {
    let base = order("sig-1", Side::Buy, SignalKind::Buy);
    let base_fp = fingerprint(&Subject::Order(base.clone()));

    let mut changed = Vec::new();
    let mut o = base.clone();
    o.account = AccountKind::Live;
    changed.push(o);
    let mut o = base.clone();
    o.symbol = "000660".to_string();
    changed.push(o);
    let mut o = base.clone();
    o.side = Side::Sell;
    changed.push(o);
    let mut o = base.clone();
    o.quantity = 11;
    changed.push(o);
    let mut o = base.clone();
    o.price = Decimal::new(71_100, 0);
    changed.push(o);
    let mut o = base.clone();
    o.order_type = OrderType::Market;
    changed.push(o);
    let mut o = base.clone();
    o.signal_id = "sig-2".to_string();
    changed.push(o);

    for o in changed {
        assert_ne!(fingerprint(&Subject::Order(o.clone())), base_fp, "{o:?}");
    }
}

// 검증: M5
#[test]
fn signal_kind_and_validity_are_not_in_fingerprint() {
    let base = order("sig-1", Side::Sell, SignalKind::StopLoss);
    let mut other = base.clone();
    other.signal_kind = SignalKind::TakeProfit;
    other.signal_valid_until = UnixMillis(T0 + 1);
    assert_eq!(
        fingerprint(&Subject::Order(base)),
        fingerprint(&Subject::Order(other))
    );
}

// 검증: M5
#[test]
fn equal_prices_with_different_scale_share_fingerprint() {
    let mut a = order("sig-1", Side::Buy, SignalKind::Buy);
    a.price = Decimal::new(71_000, 0);
    let mut b = a.clone();
    b.price = Decimal::new(7_100_000, 2);
    assert_eq!(
        fingerprint(&Subject::Order(a)),
        fingerprint(&Subject::Order(b))
    );
}

// 검증: M5 V42
#[test]
fn other_subjects_canonical_json() {
    assert_eq!(
        canonical_json(&Subject::LivePromotion {
            template: TemplateId(3),
            revision: 2
        }),
        r#"{"kind":"live_promotion","template":3,"revision":2}"#
    );
    assert_eq!(
        canonical_json(&Subject::RuleImprovement {
            proposal: ProposalId(7)
        }),
        r#"{"kind":"rule_improvement","proposal":7}"#
    );
    assert_eq!(canonical_json(&Subject::Restart), r#"{"kind":"restart"}"#);
}

// 검증: M5 E7
#[test]
fn subject_kind_and_target_separate_fingerprints() {
    let promo1 = fingerprint(&Subject::LivePromotion {
        template: TemplateId(1),
        revision: 1,
    });
    let promo2 = fingerprint(&Subject::LivePromotion {
        template: TemplateId(2),
        revision: 1,
    });
    // 같은 템플릿이라도 판이 다르면 지문이 다르다(2차 1번).
    let promo1_rev2 = fingerprint(&Subject::LivePromotion {
        template: TemplateId(1),
        revision: 2,
    });
    let rule1 = fingerprint(&Subject::RuleImprovement {
        proposal: ProposalId(1),
    });
    let restart = fingerprint(&Subject::Restart);
    let order = fingerprint(&buy("1"));
    let all = [&promo1, &promo2, &promo1_rev2, &rule1, &restart, &order];
    for (i, a) in all.iter().enumerate() {
        for b in all.iter().skip(i + 1) {
            assert_ne!(a, b);
        }
    }
}

// 검증: M5
#[test]
fn strings_are_json_escaped() {
    let mut o = order("sig\"1\\", Side::Buy, SignalKind::Buy);
    o.symbol = "A\"B".to_string();
    let json = canonical_json(&Subject::Order(o));
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed["symbol"], "A\"B");
    assert_eq!(parsed["signal_id"], "sig\"1\\");
    assert!(!json.contains(' '));
}
