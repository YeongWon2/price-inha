//! 임베딩 다시 만들기의 한 걸음(4차 3번): `reembed_step`.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use pi_knowledge::{
    CHUNK_OVERLAP, CHUNK_TOKENS, EmbedError, Embedder, Error, IndexSettings, Knowledge,
    ReembedProgress,
};
use pi_store::{ChunkId, KnowledgeKind, Store};

use crate::common::{Fixture, test_config};

/// 모델 이름을 고를 수 있고, 실패를 켰다 껐다 할 수 있는 가짜 임베딩. 부른 횟수와 받은 글을 남긴다.
struct ModelEmbedder {
    model: &'static str,
    calls: AtomicUsize,
    failing: AtomicBool,
    seen: Mutex<Vec<String>>,
}

impl ModelEmbedder {
    fn new(model: &'static str) -> Arc<ModelEmbedder> {
        Arc::new(ModelEmbedder {
            model,
            calls: AtomicUsize::new(0),
            failing: AtomicBool::new(false),
            seen: Mutex::new(Vec::new()),
        })
    }
    fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
    fn fail(&self, on: bool) {
        self.failing.store(on, Ordering::SeqCst);
    }
    fn seen(&self) -> Vec<String> {
        self.seen.lock().expect("잠금").clone()
    }
}

#[async_trait]
impl Embedder for ModelEmbedder {
    fn model(&self) -> &str {
        self.model
    }
    fn dims(&self) -> u32 {
        3
    }
    async fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, EmbedError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if self.failing.load(Ordering::SeqCst) {
            return Err(EmbedError::Failed("가짜 실패".to_string()));
        }
        self.seen
            .lock()
            .expect("잠금")
            .extend(texts.iter().cloned());
        Ok(texts.iter().map(|_| vec![0.25, 0.5, 1.0]).collect())
    }
}

fn settings(model: &str, dims: Option<u32>) -> IndexSettings {
    IndexSettings {
        provider: "fake".to_string(),
        model: model.to_string(),
        dims,
        chunk_tokens: CHUNK_TOKENS,
        chunk_overlap: CHUNK_OVERLAP,
        tokenizer: "trigram".to_string(),
        scope_hash: "scope-1".to_string(),
    }
}

fn knowledge_with(store: &Store, embedder: &Arc<ModelEmbedder>) -> Knowledge {
    Knowledge::new(store.clone(), test_config()).with_embedder(embedder.clone())
}

/// 매매 기록 셋(조각 셋)을 올린 시험대.
fn three_records() -> Fixture {
    let f = Fixture::new();
    f.add_trade("T-1", "삼성전자 매수 진행");
    f.add_trade("T-2", "하이닉스 손절 처리");
    f.add_trade("T-3", "카카오 매도 완료");
    f
}

fn all_chunks(f: &Fixture) -> Vec<ChunkId> {
    ["T-1", "T-2", "T-3"]
        .iter()
        .flat_map(|r| {
            f.store
                .knowledge_chunks(KnowledgeKind::Trade, r)
                .expect("조각")
        })
        .map(|c| c.id)
        .collect()
}

fn model_of(f: &Fixture, id: ChunkId) -> Option<String> {
    f.store.chunk_embedding(id).expect("읽기").map(|e| e.model)
}

/// `Done`이 나올 때까지 한 걸음씩 부른다. 걸음마다 포트 호출이 많아야 하나인지, `InProgress` 동안
/// 남은 설정이 `before` 그대로인지 본다. 둔 조각 수의 합을 돌려준다.
async fn run_to_done(
    k: &Knowledge,
    embedder: &ModelEmbedder,
    current: &IndexSettings,
    batch: usize,
    before: Option<IndexSettings>,
) -> usize {
    let mut total = 0;
    for _ in 0..20 {
        let calls = embedder.calls();
        let step = k.reembed_step(current, batch).await.expect("한 걸음");
        assert!(
            embedder.calls() - calls <= 1,
            "한 걸음에 포트 호출이 둘 이상"
        );
        match step {
            ReembedProgress::InProgress { embedded } => {
                assert!(embedded > 0 && embedded <= batch);
                total += embedded;
                assert_eq!(
                    k.stored_settings().expect("설정"),
                    before,
                    "도중에 설정이 바뀜"
                );
            }
            ReembedProgress::Done { embedded, .. } => {
                assert!(embedded <= batch);
                return total + embedded;
            }
            ReembedProgress::Unavailable => panic!("포트가 있는데 쓸 수 없다고 한다"),
        }
    }
    panic!("스무 걸음 안에 끝나지 않았다");
}

// 검증: V23 D10
#[tokio::test]
async fn without_an_embedder_reembed_step_is_unavailable_and_changes_nothing() {
    let f = three_records();
    let old = settings("model-a", Some(3));
    f.knowledge.record_settings(&old).expect("설정");
    let step = f
        .knowledge
        .reembed_step(&settings("model-b", Some(3)), 10)
        .await
        .expect("결과");
    assert_eq!(step, ReembedProgress::Unavailable);
    assert_eq!(f.knowledge.stored_settings().expect("설정"), Some(old));
    assert_eq!(
        f.store
            .chunks_missing_embedding("model-b", 10)
            .expect("목록")
            .len(),
        3
    );
    for id in all_chunks(&f) {
        assert_eq!(model_of(&f, id), None);
    }
}

// 검증: D10
#[tokio::test]
async fn settings_that_disagree_with_the_port_are_an_embedder_error_and_change_nothing() {
    let f = three_records();
    let a = ModelEmbedder::new("model-a");
    let k = knowledge_with(&f.store, &a);
    for wrong in [
        settings("model-b", Some(3)),
        settings("model-a", Some(4)),
        settings("model-a", None),
    ] {
        let r = k.reembed_step(&wrong, 10).await;
        assert!(matches!(r, Err(Error::Embedder(_))), "{wrong:?} → {r:?}");
    }
    assert_eq!(a.calls(), 0);
    assert_eq!(k.stored_settings().expect("설정"), None);
    for id in all_chunks(&f) {
        assert_eq!(model_of(&f, id), None);
    }
}

// 검증: D10 V23
#[tokio::test]
async fn first_build_embeds_every_chunk_and_records_settings_only_when_done() {
    let f = three_records();
    let a = ModelEmbedder::new("model-a");
    let k = knowledge_with(&f.store, &a);
    let current = settings("model-a", Some(3));
    let total = run_to_done(&k, &a, &current, 1, None).await;
    assert_eq!(total, 3);
    assert_eq!(k.stored_settings().expect("설정"), Some(current));
    for id in all_chunks(&f) {
        assert_eq!(model_of(&f, id).as_deref(), Some("model-a"));
    }
}

// 검증: D10 V23 M7
#[tokio::test]
async fn switching_to_model_b_rebuilds_every_chunk_and_moves_settings_only_on_done() {
    let f = three_records();
    let a = ModelEmbedder::new("model-a");
    let ka = knowledge_with(&f.store, &a);
    let sa = settings("model-a", Some(3));
    run_to_done(&ka, &a, &sa, 10, None).await;
    assert_eq!(ka.stored_settings().expect("설정"), Some(sa.clone()));

    let b = ModelEmbedder::new("model-b");
    let kb = knowledge_with(&f.store, &b);
    let sb = settings("model-b", Some(3));
    let total = run_to_done(&kb, &b, &sb, 2, Some(sa)).await;
    assert_eq!(total, 3);
    assert_eq!(kb.stored_settings().expect("설정"), Some(sb));
    for id in all_chunks(&f) {
        assert_eq!(model_of(&f, id).as_deref(), Some("model-b"));
    }
    assert!(
        f.store
            .chunks_missing_embedding("model-b", 10)
            .expect("목록")
            .is_empty()
    );
}

// 검증: D10 V23
#[tokio::test]
async fn a_failure_midway_keeps_old_settings_and_the_next_call_continues() {
    let f = three_records();
    let a = ModelEmbedder::new("model-a");
    let ka = knowledge_with(&f.store, &a);
    let sa = settings("model-a", Some(3));
    run_to_done(&ka, &a, &sa, 10, None).await;

    let b = ModelEmbedder::new("model-b");
    let kb = knowledge_with(&f.store, &b);
    let sb = settings("model-b", Some(3));

    let first = kb.reembed_step(&sb, 1).await.expect("첫 걸음");
    assert_eq!(first, ReembedProgress::InProgress { embedded: 1 });
    assert_eq!(kb.stored_settings().expect("설정"), Some(sa.clone()));

    b.fail(true);
    let calls = b.calls();
    let failed = kb.reembed_step(&sb, 1).await;
    assert!(matches!(failed, Err(Error::Embedder(_))), "{failed:?}");
    assert!(b.calls() - calls <= 1);
    assert_eq!(kb.stored_settings().expect("설정"), Some(sa.clone()));
    assert_eq!(
        f.store
            .chunks_missing_embedding("model-b", 10)
            .expect("목록")
            .len(),
        2
    );

    b.fail(false);
    let rest = run_to_done(&kb, &b, &sb, 1, Some(sa)).await;
    assert_eq!(rest, 2);
    assert_eq!(kb.stored_settings().expect("설정"), Some(sb));
    for id in all_chunks(&f) {
        assert_eq!(model_of(&f, id).as_deref(), Some("model-b"));
    }
}

// 검증: M7 V23
#[tokio::test]
async fn only_the_changed_record_is_embedded_again() {
    let f = three_records();
    let a = ModelEmbedder::new("model-a");
    let k = knowledge_with(&f.store, &a);
    let sa = settings("model-a", Some(3));
    run_to_done(&k, &a, &sa, 10, None).await;
    let before = a.seen().len();

    f.add_trade("T-2", "하이닉스 손절 처리 뒤 다시 매수");
    let total = run_to_done(&k, &a, &sa, 10, Some(sa.clone())).await;
    assert_eq!(total, 1);
    assert_eq!(
        a.seen()[before..].to_vec(),
        vec!["하이닉스 손절 처리 뒤 다시 매수".to_string()]
    );
    for id in all_chunks(&f) {
        assert_eq!(model_of(&f, id).as_deref(), Some("model-a"));
    }
    assert_eq!(k.stored_settings().expect("설정"), Some(sa));
}

// 검증: D10
#[tokio::test]
async fn done_with_nothing_missing_makes_no_port_call() {
    let f = three_records();
    let a = ModelEmbedder::new("model-a");
    let k = knowledge_with(&f.store, &a);
    let sa = settings("model-a", Some(3));
    run_to_done(&k, &a, &sa, 10, None).await;
    let calls = a.calls();
    let step = k.reembed_step(&sa, 10).await.expect("한 걸음");
    assert!(
        matches!(step, ReembedProgress::Done { embedded: 0, .. }),
        "{step:?}"
    );
    assert_eq!(a.calls(), calls);
}
