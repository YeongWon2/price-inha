//! 조건 검색의 실수 유형(4차 1번): `Condition::mistake` · `TradeCatalog::mistakes`.

use std::collections::HashMap;

use pi_knowledge::{Condition, TradeCatalog, TradeRecord};
use pi_store::{KnowledgeKind, KnowledgeSource, UnixMillis};

use crate::common::{DAY, FakeCatalog, Fixture, T0, any_condition, refs, trade};

/// 실수 유형까지 아는 가짜 매매 요약.
struct MistakeCatalog {
    trades: Vec<TradeRecord>,
    mistakes: HashMap<String, Vec<String>>,
}

impl TradeCatalog for MistakeCatalog {
    fn trades(&self) -> Vec<TradeRecord> {
        self.trades.clone()
    }
    fn mistakes(&self, record_ref: &str) -> Vec<String> {
        self.mistakes.get(record_ref).cloned().unwrap_or_default()
    }
}

/// (매매, 실수들) 묶음으로 카탈로그를 만든다.
fn catalog(items: Vec<(TradeRecord, &[&str])>) -> MistakeCatalog {
    let mut mistakes = HashMap::new();
    let mut trades = Vec::new();
    for (t, ms) in items {
        mistakes.insert(
            t.record_ref.clone(),
            ms.iter().map(|m| m.to_string()).collect(),
        );
        trades.push(t);
    }
    MistakeCatalog { trades, mistakes }
}

fn with_mistake(m: &str) -> Condition {
    Condition {
        mistake: Some(m.to_string()),
        ..any_condition()
    }
}

/// 매매 기록 셋(T-1 늦은 손절, T-2 추격 매수, T-3 실수 없음)을 올린 시험대.
fn three_trades() -> (Fixture, MistakeCatalog) {
    let f = Fixture::new();
    f.add_trade("T-1", "삼성전자 매수 뒤 손절이 늦었다");
    f.add_trade("T-2", "하이닉스 추격 매수");
    f.add_trade("T-3", "카카오 계획대로 매도");
    let c = catalog(vec![
        (trade("T-1", "005930", T0 - DAY, Some(-100)), &["late_stop"]),
        (
            trade("T-2", "000660", T0 - 2 * DAY, Some(50)),
            &["chased_entry", "oversized_position"],
        ),
        (trade("T-3", "035720", T0 - 3 * DAY, Some(30)), &[]),
    ]);
    (f, c)
}

// 검증: D10 V22
#[test]
fn mistake_condition_keeps_only_trades_with_that_mistake() {
    let (f, c) = three_trades();
    let out = f
        .knowledge
        .search_by_condition(&with_mistake("late_stop"), &c)
        .expect("찾기");
    assert_eq!(refs(&out), vec!["T-1"]);
    assert_eq!(out.trade_refs(), vec!["T-1"]);
    let hit = &out.results[0];
    assert_eq!(hit.kind, "trade");

    let out = f
        .knowledge
        .search_by_condition(&with_mistake("oversized_position"), &c)
        .expect("찾기");
    assert_eq!(refs(&out), vec!["T-2"]);
}

// 검증: D10
#[test]
fn mistake_condition_trims_spaces_on_both_sides() {
    let f = Fixture::new();
    f.add_trade("T-1", "삼성전자 손절 늦음");
    f.add_trade("T-2", "하이닉스 추격 매수");
    let c = catalog(vec![
        (
            trade("T-1", "005930", T0 - DAY, Some(-100)),
            &["  late_stop \n"],
        ),
        (
            trade("T-2", "000660", T0 - DAY, Some(50)),
            &[" chased_entry "],
        ),
    ]);
    let out = f
        .knowledge
        .search_by_condition(&with_mistake("\tlate_stop  "), &c)
        .expect("찾기");
    assert_eq!(refs(&out), vec!["T-1"]);
}

// 검증: D10
#[test]
fn mistake_condition_is_case_sensitive_and_not_a_substring_match() {
    let (f, c) = three_trades();
    for m in ["Late_Stop", "LATE_STOP", "late", "stop", "late_stop_x"] {
        let out = f
            .knowledge
            .search_by_condition(&with_mistake(m), &c)
            .expect("찾기");
        assert!(out.results.is_empty(), "{m:?}가 맞으면 안 된다");
    }
}

// 검증: D10
#[test]
fn trades_without_mistakes_never_match_a_mistake_condition() {
    let f = Fixture::new();
    f.add_trade("T-3", "카카오 계획대로 매도");
    let c = catalog(vec![(trade("T-3", "035720", T0 - DAY, Some(30)), &[])]);
    let out = f
        .knowledge
        .search_by_condition(&with_mistake("late_stop"), &c)
        .expect("찾기");
    assert!(out.results.is_empty());
}

// 검증: D10
#[test]
fn blank_mistake_means_no_mistake_condition() {
    let (f, c) = three_trades();
    for m in ["", "   ", "\n"] {
        let out = f
            .knowledge
            .search_by_condition(&with_mistake(m), &c)
            .expect("찾기");
        assert_eq!(refs(&out), vec!["T-1", "T-2", "T-3"], "{m:?}");
    }
}

// 검증: D10 V22
#[test]
fn mistake_is_and_ed_with_symbol_and_period() {
    let f = Fixture::new();
    f.add_trade("T-1", "삼성전자 손절 늦음");
    f.add_trade("T-4", "하이닉스 손절 늦음");
    f.add_trade("T-5", "삼성전자 예전 손절 늦음");
    f.add_trade("T-6", "삼성전자 추격 매수");
    let c = catalog(vec![
        (trade("T-1", "005930", T0 - DAY, Some(-100)), &["late_stop"]),
        (trade("T-4", "000660", T0 - DAY, Some(-50)), &["late_stop"]),
        (
            trade("T-5", "005930", T0 - 10 * DAY, Some(-70)),
            &["late_stop"],
        ),
        (
            trade("T-6", "005930", T0 - DAY, Some(20)),
            &["chased_entry"],
        ),
    ]);
    let condition = Condition {
        symbol: Some("005930".to_string()),
        from: Some(UnixMillis(T0 - 2 * DAY)),
        to: Some(UnixMillis(T0)),
        mistake: Some("late_stop".to_string()),
        ..any_condition()
    };
    let out = f
        .knowledge
        .search_by_condition(&condition, &c)
        .expect("찾기");
    assert_eq!(refs(&out), vec!["T-1"]);

    let with_pnl = Condition {
        max_pnl: Some((-80).into()),
        mistake: Some("late_stop".to_string()),
        ..any_condition()
    };
    let out = f
        .knowledge
        .search_by_condition(&with_pnl, &c)
        .expect("찾기");
    assert_eq!(refs(&out), vec!["T-1"]);
}

// 검증: D10 M7 V22
#[test]
fn recall_by_condition_uses_the_same_mistake_rule_and_skips_outside_material() {
    let f = Fixture::new();
    f.add_trade("T-1", "삼성전자 손절 늦음");
    f.add(
        KnowledgeKind::Trade,
        KnowledgeSource::External,
        "T-7",
        "바깥에서 들어온 손절 사례",
        T0,
    );
    f.add_trade("T-2", "하이닉스 추격 매수");
    let c = catalog(vec![
        (
            trade("T-1", "005930", T0 - 2 * DAY, Some(-100)),
            &["late_stop"],
        ),
        (trade("T-7", "005930", T0 - DAY, Some(-10)), &["late_stop"]),
        (
            trade("T-2", "000660", T0 - DAY, Some(50)),
            &["chased_entry"],
        ),
    ]);
    let condition = with_mistake("late_stop");

    let explicit = f
        .knowledge
        .search_by_condition(&condition, &c)
        .expect("찾기");
    assert_eq!(refs(&explicit), vec!["T-7", "T-1"]);

    let recalled = f
        .knowledge
        .recall_by_condition(&condition, &c)
        .expect("회상");
    assert_eq!(refs(&recalled), vec!["T-1"]);
}

// 검증: D10
#[test]
fn a_catalog_without_mistakes_matches_nothing_under_a_mistake_condition() {
    let f = Fixture::new();
    f.add_trade("T-1", "삼성전자 손절 늦음");
    f.add_trade("T-2", "하이닉스 추격 매수");
    let c = FakeCatalog(vec![
        trade("T-1", "005930", T0 - DAY, Some(-100)),
        trade("T-2", "000660", T0 - DAY, Some(50)),
    ]);
    assert!(c.mistakes("T-1").is_empty());
    let out = f
        .knowledge
        .search_by_condition(&with_mistake("late_stop"), &c)
        .expect("찾기");
    assert!(out.results.is_empty());
    let out = f
        .knowledge
        .recall_by_condition(&with_mistake("late_stop"), &c)
        .expect("회상");
    assert!(out.results.is_empty());
}
