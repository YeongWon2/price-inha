//! 종류별로 넣는 길(4차 2번): 실수 기록 · 시장 메모 · 바깥 자료.

use pi_knowledge::{Error, RecordText, SearchRequest};
use pi_store::{KnowledgeKind, KnowledgeSource, UnixMillis};

use crate::common::{DAY, Fixture, T0, refs};

fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| s.to_string()).collect()
}

fn auto_recall(text: &str) -> SearchRequest {
    SearchRequest {
        auto_recall: true,
        ..SearchRequest::keywords(text)
    }
}

// 검증: M7
#[test]
fn mistakes_record_has_mistake_kind_team_source_and_the_fixed_text_shape() {
    let r = RecordText::mistakes(
        "T-1",
        "005930",
        &strings(&["late_stop", "chased_entry"]),
        "손절 선을 지키지 못했다",
        UnixMillis(T0),
    )
    .expect("실수가 있으면 기록이 있다");
    assert_eq!(r.kind, KnowledgeKind::Mistake);
    assert_eq!(r.source, KnowledgeSource::Team);
    assert_eq!(r.record_ref, "T-1");
    assert_eq!(
        r.text,
        "실수 · 매매 T-1 · 종목 005930\n실수: late_stop, chased_entry\n손절 선을 지키지 못했다"
    );
    assert_eq!(r.at, UnixMillis(T0));
}

// 검증: M7
#[test]
fn mistakes_record_is_none_when_no_mistake_remains_after_trimming() {
    let none: Vec<String> = Vec::new();
    assert_eq!(
        RecordText::mistakes("T-1", "005930", &none, "잘했다", UnixMillis(T0)),
        None
    );
    assert_eq!(
        RecordText::mistakes(
            "T-1",
            "005930",
            &strings(&["", "   ", "\n"]),
            "잘했다",
            UnixMillis(T0)
        ),
        None
    );
}

// 검증: M7
#[test]
fn market_note_keeps_mine_or_team_source_and_text_as_is() {
    for source in [KnowledgeSource::Mine, KnowledgeSource::Team] {
        let r = RecordText::market_note("note-1", source, "반도체 업황 회복 조짐", UnixMillis(T0))
            .expect("메모");
        assert_eq!(r.kind, KnowledgeKind::MarketNote);
        assert_eq!(r.source, source);
        assert_eq!(r.record_ref, "note-1");
        assert_eq!(r.text, "반도체 업황 회복 조짐");
        assert_eq!(r.at, UnixMillis(T0));
    }
}

// 검증: M7
#[test]
fn market_note_refuses_external_source() {
    let r = RecordText::market_note(
        "note-1",
        KnowledgeSource::External,
        "바깥 기사 붙여넣기",
        UnixMillis(T0),
    );
    assert!(matches!(r, Err(Error::InvalidRecord(_))), "{r:?}");
}

// 검증: M7
#[test]
fn external_item_is_an_external_market_note_with_title_summary_link() {
    let r = RecordText::external(
        "news:fake:1",
        "반도체 수출 증가",
        "지난달 반도체 수출이 늘었다",
        "http://127.0.0.1/news/1",
        UnixMillis(T0 - DAY),
    );
    assert_eq!(r.kind, KnowledgeKind::MarketNote);
    assert_eq!(r.source, KnowledgeSource::External);
    assert_eq!(r.record_ref, "news:fake:1");
    assert_eq!(
        r.text,
        "반도체 수출 증가\n지난달 반도체 수출이 늘었다\nhttp://127.0.0.1/news/1"
    );
    assert_eq!(r.at, UnixMillis(T0 - DAY));
}

// 검증: M7
#[test]
fn external_chunks_stay_out_of_auto_recall_but_show_in_explicit_search() {
    let f = Fixture::new();
    f.knowledge
        .index(RecordText::external(
            "news:fake:1",
            "반도체 수출 증가",
            "반도체 수출이 늘었다",
            "http://127.0.0.1/news/1",
            UnixMillis(T0),
        ))
        .expect("색인");
    f.knowledge
        .index(
            RecordText::market_note(
                "note-1",
                KnowledgeSource::Team,
                "반도체 업황 메모",
                UnixMillis(T0),
            )
            .expect("메모"),
        )
        .expect("색인");

    let explicit = f.find("반도체");
    let mut found = refs(&explicit);
    found.sort();
    assert_eq!(found, vec!["news:fake:1", "note-1"]);
    let external = explicit
        .results
        .iter()
        .find(|h| h.record_ref == "news:fake:1")
        .expect("바깥 자료");
    assert_eq!(
        (external.kind.as_str(), external.source.as_str()),
        ("market_note", "external")
    );

    let recalled = f
        .knowledge
        .search(&auto_recall("반도체"), UnixMillis(T0))
        .expect("회상");
    assert_eq!(refs(&recalled), vec!["note-1"]);
}

// 검증: M7
#[test]
fn indexing_the_same_news_twice_keeps_the_same_chunks() {
    let f = Fixture::new();
    let item = || {
        RecordText::external(
            "dart:20261121000001",
            "주요사항 보고",
            "자기주식 취득 결정",
            "http://127.0.0.1/dart/1",
            UnixMillis(T0),
        )
    };
    let first = f.knowledge.index(item()).expect("색인");
    let second = f.knowledge.index(item()).expect("다시 색인");
    assert!(!first.is_empty());
    assert_eq!(first, second);
    let chunks = f
        .store
        .knowledge_chunks(KnowledgeKind::MarketNote, "dart:20261121000001")
        .expect("조각");
    assert_eq!(chunks.len(), first.len());
}

// 검증: M7
#[test]
fn mistakes_record_is_searchable_as_a_mistake_and_recalled() {
    let f = Fixture::new();
    let r = RecordText::mistakes(
        "T-1",
        "005930",
        &strings(&["late_stop"]),
        "손절 늦음",
        UnixMillis(T0),
    )
    .expect("기록");
    f.knowledge.index(r).expect("색인");
    let out = f
        .knowledge
        .search(
            &SearchRequest {
                kinds: vec![KnowledgeKind::Mistake],
                ..auto_recall("late_stop")
            },
            UnixMillis(T0),
        )
        .expect("회상");
    assert_eq!(refs(&out), vec!["T-1"]);
    assert_eq!(
        (out.results[0].kind.as_str(), out.results[0].source.as_str()),
        ("mistake", "team")
    );
}
