//! 키워드 검색: 한글 두 글자, AND → OR, 걸러 내기, 개수, 최신성, MMR, 문턱.

use pi_knowledge::{Knowledge, SearchConfig, SearchMode, SearchRequest};
use pi_store::{KnowledgeKind, KnowledgeSource, UnixMillis};

use crate::common::{DAY, Fixture, T0, refs, test_config};

// 검증: D10
#[test]
fn two_letter_korean_words_are_found() {
    let f = Fixture::new();
    f.add_trade("T-1", "오늘 삼성전자 매수 진행");
    f.add_trade("T-2", "기준을 넘어 손절 처리");
    assert_eq!(refs(&f.find("매수")), vec!["T-1"]);
    assert_eq!(refs(&f.find("손절")), vec!["T-2"]);
    assert_eq!(f.find("매수").results[0].score_micros, None);
}

// 검증: D10
#[test]
fn three_letter_words_are_scored_between_zero_and_one() {
    let f = Fixture::new();
    f.add_trade("T-1", "오늘 삼성전자 매수 진행");
    let out = f.find("삼성전자");
    assert_eq!(out.mode, SearchMode::Keyword);
    let score = out.results[0].score_micros.expect("점수");
    assert!(score > 0 && score <= 1_000_000, "점수 {score}");
}

// 검증: D10
#[test]
fn every_word_must_match() {
    let f = Fixture::new();
    f.add_trade("T-1", "삼성전자 매수 진행");
    f.add_trade("T-2", "삼성전자 손절 처리");
    let out = f.find("삼성전자 손절");
    assert_eq!(refs(&out), vec!["T-2"]);
    assert!(!out.relaxed);
}

// 검증: D10
#[test]
fn zero_hits_relax_and_to_or() {
    let f = Fixture::new();
    f.add_trade("T-1", "삼성전자 매수 진행");
    f.add_trade("T-2", "하이닉스 손절 처리");
    let out = f.find("삼성전자 손절");
    assert!(out.relaxed);
    let mut got = refs(&out);
    got.sort();
    assert_eq!(got, vec!["T-1", "T-2"]);
}

// 검증: D10
#[test]
fn nothing_matches_gives_an_empty_result() {
    let f = Fixture::new();
    f.add_trade("T-1", "삼성전자 매수 진행");
    let out = f.find("없는낱말");
    assert!(out.results.is_empty());
}

// 검증: D10
#[test]
fn english_ignores_case() {
    let f = Fixture::new();
    f.add_trade("T-1", "Rebalance the portfolio");
    assert_eq!(refs(&f.find("REBALANCE")), vec!["T-1"]);
}

// 검증: D10 M7
#[test]
fn kinds_and_sources_filter_the_result() {
    let f = Fixture::new();
    f.add(
        KnowledgeKind::Trade,
        KnowledgeSource::Mine,
        "T-1",
        "삼성전자 메모",
        T0,
    );
    f.add(
        KnowledgeKind::Review,
        KnowledgeSource::Team,
        "R-1",
        "삼성전자 복기",
        T0,
    );
    f.add(
        KnowledgeKind::Mistake,
        KnowledgeSource::Mine,
        "X-1",
        "삼성전자 실수",
        T0,
    );
    let mut req = SearchRequest::keywords("삼성전자");
    req.kinds = vec![KnowledgeKind::Review, KnowledgeKind::Mistake];
    let out = f.knowledge.search(&req, UnixMillis(T0)).expect("검색");
    let mut got = refs(&out);
    got.sort();
    assert_eq!(got, vec!["R-1", "X-1"]);
    let mut req = SearchRequest::keywords("삼성전자");
    req.sources = vec![KnowledgeSource::Team];
    let out = f.knowledge.search(&req, UnixMillis(T0)).expect("검색");
    assert_eq!(refs(&out), vec!["R-1"]);
}

// 검증: D10
#[test]
fn default_result_count_is_six_and_request_can_lower_it() {
    let f = Fixture::new();
    for i in 0..10 {
        f.add_trade(&format!("T-{i}"), &format!("삼성전자 메모 {i}번"));
    }
    assert_eq!(f.find("삼성전자").results.len(), 6);
    let mut req = SearchRequest::keywords("삼성전자");
    req.max = Some(3);
    assert_eq!(
        f.knowledge
            .search(&req, UnixMillis(T0))
            .expect("검색")
            .results
            .len(),
        3
    );
    req.max = Some(0);
    assert!(
        f.knowledge
            .search(&req, UnixMillis(T0))
            .expect("검색")
            .results
            .is_empty()
    );
}

// 검증: D10
#[test]
fn blank_query_is_an_empty_result_not_an_error() {
    let f = Fixture::new();
    f.add_trade("T-1", "삼성전자 매수 진행");
    assert!(f.find("   ").results.is_empty());
    assert!(f.find("").results.is_empty());
}

// 검증: D10
#[test]
fn defaults_follow_the_original_numbers() {
    let c = SearchConfig::default();
    assert_eq!(c.max_results, 6);
    assert_eq!(c.candidate_multiplier, 4);
    assert_eq!(c.half_life_days, 30);
    assert_eq!(c.min_score_micros, 350_000);
    assert_eq!(c.mmr_lambda_micros, 700_000);
}

// 검증: D10
#[test]
fn older_records_rank_lower_with_a_30_day_half_life() {
    let f = Fixture::new();
    f.add(
        KnowledgeKind::Trade,
        KnowledgeSource::Mine,
        "OLD",
        "삼성전자 매수 메모",
        T0 - 30 * DAY,
    );
    f.add(
        KnowledgeKind::Trade,
        KnowledgeSource::Mine,
        "NEW",
        "삼성전자 매수 메모",
        T0,
    );
    let out = f.find("삼성전자");
    assert_eq!(refs(&out), vec!["NEW", "OLD"]);
    let new = out.results[0].score_micros.expect("점수");
    let old = out.results[1].score_micros.expect("점수");
    assert!((new - 2 * old).abs() <= 2, "새것 {new}, 30일 묵은 것 {old}");
}

// 검증: D10
#[test]
fn mmr_with_lambda_one_keeps_score_order() {
    let f = mmr_fixture(1_000_000);
    let mut req = SearchRequest::keywords("삼성전자 손절");
    req.max = Some(2);
    let out = f.knowledge.search(&req, UnixMillis(T0)).expect("검색");
    assert_eq!(refs(&out), vec!["M-1", "M-2"]);
}

// 검증: D10
#[test]
fn mmr_with_a_low_lambda_prefers_a_different_record_over_a_near_copy() {
    let f = mmr_fixture(300_000);
    let mut req = SearchRequest::keywords("삼성전자 손절");
    req.max = Some(2);
    let out = f.knowledge.search(&req, UnixMillis(T0)).expect("검색");
    assert_eq!(refs(&out), vec!["M-1", "M-3"]);
}

/// 같은 글 둘(M-1, M-2)과 겹침이 적은 더 긴 글 하나(M-3).
fn mmr_fixture(lambda: i64) -> Fixture {
    let config = SearchConfig {
        mmr_lambda_micros: lambda,
        ..test_config()
    };
    let f = Fixture::with_config(config);
    f.add_trade("M-1", "삼성전자 손절 규칙 위반");
    f.add_trade("M-2", "삼성전자 손절 규칙 위반");
    f.add_trade("M-3", "삼성전자 손절 기준 점검 메모 추가 내용");
    f
}

// 검증: D10
#[test]
fn a_threshold_above_every_score_does_not_empty_the_result() {
    let config = SearchConfig {
        min_score_micros: i64::MAX,
        ..test_config()
    };
    let f = Fixture::with_config(config);
    f.add_trade("T-1", "삼성전자 매수 진행");
    assert_eq!(refs(&f.find("삼성전자")), vec!["T-1"]);
}

// 검증: V23
#[test]
fn keyword_search_never_needs_an_embedder() {
    let f = Fixture::new();
    f.add_trade("T-1", "삼성전자 매수 진행");
    let again = Knowledge::new(f.store.clone(), test_config());
    let out = again
        .search(&SearchRequest::keywords("삼성전자"), UnixMillis(T0))
        .expect("검색");
    assert_eq!(refs(&out), vec!["T-1"]);
}
