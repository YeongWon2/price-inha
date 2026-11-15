//! 조건으로 찾기: 종목, 기간, 손익.

use pi_knowledge::{Condition, SearchMode};
use pi_store::UnixMillis;
use rust_decimal::Decimal;

use crate::common::{DAY, FakeCatalog, Fixture, T0, any_condition, refs, trade};

/// 매매 네 건을 색인하고 요약을 돌려준다. A(삼성, T0-3일, +100) B(삼성, T0-1일, -50) C(하이닉스, T0-2일, +10) D(삼성, T0, 손익 모름).
fn setup() -> (Fixture, FakeCatalog) {
    let f = Fixture::new();
    for r in ["A", "B", "C", "D"] {
        f.add_trade(r, &format!("{r} 매매 기록"));
    }
    let catalog = FakeCatalog(vec![
        trade("A", "005930", T0 - 3 * DAY, Some(100)),
        trade("B", "005930", T0 - DAY, Some(-50)),
        trade("C", "000660", T0 - 2 * DAY, Some(10)),
        trade("D", "005930", T0, None),
    ]);
    (f, catalog)
}

fn find(f: &Fixture, cond: &Condition, catalog: &FakeCatalog) -> Vec<String> {
    refs(
        &f.knowledge
            .search_by_condition(cond, catalog)
            .expect("조건 검색"),
    )
}

// 검증: D10
#[test]
fn no_condition_returns_every_trade_newest_first() {
    let (f, catalog) = setup();
    let out = f
        .knowledge
        .search_by_condition(&any_condition(), &catalog)
        .expect("조건 검색");
    assert_eq!(out.mode, SearchMode::Condition);
    assert_eq!(refs(&out), vec!["D", "B", "C", "A"]);
    assert!(out.results.iter().all(|h| h.score_micros.is_none()));
}

// 검증: D10
#[test]
fn symbol_filter() {
    let (f, catalog) = setup();
    let cond = Condition {
        symbol: Some("000660".to_string()),
        ..any_condition()
    };
    assert_eq!(find(&f, &cond, &catalog), vec!["C"]);
}

// 검증: D10
#[test]
fn period_bounds_are_inclusive() {
    let (f, catalog) = setup();
    let cond = Condition {
        from: Some(UnixMillis(T0 - 3 * DAY)),
        to: Some(UnixMillis(T0 - 2 * DAY)),
        ..any_condition()
    };
    assert_eq!(find(&f, &cond, &catalog), vec!["C", "A"]);
    let cond = Condition {
        from: Some(UnixMillis(T0 - 3 * DAY + 1)),
        to: Some(UnixMillis(T0 - 2 * DAY - 1)),
        ..any_condition()
    };
    assert!(find(&f, &cond, &catalog).is_empty());
}

// 검증: D10
#[test]
fn pnl_range_is_inclusive_and_skips_unknown_pnl() {
    let (f, catalog) = setup();
    let loss = Condition {
        max_pnl: Some(Decimal::from(-50)),
        ..any_condition()
    };
    assert_eq!(find(&f, &loss, &catalog), vec!["B"]);
    let band = Condition {
        min_pnl: Some(Decimal::from(10)),
        max_pnl: Some(Decimal::from(100)),
        ..any_condition()
    };
    assert_eq!(find(&f, &band, &catalog), vec!["C", "A"]);
}

// 검증: D10
#[test]
fn conditions_combine_with_and() {
    let (f, catalog) = setup();
    let cond = Condition {
        symbol: Some("005930".to_string()),
        min_pnl: Some(Decimal::from(0)),
        ..any_condition()
    };
    assert_eq!(find(&f, &cond, &catalog), vec!["A"]);
}

// 검증: D10
#[test]
fn trades_without_an_indexed_record_are_skipped() {
    let (f, mut catalog) = setup();
    catalog.0.push(trade("Z", "005930", T0 + DAY, Some(1)));
    let got = find(&f, &any_condition(), &catalog);
    assert!(!got.contains(&"Z".to_string()));
    assert_eq!(got.len(), 4);
}

// 검증: D10
#[test]
fn max_caps_the_result_and_default_is_six() {
    let (f, catalog) = setup();
    let cond = Condition {
        max: Some(2),
        ..any_condition()
    };
    assert_eq!(find(&f, &cond, &catalog), vec!["D", "B"]);
    let many = Fixture::new();
    let mut trades = Vec::new();
    for i in 0..9 {
        many.add_trade(&format!("T-{i}"), "매매 기록");
        trades.push(trade(&format!("T-{i}"), "005930", T0 - i * DAY, Some(1)));
    }
    assert_eq!(find(&many, &any_condition(), &FakeCatalog(trades)).len(), 6);
}

// 검증: D10 M7
#[test]
fn only_trade_records_are_found_by_condition() {
    let (f, catalog) = setup();
    f.add(
        pi_store::KnowledgeKind::Review,
        pi_store::KnowledgeSource::Mine,
        "A",
        "A 복기",
        T0,
    );
    let out = f
        .knowledge
        .search_by_condition(&any_condition(), &catalog)
        .expect("조건 검색");
    assert!(out.results.iter().all(|h| h.kind == "trade"));
    assert_eq!(out.trade_refs(), vec!["D", "B", "C", "A"]);
}
