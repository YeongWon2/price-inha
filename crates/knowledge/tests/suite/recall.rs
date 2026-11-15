//! 출처 등급(M7)과 지난 매매 꺼내 보기(V22).

use pi_knowledge::SearchRequest;
use pi_store::{KnowledgeKind, KnowledgeSource, UnixMillis};

use crate::common::{Fixture, T0, refs, word_lines};

// 검증: M7
#[test]
fn auto_recall_never_returns_outside_material() {
    let f = Fixture::new();
    f.add(
        KnowledgeKind::MarketNote,
        KnowledgeSource::External,
        "N-1",
        "삼성전자 뉴스 요약",
        T0,
    );
    f.add(
        KnowledgeKind::MarketNote,
        KnowledgeSource::Mine,
        "N-2",
        "삼성전자 내 메모",
        T0,
    );
    let mut req = SearchRequest::keywords("삼성전자");
    req.auto_recall = true;
    let out = f.knowledge.search(&req, UnixMillis(T0)).expect("검색");
    assert_eq!(refs(&out), vec!["N-2"]);
}

// 검증: M7
#[test]
fn auto_recall_keeps_outside_material_out_even_when_asked_for() {
    let f = Fixture::new();
    f.add(
        KnowledgeKind::MarketNote,
        KnowledgeSource::External,
        "N-1",
        "삼성전자 뉴스 요약",
        T0,
    );
    let mut req = SearchRequest::keywords("삼성전자");
    req.auto_recall = true;
    req.sources = vec![KnowledgeSource::External];
    let out = f.knowledge.search(&req, UnixMillis(T0)).expect("검색");
    assert!(out.results.is_empty());
}

// 검증: M7
#[test]
fn an_explicit_search_can_read_outside_material() {
    let f = Fixture::new();
    f.add(
        KnowledgeKind::MarketNote,
        KnowledgeSource::External,
        "N-1",
        "삼성전자 뉴스 요약",
        T0,
    );
    let out = f.find("삼성전자");
    assert_eq!(refs(&out), vec!["N-1"]);
    assert_eq!(out.results[0].source, "external");
}

// 검증: M7
#[test]
fn auto_recall_keeps_my_and_team_records() {
    let f = Fixture::new();
    f.add(
        KnowledgeKind::Review,
        KnowledgeSource::Mine,
        "R-1",
        "삼성전자 복기",
        T0,
    );
    f.add(
        KnowledgeKind::Review,
        KnowledgeSource::Team,
        "R-2",
        "삼성전자 복기",
        T0,
    );
    let mut req = SearchRequest::keywords("삼성전자");
    req.auto_recall = true;
    let out = f.knowledge.search(&req, UnixMillis(T0)).expect("검색");
    let mut got = refs(&out);
    got.sort();
    assert_eq!(got, vec!["R-1", "R-2"]);
}

// 검증: V22 M7
#[test]
fn recalled_trades_are_listed_once_each_in_rank_order() {
    let f = Fixture::new();
    f.add_trade("T-1", "삼성전자 매수 진행");
    f.add(
        KnowledgeKind::Review,
        KnowledgeSource::Mine,
        "R-1",
        "삼성전자 복기 메모",
        T0,
    );
    // 조각이 여럿인 매매: 같은 매매가 두 번 나오면 한 번만 센다.
    let long = word_lines(500)
        .replace("w0300", "삼성전자")
        .replace("w0450", "삼성전자");
    f.add_trade("T-2", &long);
    let out = f.find("삼성전자");
    let trades = out.trade_refs();
    assert_eq!(trades.len(), 2);
    assert!(trades.contains(&"T-1".to_string()) && trades.contains(&"T-2".to_string()));
    assert!(!trades.contains(&"R-1".to_string()));
}

// 검증: V22
#[test]
fn no_similar_trade_means_no_recalled_trades() {
    let f = Fixture::new();
    f.add(
        KnowledgeKind::Review,
        KnowledgeSource::Mine,
        "R-1",
        "삼성전자 복기 메모",
        T0,
    );
    assert!(f.find("삼성전자").trade_refs().is_empty());
    assert!(f.find("없는낱말").trade_refs().is_empty());
}
