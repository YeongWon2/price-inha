//! 10차(2026-11-21 스윙에 뉴스 구독 — 사용자 결정 19): 스윙 프리셋 구독 트리거(C4 · V27).
//! 지시서 docs/dev/modules/template.md "10차 작업". 원본에 없음 — 사용자 결정.

use pi_template::{Style, TriggerKind, preset};

// 검증: C4, V27
#[test]
fn swing_preset_subscribes_quote_disclosure_news_in_order() {
    assert_eq!(
        preset(Style::Swing).triggers,
        vec![
            TriggerKind::Quote,
            TriggerKind::Disclosure,
            TriggerKind::News
        ]
    );
}

// 검증: C4, V27
#[test]
fn day_trade_and_long_term_triggers_stay_the_same() {
    assert_eq!(
        preset(Style::DayTrade).triggers,
        vec![
            TriggerKind::Quote,
            TriggerKind::Disclosure,
            TriggerKind::News
        ]
    );
    assert_eq!(
        preset(Style::LongTerm).triggers,
        vec![TriggerKind::Disclosure, TriggerKind::Schedule]
    );
}
