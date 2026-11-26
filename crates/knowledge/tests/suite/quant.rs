//! (2차, 결정 31) 임베딩 줄이기: 정수 양자화 · 되살린 코사인 순위 · 옛 f32 줄 읽기 · 오래된 바깥 자료를 요약으로.

use pi_knowledge::{RecordText, cosine, dequantize, quantize};
use pi_store::{ChunkId, KnowledgeKind, KnowledgeSource, UnixMillis};

use crate::common::{DAY, FakeEmbedder, Fixture, T0};

/// 시험용 고정 난수(선형 합동).
struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> f32 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        ((self.0 >> 40) as f32 / (1u64 << 24) as f32) * 2.0 - 1.0
    }

    fn vector(&mut self, dims: usize) -> Vec<f32> {
        (0..dims).map(|_| self.next()).collect()
    }
}

fn top(query: &[f32], vectors: &[Vec<f32>], k: usize) -> Vec<usize> {
    let mut scored: Vec<(usize, f32)> = vectors
        .iter()
        .enumerate()
        .map(|(i, v)| (i, cosine(query, v)))
        .collect();
    scored.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
    scored.into_iter().take(k).map(|(i, _)| i).collect()
}

// 검증: D10 M7
#[test]
fn 양자화는_크기_값_하나와_차원마다_1바이트이고_되살리면_가깝다() {
    let v = vec![0.5_f32, -1.25, 0.0, 2.0, -0.01];
    let bytes = quantize(&v);
    assert_eq!(bytes.len(), v.len() + 4);
    let back = dequantize(&bytes).expect("되살린다");
    assert_eq!(back.len(), v.len());
    let step = 2.0 / 127.0;
    for (a, b) in v.iter().zip(&back) {
        assert!((a - b).abs() <= step, "{a} ≈ {b}");
    }
    let zero = quantize(&[0.0, 0.0]);
    assert_eq!(dequantize(&zero).expect("0 벡터"), vec![0.0, 0.0]);
    assert_eq!(dequantize(&[1, 2]), None);
    assert!(cosine(&[1.0, 0.0], &[0.0, 0.0]).abs() < f32::EPSILON);
    assert!((cosine(&[1.0, 1.0], &[2.0, 2.0]) - 1.0).abs() < 1e-6);
}

// 검증: D10 M7
#[test]
fn 양자화해도_코사인_순위의_상위가_거의_같다() {
    let mut rng = Lcg(7);
    let dims = 64;
    let vectors: Vec<Vec<f32>> = (0..300).map(|_| rng.vector(dims)).collect();
    let restored: Vec<Vec<f32>> = vectors
        .iter()
        .map(|v| dequantize(&quantize(v)).expect("되살린다"))
        .collect();
    let mut overlap = 0;
    let queries = 20;
    for _ in 0..queries {
        let q = rng.vector(dims);
        let exact = top(&q, &vectors, 10);
        let approx = top(&q, &restored, 10);
        let same = exact.iter().filter(|i| approx.contains(i)).count();
        assert!(same >= 8, "상위 10개 가운데 {same}개만 겹친다");
        assert!(
            approx[..3].contains(&exact[0]),
            "정확한 1등은 양자화한 상위 3 안에 있다"
        );
        overlap += same;
    }
    assert!(
        overlap * 100 >= queries * 10 * 95,
        "평균 95% 이상 겹친다: {overlap}"
    );
}

// 검증: D10 M7
#[tokio::test]
async fn 새로_만든_임베딩은_정수로_줄여_두고_되살려_읽는다() {
    let f = Fixture::with_embedder(FakeEmbedder::good());
    f.add_trade("T-1", "삼성전자 매수 진행");
    f.knowledge.embed_missing(10).await.expect("임베딩");
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
    assert!(emb.quantized);
    assert_eq!(emb.bytes.len(), 3 + 4);
    let want = FakeEmbedder::vector_of(&chunk.text);
    let got = f
        .knowledge
        .stored_vector(chunk.id)
        .expect("읽기")
        .expect("있다");
    let scale = want.iter().fold(0.0_f32, |m, v| m.max(v.abs())) / 127.0;
    for (a, b) in want.iter().zip(&got) {
        assert!((a - b).abs() <= scale, "{a} ≈ {b}");
    }
}

// 검증: D10 M7
#[tokio::test]
async fn 옛_f32_줄은_그대로_읽고_다시_넣으면_정수로_바뀐다() {
    let f = Fixture::with_embedder(FakeEmbedder::good());
    f.add_trade("T-1", "옛 매매 기록");
    let id = f
        .store
        .knowledge_chunks(KnowledgeKind::Trade, "T-1")
        .expect("조각")[0]
        .id;
    let old = vec![0.25_f32, -0.5, 1.0];
    let bytes: Vec<u8> = old.iter().flat_map(|v| v.to_le_bytes()).collect();
    f.store
        .set_chunk_embedding(id, FakeEmbedder::MODEL, 3, &bytes)
        .expect("옛 줄");
    assert_eq!(
        f.knowledge.stored_vector(id).expect("읽기"),
        Some(old.clone())
    );
    // 옛 줄과 새 줄이 섞여도 순위를 낸다.
    f.add_trade("T-2", "다른 매매 기록입니다");
    f.knowledge.embed_missing(10).await.expect("임베딩");
    let other = f
        .store
        .knowledge_chunks(KnowledgeKind::Trade, "T-2")
        .expect("조각")[0]
        .id;
    let ranked = f
        .knowledge
        .rank_by_vector(&old, &[other, id, ChunkId(999)], 5)
        .expect("순위");
    assert_eq!(ranked.len(), 2, "벡터가 없는 조각은 빠진다");
    assert_eq!(ranked[0].0, id);
    assert!((ranked[0].1 - 1.0).abs() < 1e-6);
    // 글이 바뀌어 다시 넣으면 정수로 바뀐다.
    f.add_trade("T-1", "고친 매매 기록");
    f.knowledge.embed_missing(10).await.expect("임베딩");
    let new_id = f
        .store
        .knowledge_chunks(KnowledgeKind::Trade, "T-1")
        .expect("조각")[0]
        .id;
    let emb = f
        .store
        .chunk_embedding(new_id)
        .expect("읽기")
        .expect("있다");
    assert!(emb.quantized);
}

// 검증: D10 M7
#[test]
fn 오래된_바깥_자료는_요약_기록_하나로_바뀐다() {
    let f = Fixture::new();
    for (i, day) in [(1, 0), (2, 1), (3, 30)] {
        f.knowledge
            .index(RecordText::external(
                &format!("news-{i}"),
                &format!("제목 {i}"),
                "요약 글",
                "https://example.invalid/a",
                UnixMillis(T0 + day * DAY),
            ))
            .expect("색인");
    }
    f.add(
        KnowledgeKind::MarketNote,
        KnowledgeSource::Team,
        "note-1",
        "팀 시장 메모",
        T0,
    );
    let old = f
        .knowledge
        .old_external_records(UnixMillis(T0 + 7 * DAY), 10)
        .expect("고르기");
    let refs: Vec<&str> = old.iter().map(|r| r.record_ref.as_str()).collect();
    assert_eq!(refs, ["news-1", "news-2"]);
    assert!(old[0].text.contains("제목 1"));
    let summary = RecordText {
        kind: KnowledgeKind::MarketNote,
        source: KnowledgeSource::External,
        record_ref: "archive:knowledge:2027-W02".into(),
        text: "보관 요약: 바깥 자료 2건(제목 1 · 제목 2)".into(),
        at: UnixMillis(T0 + 40 * DAY),
    };
    let ids = f
        .knowledge
        .replace_with_summary(&["news-1".into(), "news-2".into()], summary)
        .expect("바꾸기");
    assert_eq!(ids.len(), 1);
    for gone in ["news-1", "news-2"] {
        assert!(
            f.store
                .knowledge_chunks(KnowledgeKind::MarketNote, gone)
                .expect("조각")
                .is_empty()
        );
    }
    assert_eq!(
        f.store
            .knowledge_chunks(KnowledgeKind::MarketNote, "news-3")
            .expect("조각")
            .len(),
        1
    );
    let again = f
        .knowledge
        .old_external_records(UnixMillis(T0 + 100 * DAY), 10)
        .expect("고르기");
    let refs: Vec<&str> = again.iter().map(|r| r.record_ref.as_str()).collect();
    assert_eq!(refs, ["news-3"], "보관 요약 기록은 다시 고르지 않는다");
}
