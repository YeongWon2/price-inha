//! 임베딩 다시 만들기의 한 걸음(4차 3번): `reembed_step`.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use pi_knowledge::{
    CHUNK_OVERLAP, CHUNK_TOKENS, EmbedError, Embedder, Error, IndexSettings, Knowledge,
    ReembedProgress, ReindexPlan, ReindexReason,
};
use pi_store::{ChunkId, KnowledgeKind, Store};

use crate::common::{Fixture, test_config};

/// 모델 이름을 고를 수 있고, 실패를 켰다 껐다 할 수 있는 가짜 임베딩. 부른 횟수와 받은 글을 남긴다.
struct ModelEmbedder {
    model: &'static str,
    dims: u32,
    calls: AtomicUsize,
    failing: AtomicBool,
    seen: Mutex<Vec<String>>,
}

impl ModelEmbedder {
    fn new(model: &'static str) -> Arc<ModelEmbedder> {
        ModelEmbedder::with_dims(model, 3)
    }
    fn with_dims(model: &'static str, dims: u32) -> Arc<ModelEmbedder> {
        Arc::new(ModelEmbedder {
            model,
            dims,
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
        self.dims
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
        Ok(texts
            .iter()
            .map(|_| (1..=self.dims).map(|i| 0.25 * i as f32).collect())
            .collect())
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

/// `Done`이 나올 때까지 한 걸음씩 불러 그 `Done`에 실린 계획을 돌려준다.
async fn plan_when_done(k: &Knowledge, current: &IndexSettings, batch: usize) -> ReindexPlan {
    for _ in 0..20 {
        if let ReembedProgress::Done { plan, .. } =
            k.reembed_step(current, batch).await.expect("한 걸음")
        {
            return plan;
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

// 검증: D10 V23
#[tokio::test]
async fn a_zero_batch_is_in_progress_with_nothing_done_and_never_records_settings() {
    let f = three_records();
    let a = ModelEmbedder::new("model-a");
    let ka = knowledge_with(&f.store, &a);
    let sa = settings("model-a", Some(3));
    run_to_done(&ka, &a, &sa, 10, None).await;

    let b = ModelEmbedder::new("model-b");
    let kb = knowledge_with(&f.store, &b);
    let sb = settings("model-b", Some(3));
    for _ in 0..2 {
        let step = kb.reembed_step(&sb, 0).await.expect("한 걸음");
        assert_eq!(step, ReembedProgress::InProgress { embedded: 0 });
    }
    assert_eq!(b.calls(), 0, "batch 0인데 포트를 불렀다");
    assert_eq!(kb.stored_settings().expect("설정"), Some(sa.clone()));
    for id in all_chunks(&f) {
        assert_eq!(model_of(&f, id).as_deref(), Some("model-a"));
    }
    assert_eq!(
        f.store
            .chunks_missing_embedding("model-b", 10)
            .expect("목록")
            .len(),
        3
    );

    let total = run_to_done(&kb, &b, &sb, 2, Some(sa)).await;
    assert_eq!(total, 3);
    assert_eq!(kb.stored_settings().expect("설정"), Some(sb));
    for id in all_chunks(&f) {
        assert_eq!(model_of(&f, id).as_deref(), Some("model-b"));
    }
}

// 검증: D10 V23
#[tokio::test]
async fn same_model_with_other_dims_is_an_embedder_error_and_keeps_the_old_settings() {
    let f = three_records();
    let a3 = ModelEmbedder::new("model-a");
    let k3 = knowledge_with(&f.store, &a3);
    let s3 = settings("model-a", Some(3));
    run_to_done(&k3, &a3, &s3, 10, None).await;

    let a4 = ModelEmbedder::with_dims("model-a", 4);
    let k4 = knowledge_with(&f.store, &a4);
    let s4 = settings("model-a", Some(4));
    for _ in 0..3 {
        let r = k4.reembed_step(&s4, 10).await;
        assert!(matches!(r, Err(Error::Embedder(_))), "{r:?}");
        assert_eq!(k4.stored_settings().expect("설정"), Some(s3.clone()));
    }
    assert_eq!(a4.calls(), 0, "차원만 다른데 포트를 불렀다");
    for id in all_chunks(&f) {
        let e = f.store.chunk_embedding(id).expect("읽기").expect("임베딩");
        assert_eq!(e.model, "model-a");
        assert_eq!(e.dims, 3);
    }
}

// 검증: D10
#[tokio::test]
async fn done_carries_the_plan_from_before_the_settings_were_recorded() {
    let f = three_records();
    let a = ModelEmbedder::new("model-a");
    let ka = knowledge_with(&f.store, &a);
    let sa = settings("model-a", Some(3));
    assert_eq!(
        plan_when_done(&ka, &sa, 2).await,
        ReindexPlan::Rebuild(ReindexReason::FirstTime)
    );

    let calls = a.calls();
    let again = ka.reembed_step(&sa, 10).await.expect("한 걸음");
    assert_eq!(
        again,
        ReembedProgress::Done {
            embedded: 0,
            plan: ReindexPlan::Keep
        }
    );
    assert_eq!(a.calls(), calls);

    let b = ModelEmbedder::new("model-b");
    let kb = knowledge_with(&f.store, &b);
    let sb = settings("model-b", Some(3));
    assert_eq!(
        plan_when_done(&kb, &sb, 2).await,
        ReindexPlan::Rebuild(ReindexReason::ModelChanged)
    );
    assert_eq!(kb.stored_settings().expect("설정"), Some(sb));
}

/// 남은 설정에서 모델 칸의 키. `record_settings`가 칸을 따로따로 쓰는 것을 흉내 낼 때 쓴다.
const MODEL_KEY: &str = "settings.model";
/// 남은 설정에서 차원 칸의 키. 값이 빈 글자면 벡터가 없다는 뜻이다.
const DIMS_KEY: &str = "settings.dims";

/// 키워드 전용 설정(모델 빈 글자 · 차원 없음)을 남기고, 설정은 건드리지 않은 채 model-a(차원 3)로
/// 모든 조각의 임베딩을 만든 시험대. `Done` 직전에 설정 남기기만 남은 상태다.
async fn keyword_settings_with_all_chunks_embedded() -> (Fixture, Arc<ModelEmbedder>, Knowledge) {
    let f = three_records();
    let keyword_only = settings("", None);
    f.knowledge.record_settings(&keyword_only).expect("설정");
    let a = ModelEmbedder::new("model-a");
    let ka = knowledge_with(&f.store, &a);
    let report = ka.embed_missing(10).await.expect("임베딩");
    assert_eq!(report.embedded, 3);
    assert_eq!(ka.stored_settings().expect("설정"), Some(keyword_only));
    (f, a, ka)
}

// 검증: D10 V23
#[tokio::test]
async fn settings_torn_after_the_dims_cell_heal_on_the_next_step() {
    let (f, a, ka) = keyword_settings_with_all_chunks_embedded().await;
    // 새 순서(모델 칸이 맨 마지막)에서 끊긴 모습: 차원 칸만 새것이고 모델 칸은 옛것(빈 글자)이다.
    f.store.set_knowledge_meta(DIMS_KEY, "3").expect("차원 칸");
    let torn = settings("", Some(3));
    assert_eq!(ka.stored_settings().expect("설정"), Some(torn.clone()));

    let sa = settings("model-a", Some(3));
    let total = run_to_done(&ka, &a, &sa, 10, Some(torn)).await;
    assert_eq!(total, 0, "이미 다 만든 조각을 또 만들었다");
    assert_eq!(ka.stored_settings().expect("설정"), Some(sa.clone()));
    for id in all_chunks(&f) {
        assert_eq!(model_of(&f, id).as_deref(), Some("model-a"));
    }

    // 다시 열어도 온전한 설정이 남아 있고, 더 할 일이 없다.
    let f = f.reopen();
    let again = knowledge_with(&f.store, &a);
    assert_eq!(again.stored_settings().expect("설정"), Some(sa.clone()));
    assert_eq!(
        again.reembed_step(&sa, 10).await.expect("한 걸음"),
        ReembedProgress::Done {
            embedded: 0,
            plan: ReindexPlan::Keep
        }
    );
}

// 검증: D10 V23
#[tokio::test]
async fn a_model_and_dims_switch_torn_after_the_dims_cell_reembeds_everything() {
    let f = three_records();
    let a = ModelEmbedder::new("model-a");
    let ka = knowledge_with(&f.store, &a);
    let sa = settings("model-a", Some(3));
    run_to_done(&ka, &a, &sa, 10, None).await;

    // A,3 → B,4 로 남기다 끊긴 모습: 차원 칸만 4이고 모델 칸은 아직 A다.
    f.store.set_knowledge_meta(DIMS_KEY, "4").expect("차원 칸");
    let torn = settings("model-a", Some(4));
    assert_eq!(ka.stored_settings().expect("설정"), Some(torn.clone()));

    let b = ModelEmbedder::with_dims("model-b", 4);
    let kb = knowledge_with(&f.store, &b);
    let sb = settings("model-b", Some(4));
    let total = run_to_done(&kb, &b, &sb, 2, Some(torn)).await;
    assert_eq!(total, 3);
    assert_eq!(kb.stored_settings().expect("설정"), Some(sb));
    for id in all_chunks(&f) {
        let e = f.store.chunk_embedding(id).expect("읽기").expect("임베딩");
        assert_eq!(e.model, "model-b");
        assert_eq!(e.dims, 4);
    }
}

// 검증: D10 V23
#[tokio::test]
async fn settings_torn_after_the_model_cell_with_no_dims_still_reach_done() {
    let (f, a, ka) = keyword_settings_with_all_chunks_embedded().await;
    // 옛 순서(모델 칸이 차원보다 먼저)에서 끊겨 이미 남아 있을 수 있는 모습: 모델 칸만 새것이고
    // 차원 칸은 아직 비어 있다. 옛 차원 벡터가 있을 수 없으니 막지 않고 이어서 끝낸다.
    f.store
        .set_knowledge_meta(MODEL_KEY, "model-a")
        .expect("모델 칸");
    let torn = settings("model-a", None);
    assert_eq!(ka.stored_settings().expect("설정"), Some(torn.clone()));

    let sa = settings("model-a", Some(3));
    let total = run_to_done(&ka, &a, &sa, 10, Some(torn)).await;
    assert_eq!(total, 0, "이미 다 만든 조각을 또 만들었다");
    assert_eq!(ka.stored_settings().expect("설정"), Some(sa));
    for id in all_chunks(&f) {
        let e = f.store.chunk_embedding(id).expect("읽기").expect("임베딩");
        assert_eq!(e.model, "model-a");
        assert_eq!(e.dims, 3);
    }
}
