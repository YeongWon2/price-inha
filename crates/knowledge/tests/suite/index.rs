//! 색인: 조각 400/80, 앞 8,192자, 바뀌면 다시 만들기, 거부, 다시 열기.

use pi_knowledge::{CHUNK_OVERLAP, CHUNK_TOKENS, split_chunks};
use pi_store::{KnowledgeKind, KnowledgeSource};

use crate::common::{Fixture, T0, record, refs, word_lines};

// 검증: M7 D10
#[test]
fn long_text_splits_into_400_word_chunks_overlapping_80() {
    let chunks = split_chunks(&word_lines(1000), CHUNK_TOKENS, CHUNK_OVERLAP);
    let lines: Vec<(u32, u32)> = chunks.iter().map(|c| (c.start_line, c.end_line)).collect();
    assert_eq!(lines, vec![(1, 400), (321, 720), (641, 1000)]);
    assert_eq!(chunks[0].text.split_whitespace().count(), 400);
    assert!(chunks[1].text.starts_with("w0321"));
}

// 검증: M7
#[test]
fn short_text_is_one_chunk_and_empty_text_is_none() {
    let one = split_chunks("삼성전자 매수 메모", CHUNK_TOKENS, CHUNK_OVERLAP);
    assert_eq!(one.len(), 1);
    assert_eq!((one[0].start_line, one[0].end_line), (1, 1));
    assert!(split_chunks("  \n \n", CHUNK_TOKENS, CHUNK_OVERLAP).is_empty());
}

// 검증: M7 D10
#[test]
fn index_stores_chunks_in_order_and_returns_their_ids() {
    let f = Fixture::new();
    let ids = f
        .knowledge
        .index(record(
            KnowledgeKind::Review,
            KnowledgeSource::Mine,
            "R-1",
            &word_lines(1000),
            T0,
        ))
        .expect("색인");
    assert_eq!(ids.len(), 3);
    let stored = f
        .store
        .knowledge_chunks(KnowledgeKind::Review, "R-1")
        .expect("조각");
    assert_eq!(stored.iter().map(|c| c.id).collect::<Vec<_>>(), ids);
    assert_eq!(
        stored.iter().map(|c| c.ordinal).collect::<Vec<_>>(),
        vec![0, 1, 2]
    );
}

// 검증: M7
#[test]
fn only_the_first_8192_chars_are_indexed() {
    let f = Fixture::new();
    let text = format!("머리말표지 {}꼬리말표지", "a".repeat(9000));
    f.add_trade("T-1", &text);
    assert_eq!(refs(&f.find("머리말표지")), vec!["T-1"]);
    assert!(f.find("꼬리말표지").results.is_empty());
}

// 검증: M7
#[test]
fn same_text_keeps_chunk_ids_and_embeddings() {
    let f = Fixture::new();
    let rec = record(
        KnowledgeKind::Trade,
        KnowledgeSource::Mine,
        "T-1",
        "삼성전자 매수 메모",
        T0,
    );
    let first = f.knowledge.index(rec.clone()).expect("처음");
    f.store
        .set_chunk_embedding(first[0], "m", 1, &[0, 0, 0, 0])
        .expect("임베딩");
    let again = f.knowledge.index(rec).expect("다시");
    assert_eq!(again, first);
    assert!(f.store.chunk_embedding(first[0]).expect("읽기").is_some());
}

// 검증: M7
#[test]
fn changed_text_rebuilds_chunks_and_drops_embeddings() {
    let f = Fixture::new();
    let old = f
        .knowledge
        .index(record(
            KnowledgeKind::Trade,
            KnowledgeSource::Mine,
            "T-1",
            "삼성전자 매수 메모",
            T0,
        ))
        .expect("처음");
    f.store
        .set_chunk_embedding(old[0], "m", 1, &[0, 0, 0, 0])
        .expect("임베딩");
    let new = f
        .knowledge
        .index(record(
            KnowledgeKind::Trade,
            KnowledgeSource::Mine,
            "T-1",
            "삼성전자 손절 메모",
            T0,
        ))
        .expect("바뀐 글");
    assert_ne!(new, old);
    assert!(f.store.chunk_embedding(old[0]).expect("읽기").is_none());
    assert_eq!(refs(&f.find("손절")), vec!["T-1"]);
    assert!(f.find("매수").results.is_empty());
}

// 검증: M7
#[test]
fn blank_ref_or_blank_text_is_rejected_and_changes_nothing() {
    let f = Fixture::new();
    f.add_trade("T-1", "삼성전자 매수 메모");
    let no_ref = f.knowledge.index(record(
        KnowledgeKind::Trade,
        KnowledgeSource::Mine,
        "",
        "글",
        T0,
    ));
    assert!(matches!(no_ref, Err(pi_knowledge::Error::InvalidRecord(_))));
    let no_text = f.knowledge.index(record(
        KnowledgeKind::Trade,
        KnowledgeSource::Mine,
        "T-1",
        "  \n ",
        T0,
    ));
    assert!(matches!(
        no_text,
        Err(pi_knowledge::Error::InvalidRecord(_))
    ));
    assert_eq!(refs(&f.find("삼성전자")), vec!["T-1"]);
}

// 검증: D10 M7
#[test]
fn index_survives_reopening_the_store() {
    let f = Fixture::new();
    f.add_trade("T-1", "삼성전자 매수 메모");
    let f = f.reopen();
    assert_eq!(refs(&f.find("삼성전자")), vec!["T-1"]);
    // 다시 열어도 같은 글은 그대로 두고, 이어서 올릴 수 있다.
    f.add_trade("T-2", "삼성전자 손절 메모");
    assert_eq!(f.find("삼성전자").results.len(), 2);
}
