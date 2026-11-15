//! 임베딩 포트: 없을 때의 길(V23)과 있을 때 바이트로 두기.

use pi_knowledge::{Error, SemanticStatus};
use pi_store::{ChunkId, KnowledgeKind, UnixMillis};

use crate::common::{FakeEmbedder, Fixture, T0};

// 검증: V23 D10
#[tokio::test]
async fn without_an_embedder_semantic_search_is_unavailable_not_an_error() {
    let f = Fixture::new();
    f.add_trade("T-1", "삼성전자 매수 진행");
    let out = f
        .knowledge
        .semantic_search("삼성전자", UnixMillis(T0))
        .await
        .expect("결과");
    assert!(out.results.is_empty());
    match out.semantic {
        SemanticStatus::Unavailable {
            warning,
            action,
            code,
        } => {
            assert_eq!(code, "no_embedder");
            assert!(!warning.is_empty() && !action.is_empty());
        }
        SemanticStatus::Available => panic!("임베딩이 없는데 쓸 수 있다고 한다"),
    }
}

// 검증: V23
#[tokio::test]
async fn without_an_embedder_embed_missing_does_nothing() {
    let f = Fixture::new();
    f.add_trade("T-1", "삼성전자 매수 진행");
    let report = f.knowledge.embed_missing(10).await.expect("결과");
    assert!(report.unavailable);
    assert_eq!(report.embedded, 0);
    let ids = f.store.chunks_missing_embedding("any", 10).expect("목록");
    assert_eq!(ids.len(), 1);
}

// 검증: V23 D10
#[tokio::test]
async fn keyword_search_and_indexing_make_no_outside_request() {
    let embedder = FakeEmbedder::good();
    let f = Fixture::with_embedder(embedder.clone());
    f.add_trade("T-1", "삼성전자 매수 진행");
    f.find("삼성전자");
    f.find("매수");
    assert_eq!(embedder.calls(), 0);
}

// 검증: V23
#[tokio::test]
async fn embed_missing_stores_little_endian_f32_bytes_with_model_and_dims() {
    let embedder = FakeEmbedder::good();
    let f = Fixture::with_embedder(embedder.clone());
    f.add_trade("T-1", "삼성전자 매수 진행");
    f.add_trade("T-2", "하이닉스 손절 처리");
    let report = f.knowledge.embed_missing(10).await.expect("임베딩");
    assert_eq!((report.embedded, report.unavailable), (2, false));
    let chunk = f
        .store
        .knowledge_chunks(KnowledgeKind::Trade, "T-1")
        .expect("조각")
        .remove(0);
    let emb = f
        .store
        .chunk_embedding(chunk.id)
        .expect("읽기")
        .expect("있다");
    assert_eq!(emb.model, FakeEmbedder::MODEL);
    assert_eq!(emb.dims, 3);
    assert_eq!(emb.bytes, FakeEmbedder::bytes_of(&chunk.text));
    assert!(
        f.store
            .chunks_missing_embedding(FakeEmbedder::MODEL, 10)
            .expect("목록")
            .is_empty()
    );
}

// 검증: V23
#[tokio::test]
async fn embed_missing_skips_done_chunks_and_calls_the_port_at_most_once() {
    let embedder = FakeEmbedder::good();
    let f = Fixture::with_embedder(embedder.clone());
    f.add_trade("T-1", "삼성전자 매수 진행");
    f.knowledge.embed_missing(10).await.expect("첫");
    assert_eq!(embedder.calls(), 1);
    let again = f.knowledge.embed_missing(10).await.expect("둘째");
    assert_eq!(again.embedded, 0);
    assert_eq!(embedder.calls(), 1);
}

// 검증: V23 M7
#[tokio::test]
async fn embed_missing_respects_max_and_new_text_needs_new_embeddings() {
    let embedder = FakeEmbedder::good();
    let f = Fixture::with_embedder(embedder.clone());
    f.add_trade("T-1", "삼성전자 매수 진행");
    f.add_trade("T-2", "하이닉스 손절 처리");
    assert_eq!(
        f.knowledge.embed_missing(1).await.expect("하나").embedded,
        1
    );
    assert_eq!(
        f.knowledge
            .embed_missing(10)
            .await
            .expect("나머지")
            .embedded,
        1
    );
    f.add_trade("T-1", "삼성전자 매수를 다시 썼다");
    assert_eq!(
        f.knowledge
            .embed_missing(10)
            .await
            .expect("바뀐 것")
            .embedded,
        1
    );
}

// 검증: V23
#[tokio::test]
async fn a_failing_embedder_stores_nothing_and_reports_the_error() {
    let embedder = FakeEmbedder::failing();
    let f = Fixture::with_embedder(embedder.clone());
    f.add_trade("T-1", "삼성전자 매수 진행");
    let err = f
        .knowledge
        .embed_missing(10)
        .await
        .expect_err("실패해야 한다");
    assert!(matches!(err, Error::Embedder(_)));
    assert_eq!(
        f.store
            .chunks_missing_embedding(FakeEmbedder::MODEL, 10)
            .expect("목록")
            .len(),
        1
    );
}

// 검증: V23
#[tokio::test]
async fn an_embedder_that_breaks_its_promise_on_dims_stores_nothing() {
    let embedder = FakeEmbedder::wrong_dims();
    let f = Fixture::with_embedder(embedder);
    f.add_trade("T-1", "삼성전자 매수 진행");
    let err = f
        .knowledge
        .embed_missing(10)
        .await
        .expect_err("차원이 틀리다");
    assert!(matches!(err, Error::Embedder(_)));
    assert!(f.store.chunk_embedding(ChunkId(1)).expect("읽기").is_none());
}
