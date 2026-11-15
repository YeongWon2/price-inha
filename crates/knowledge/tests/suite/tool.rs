//! 검색 도구가 돌려주는 모양: 인용, 글 조각, JSON, 뜻 검색 상태.

use pi_knowledge::{SNIPPET_CHARS, SemanticStatus};
use pi_store::{KnowledgeKind, KnowledgeSource};

use crate::common::{FakeEmbedder, Fixture, T0, word_lines};

// 검증: D10
#[test]
fn a_one_line_record_cites_a_single_line() {
    let f = Fixture::new();
    f.add_trade("T-1", "삼성전자 메모");
    let hit = f.find("삼성전자").results.remove(0);
    assert_eq!(hit.citation, "trade/T-1#L1");
    assert_eq!((hit.start_line, hit.end_line), (1, 1));
}

// 검증: D10
#[test]
fn a_long_record_cites_the_line_range_of_the_matching_chunk() {
    let f = Fixture::new();
    f.add(
        KnowledgeKind::Review,
        KnowledgeSource::Mine,
        "R-9",
        &word_lines(1000),
        T0,
    );
    let hit = f.find("w0500").results.remove(0);
    assert_eq!((hit.start_line, hit.end_line), (321, 720));
    assert_eq!(hit.citation, "review/R-9#L321-L720");
    assert_eq!(hit.record_ref, "R-9");
}

// 검증: D10
#[test]
fn snippet_is_cut_to_700_chars_and_ends_with_the_source() {
    let f = Fixture::new();
    f.add_trade("T-1", &format!("삼성전자 {}", "가".repeat(1500)));
    let hit = f.find("삼성전자").results.remove(0);
    let suffix = "\n\nSource: trade/T-1#L1";
    assert!(hit.snippet.ends_with(suffix));
    let body = &hit.snippet[..hit.snippet.len() - suffix.len()];
    assert_eq!(body.chars().count(), SNIPPET_CHARS);
    assert!(body.starts_with("삼성전자"));
}

// 검증: D10
#[test]
fn a_short_chunk_is_not_padded() {
    let f = Fixture::new();
    f.add_trade("T-1", "삼성전자 메모");
    let hit = f.find("삼성전자").results.remove(0);
    assert_eq!(hit.snippet, "삼성전자 메모\n\nSource: trade/T-1#L1");
}

// 검증: D10 V22
#[test]
fn the_result_serializes_to_the_tool_shape() {
    let f = Fixture::new();
    f.add(
        KnowledgeKind::Mistake,
        KnowledgeSource::Team,
        "X-1",
        "삼성전자 실수 기록",
        T0,
    );
    let json = serde_json::to_value(f.find("삼성전자")).expect("JSON");
    assert_eq!(json["mode"], "keyword");
    assert_eq!(json["relaxed"], false);
    assert_eq!(json["semantic"]["status"], "unavailable");
    assert_eq!(json["semantic"]["code"], "no_embedder");
    let hit = &json["results"][0];
    assert_eq!(hit["kind"], "mistake");
    assert_eq!(hit["source"], "team");
    assert_eq!(hit["record_ref"], "X-1");
    assert_eq!(hit["citation"], "mistake/X-1#L1");
    assert!(hit["score_micros"].is_i64());
    assert!(hit["snippet"].is_string());
}

// 검증: V23 D10
#[test]
fn keyword_results_say_whether_semantic_search_is_available() {
    let plain = Fixture::new();
    plain.add_trade("T-1", "삼성전자 메모");
    assert!(matches!(
        plain.find("삼성전자").semantic,
        SemanticStatus::Unavailable { .. }
    ));
    let with = Fixture::with_embedder(FakeEmbedder::good());
    with.add_trade("T-1", "삼성전자 메모");
    assert_eq!(with.find("삼성전자").semantic, SemanticStatus::Available);
}
