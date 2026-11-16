//! 2차(후보 일괄 처리): 조각 메타가 빠진 경우의 복구, 지운 조각의 메타 정리, 조건 검색의 자동 회상 거르기.

use crate::common::*;
use pi_knowledge::{CHUNK_OVERLAP, CHUNK_TOKENS, split_chunks};
use pi_store::{KnowledgeKind, KnowledgeSource, NewKnowledgeRecord, UnixMillis};

/// 조각만 저장소에 올리고 줄 범위 메타는 남기지 않는다(조각 저장과 메타 저장 사이에 꺼진 경우).
fn index_without_meta(f: &Fixture, record_ref: &str, text: &str) {
    let chunks = split_chunks(text, CHUNK_TOKENS, CHUNK_OVERLAP);
    f.store
        .index_record(NewKnowledgeRecord {
            kind: KnowledgeKind::Trade,
            source: KnowledgeSource::Mine,
            record_ref: record_ref.to_string(),
            chunks: chunks.into_iter().map(|c| c.text).collect(),
            at: UnixMillis(T0),
        })
        .expect("조각만 올리기");
}

fn meta_of(f: &Fixture, id: i64) -> String {
    f.store
        .knowledge_meta(&format!("chunk.{id}"))
        .expect("메타 읽기")
        .unwrap_or_default()
}

// 검증: D10 V23
#[tokio::test]
async fn reindexing_the_same_text_repairs_missing_chunk_meta() {
    let embedder = FakeEmbedder::good();
    let f = Fixture::with_embedder(embedder.clone());
    index_without_meta(&f, "T-1", "삼성전자 매수 진행");

    f.add_trade("T-1", "삼성전자 매수 진행");
    let report = f.knowledge.embed_missing(10).await.expect("임베딩");

    assert_eq!(report.embedded, 1);
    let chunk = f
        .store
        .knowledge_chunks(KnowledgeKind::Trade, "T-1")
        .expect("조각")
        .remove(0);
    assert!(f.store.chunk_embedding(chunk.id).expect("읽기").is_some());
}

// 검증: D10 V23
#[tokio::test]
async fn chunks_without_meta_do_not_block_embedding_of_others() {
    let embedder = FakeEmbedder::good();
    let f = Fixture::with_embedder(embedder.clone());
    index_without_meta(&f, "X-1", "메타 없는 조각 하나");
    index_without_meta(&f, "X-2", "메타 없는 조각 둘");
    f.add_trade("T-1", "하이닉스 손절 처리");

    let report = f.knowledge.embed_missing(1).await.expect("임베딩");

    assert_eq!(report.embedded, 1);
    let chunk = f
        .store
        .knowledge_chunks(KnowledgeKind::Trade, "T-1")
        .expect("조각")
        .remove(0);
    assert!(f.store.chunk_embedding(chunk.id).expect("읽기").is_some());
}

// 검증: M7
#[test]
fn replaced_chunks_leave_no_line_meta_behind() {
    let f = Fixture::new();
    f.add_trade("T-1", &word_lines(900));
    let old: Vec<i64> = f
        .store
        .knowledge_chunks(KnowledgeKind::Trade, "T-1")
        .expect("조각")
        .iter()
        .map(|c| c.id.0)
        .collect();
    assert!(old.len() >= 2, "조각이 여럿이어야 시험이 뜻을 가진다");

    f.add_trade("T-1", "짧게 바뀐 글");
    let new: Vec<i64> = f
        .store
        .knowledge_chunks(KnowledgeKind::Trade, "T-1")
        .expect("조각")
        .iter()
        .map(|c| c.id.0)
        .collect();

    for id in old.iter().filter(|id| !new.contains(id)) {
        assert_eq!(meta_of(&f, *id), "", "지운 조각 {id}의 메타가 남았다");
    }
    for id in &new {
        assert!(!meta_of(&f, *id).is_empty(), "새 조각 {id}의 메타는 있다");
    }
}

// 검증: M7
#[test]
fn recall_by_condition_leaves_out_outside_material() {
    let f = Fixture::new();
    f.add_trade("T-1", "삼성전자 매수 기록");
    f.add(
        KnowledgeKind::Trade,
        KnowledgeSource::External,
        "T-9",
        "바깥에서 가져온 삼성전자 매매 글",
        T0,
    );
    let catalog = FakeCatalog(vec![
        trade("T-1", "005930", T0, Some(10)),
        trade("T-9", "005930", T0 + DAY, Some(10)),
    ]);

    let explicit = f
        .knowledge
        .search_by_condition(&any_condition(), &catalog)
        .expect("명시 검색");
    let recalled = f
        .knowledge
        .recall_by_condition(&any_condition(), &catalog)
        .expect("자동 회상");

    assert_eq!(refs(&explicit), vec!["T-9", "T-1"]);
    assert_eq!(refs(&recalled), vec!["T-1"]);
    assert!(recalled.results.iter().all(|h| h.source != "external"));
}
