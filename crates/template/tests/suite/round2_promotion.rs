//! 2차 2번: 승격 낙폭 기준은 그 템플릿의 지금 최대 낙폭 한도의 절반에서 센다(E26 · P14 · 결정 30번).

use crate::common::d;
use pi_template::{Change, Criterion, PromotionStats, Style, Verdict, adjust, preset};

/// 낙폭 말고 모든 기본 기준(30건 · 20일 · 손익 > 0 · 표본 밖 > 0)을 넘는 결과.
fn stats(max_drawdown_pct: &str) -> PromotionStats {
    PromotionStats {
        trades: 30,
        trading_days: 20,
        net_pnl: d("1"),
        max_drawdown_pct: d(max_drawdown_pct),
        out_of_sample_pnl: Some(d("1")),
    }
}

// 검증: E26, P14, C4
#[test]
fn preset_drawdown_criterion_is_half_of_its_limit() {
    // 스윙 한도 15% → 기준 7.5%.
    let p = preset(Style::Swing);
    assert_eq!(p.evaluate_promotion(&stats("7.5")), Verdict::Passed);
    assert_eq!(
        p.evaluate_promotion(&stats("7.5001")),
        Verdict::Short(vec![Criterion::MaxDrawdown])
    );
}

// 검증: E26, P14, C14
#[test]
fn lowering_the_limit_lowers_the_drawdown_criterion() {
    let current = preset(Style::Swing);
    let next = adjust(&current, &[Change::MaxDrawdownPct(d("10"))]).unwrap();
    assert_eq!(
        next.evaluate_promotion(&stats("7.5")),
        Verdict::Short(vec![Criterion::MaxDrawdown])
    );
    assert_eq!(
        next.evaluate_promotion(&stats("5.0001")),
        Verdict::Short(vec![Criterion::MaxDrawdown])
    );
    assert_eq!(next.evaluate_promotion(&stats("5")), Verdict::Passed);
    // 지금 값은 그대로 7.5% 기준이다.
    assert_eq!(current.evaluate_promotion(&stats("7.5")), Verdict::Passed);
}

// 검증: E26, P14, C14
#[test]
fn raising_the_limit_raises_the_drawdown_criterion() {
    // 단타 한도 10% → 기준 5%. 20%로 올리면 10%.
    let current = preset(Style::DayTrade);
    assert_eq!(
        current.evaluate_promotion(&stats("8")),
        Verdict::Short(vec![Criterion::MaxDrawdown])
    );
    let next = adjust(&current, &[Change::MaxDrawdownPct(d("20"))]).unwrap();
    assert_eq!(next.evaluate_promotion(&stats("8")), Verdict::Passed);
    assert_eq!(next.evaluate_promotion(&stats("10")), Verdict::Passed);
    assert_eq!(
        next.evaluate_promotion(&stats("10.0001")),
        Verdict::Short(vec![Criterion::MaxDrawdown])
    );
}

// 검증: E26, P14
#[test]
fn other_criteria_still_come_from_the_template() {
    let current = preset(Style::LongTerm);
    let mut criteria = current.promotion.clone();
    criteria.min_trades = 40;
    let next = adjust(&current, &[Change::Promotion(criteria)]).unwrap();
    // 중장기 한도 20% → 기준 10%. 30건이면 40건 기준에 모자란다.
    assert_eq!(
        next.evaluate_promotion(&stats("10")),
        Verdict::Short(vec![Criterion::Trades])
    );
    let thin = PromotionStats {
        trading_days: 19,
        out_of_sample_pnl: None,
        ..stats("10.5")
    };
    assert_eq!(
        current.evaluate_promotion(&thin),
        Verdict::Short(vec![
            Criterion::TradingDays,
            Criterion::MaxDrawdown,
            Criterion::OutOfSample,
        ])
    );
}
