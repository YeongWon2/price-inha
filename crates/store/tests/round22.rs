//! store 22차 시험(백로그 R 묶음 C): 모델별 조각 벡터 훑기 — knowledge 하이브리드 검색이 쓰는 조회.
//! 판은 그대로(새 표 없음). 옛 f32 줄과 정수 양자화 줄을 함께 읽고, 다른 모델은 빼고, 번호 순서로 쪽을 넘긴다.

mod common;

use common::{t, temp_store};
use pi_store::{
    ChunkId, KnowledgeKind, KnowledgeSource, NewKnowledgeRecord, SCHEMA_VERSION, Store,
};

fn record(
    kind: KnowledgeKind,
    source: KnowledgeSource,
    key: &str,
    chunks: &[&str],
) -> NewKnowledgeRecord {
    NewKnowledgeRecord {
        kind,
        source,
        record_ref: key.to_string(),
        chunks: chunks.iter().map(|c| (*c).to_string()).collect(),
        at: t(5),
    }
}

/// 조각 넷: 매매(f32 · 모델 m) · 복기(q8 · 모델 m) · 바깥 자료(q8 · 모델 다른 것) · 실수(임베딩 없음).
fn seeded(store: &Store) -> Vec<ChunkId> {
    let trade = store
        .index_record(record(
            KnowledgeKind::Trade,
            KnowledgeSource::Team,
            "T-1",
            &["삼성전자 매수"],
        ))
        .unwrap()[0];
    let review = store
        .index_record(record(
            KnowledgeKind::Review,
            KnowledgeSource::Mine,
            "T-1",
            &["손절 늦음"],
        ))
        .unwrap()[0];
    let external = store
        .index_record(record(
            KnowledgeKind::MarketNote,
            KnowledgeSource::External,
            "news:1",
            &["금리 인상 뉴스"],
        ))
        .unwrap()[0];
    let mistake = store
        .index_record(record(
            KnowledgeKind::Mistake,
            KnowledgeSource::Team,
            "T-1",
            &["실수 late_stop"],
        ))
        .unwrap()[0];
    store
        .set_chunk_embedding(trade, "m", 1, &1.0_f32.to_le_bytes())
        .unwrap();
    store
        .set_chunk_embedding_q8(review, "m", 3, &[0, 0, 128, 63, 1, 254, 127])
        .unwrap();
    store
        .set_chunk_embedding_q8(external, "other", 3, &[0, 0, 128, 63, 1, 2, 3])
        .unwrap();
    vec![trade, review, external, mistake]
}

// 검증: D10 M7
#[test]
fn 그_모델의_임베딩이_있는_조각만_번호_순서로_조각과_함께_준다() {
    let (_dir, _path, store) = temp_store();
    let ids = seeded(&store);
    let found = store.chunk_vectors("m", None, 10).unwrap();
    let got: Vec<ChunkId> = found.iter().map(|v| v.chunk.id).collect();
    assert_eq!(got, vec![ids[0], ids[1]], "모델 m의 매매 · 복기만");

    let trade = &found[0];
    assert_eq!(trade.chunk.kind, KnowledgeKind::Trade);
    assert_eq!(trade.chunk.source, KnowledgeSource::Team);
    assert_eq!(trade.chunk.record_ref, "T-1");
    assert_eq!(trade.chunk.text, "삼성전자 매수");
    assert_eq!(trade.chunk.updated_at, t(5));
    assert_eq!(trade.embedding.model, "m");
    assert_eq!(trade.embedding.dims, 1);
    assert!(!trade.embedding.quantized, "옛 f32 줄");
    assert_eq!(trade.embedding.bytes, 1.0_f32.to_le_bytes().to_vec());

    let review = &found[1];
    assert_eq!(review.chunk.kind, KnowledgeKind::Review);
    assert!(review.embedding.quantized, "정수 양자화 줄");
    assert_eq!(review.embedding.dims, 3);
    assert_eq!(review.embedding.bytes, vec![0, 0, 128, 63, 1, 254, 127]);

    let other = store.chunk_vectors("other", None, 10).unwrap();
    assert_eq!(other.len(), 1);
    assert_eq!(other[0].chunk.id, ids[2]);
    assert_eq!(other[0].chunk.source, KnowledgeSource::External);
    assert!(store.chunk_vectors("none", None, 10).unwrap().is_empty());
}

// 검증: D10
#[test]
fn 쪽을_넘겨_끝까지_읽고_개수_0이면_빈_목록이다() {
    let (_dir, _path, store) = temp_store();
    let ids = seeded(&store);
    let first = store.chunk_vectors("m", None, 1).unwrap();
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].chunk.id, ids[0]);
    let second = store
        .chunk_vectors("m", Some(first[0].chunk.id), 1)
        .unwrap();
    assert_eq!(second.len(), 1);
    assert_eq!(second[0].chunk.id, ids[1]);
    assert!(
        store
            .chunk_vectors("m", Some(second[0].chunk.id), 1)
            .unwrap()
            .is_empty()
    );
    assert!(store.chunk_vectors("m", None, 0).unwrap().is_empty());
}

// 검증: M7
#[test]
fn 기록을_다시_올려_조각이_바뀌면_옛_벡터는_나오지_않고_판은_그대로다() {
    let (_dir, _path, store) = temp_store();
    seeded(&store);
    store
        .index_record(record(
            KnowledgeKind::Review,
            KnowledgeSource::Mine,
            "T-1",
            &["손절 기준 바꿈"],
        ))
        .unwrap();
    let found = store.chunk_vectors("m", None, 10).unwrap();
    assert_eq!(found.len(), 1, "바뀐 복기의 옛 임베딩은 함께 지워졌다");
    assert_eq!(found[0].chunk.kind, KnowledgeKind::Trade);
    assert_eq!(SCHEMA_VERSION, 12, "판을 올리지 않는다");
}
