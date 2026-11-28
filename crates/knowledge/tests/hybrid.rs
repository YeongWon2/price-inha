//! knowledge 6차 시험(백로그 R 묶음 C, 4차 4번 "뜻 검색 몸통"): 키워드 + 벡터 하이브리드(원본 4.1의 1~9).
//!
//! 잠긴 `tests/suite/`를 고치지 않으려고 따로 둔 시험 바이너리다. 가짜 임베딩만 쓴다(바깥 요청 없음).
//! 가짜 임베딩은 낱말로 방향을 정한다: "하락" · "떨어" → x, "상승" · "오름" → y, 그 밖 → z.
//! (6차) 2026-11-27 백로그 작업을 합쳐 잠금.

use std::sync::Arc;
use std::sync::atomic::{AtomicU8, AtomicUsize, Ordering};

use async_trait::async_trait;
use pi_knowledge::{
    CHUNK_OVERLAP, CHUNK_TOKENS, EmbedError, Embedder, IndexSettings, Knowledge, RecordText,
    ReembedProgress, SearchConfig, SearchMode, SearchOutput, SearchRequest, SemanticStatus,
    TEXT_WEIGHT_MICROS, VECTOR_WEIGHT_MICROS,
};
use pi_store::{KnowledgeKind, KnowledgeSource, Store, UnixMillis};
use tempfile::TempDir;

const T0: i64 = 1_800_000_000_000;
const MODEL: &str = "fake-direction-1";

/// 질의 임베딩의 모양.
const GOOD: u8 = 0;
const UNAVAILABLE: u8 = 1;
const FAILED: u8 = 2;
/// 질의만 다른 방향으로 바꾼다(질의가 `embed_query`를 지나는지 본다).
const QUERY_TURNS_UP: u8 = 3;

struct Direction {
    query_mode: AtomicU8,
    queries: AtomicUsize,
}

impl Direction {
    fn new() -> Arc<Direction> {
        Arc::new(Direction {
            query_mode: AtomicU8::new(GOOD),
            queries: AtomicUsize::new(0),
        })
    }

    fn vector(text: &str) -> Vec<f32> {
        if text.contains("하락") || text.contains("떨어") {
            vec![1.0, 0.0, 0.0]
        } else if text.contains("상승") || text.contains("오름") {
            vec![0.0, 1.0, 0.0]
        } else {
            vec![0.0, 0.0, 1.0]
        }
    }

    fn set(&self, mode: u8) {
        self.query_mode.store(mode, Ordering::SeqCst);
    }
}

#[async_trait]
impl Embedder for Direction {
    fn model(&self) -> &str {
        MODEL
    }
    fn dims(&self) -> u32 {
        3
    }
    async fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, EmbedError> {
        Ok(texts.iter().map(|t| Direction::vector(t)).collect())
    }
    async fn embed_query(&self, text: &str) -> Result<Vec<f32>, EmbedError> {
        self.queries.fetch_add(1, Ordering::SeqCst);
        match self.query_mode.load(Ordering::SeqCst) {
            UNAVAILABLE => Err(EmbedError::Unavailable("연결 안 됨".to_string())),
            FAILED => Err(EmbedError::Failed("모양이 틀림".to_string())),
            QUERY_TURNS_UP => Ok(vec![0.0, 1.0, 0.0]),
            _ => Ok(Direction::vector(text)),
        }
    }
}

struct Fixture {
    _dir: TempDir,
    knowledge: Knowledge,
    embedder: Arc<Direction>,
}

fn settings() -> IndexSettings {
    IndexSettings {
        provider: "fake".to_string(),
        model: MODEL.to_string(),
        dims: Some(3),
        chunk_tokens: CHUNK_TOKENS,
        chunk_overlap: CHUNK_OVERLAP,
        tokenizer: "trigram".to_string(),
        scope_hash: "all".to_string(),
    }
}

fn config(min_score_micros: i64) -> SearchConfig {
    SearchConfig {
        min_score_micros,
        ..SearchConfig::default()
    }
}

fn open(min_score_micros: i64) -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("k.sqlite")).unwrap();
    let embedder = Direction::new();
    let shared: Arc<dyn Embedder> = embedder.clone();
    let knowledge = Knowledge::new(store, config(min_score_micros)).with_embedder(shared);
    Fixture {
        _dir: dir,
        knowledge,
        embedder,
    }
}

impl Fixture {
    fn add(&self, kind: KnowledgeKind, source: KnowledgeSource, key: &str, text: &str) {
        self.knowledge
            .index(RecordText {
                kind,
                source,
                record_ref: key.to_string(),
                text: text.to_string(),
                at: UnixMillis(T0),
            })
            .unwrap();
    }

    fn trade(&self, key: &str, text: &str) {
        self.add(KnowledgeKind::Trade, KnowledgeSource::Team, key, text);
    }

    /// 다시 만들기를 끝까지 돌려 색인 설정을 남긴다.
    async fn embed_all(&self) {
        for _ in 0..20 {
            match self.knowledge.reembed_step(&settings(), 100).await.unwrap() {
                ReembedProgress::Done { .. } => return,
                ReembedProgress::InProgress { .. } => {}
                ReembedProgress::Unavailable => panic!("포트가 붙어 있다"),
            }
        }
        panic!("다시 만들기가 끝나야 한다");
    }

    async fn hybrid(&self, request: &SearchRequest) -> SearchOutput {
        self.knowledge
            .hybrid_search(request, UnixMillis(T0))
            .await
            .expect("하이브리드 검색은 오류가 아니다")
    }
}

fn refs(output: &SearchOutput) -> Vec<String> {
    output
        .results
        .iter()
        .map(|h| h.record_ref.clone())
        .collect()
}

fn code_of(output: &SearchOutput) -> Option<String> {
    match &output.semantic {
        SemanticStatus::Unavailable {
            code,
            warning,
            action,
        } => {
            assert!(!warning.is_empty() && !action.is_empty(), "{output:?}");
            Some(code.clone())
        }
        SemanticStatus::Available => None,
    }
}

// 검증: D10 V23
#[tokio::test]
async fn 뜻이_가까운_조각은_키워드가_맞지_않아도_나온다() {
    let f = open(0);
    f.trade("T-down", "주가가 크게 떨어졌다 손실 정리");
    f.trade("T-up", "실적 발표 뒤 상승 기대");
    f.embed_all().await;
    let keyword = f
        .knowledge
        .search(&SearchRequest::keywords("하락"), UnixMillis(T0))
        .unwrap();
    assert!(
        keyword.results.is_empty(),
        "키워드로는 맞는 것이 없다: {keyword:?}"
    );

    let out = f.hybrid(&SearchRequest::keywords("하락")).await;
    assert_eq!(out.mode, SearchMode::Semantic);
    assert_eq!(out.semantic, SemanticStatus::Available);
    assert_eq!(
        refs(&out).first().map(String::as_str),
        Some("T-down"),
        "{out:?}"
    );
    assert!(
        !refs(&out).contains(&"T-up".to_string()),
        "방향이 다른 것은 벡터 점수가 0이다"
    );
    let semantic = f
        .knowledge
        .semantic_search("하락", UnixMillis(T0))
        .await
        .unwrap();
    assert_eq!(
        refs(&semantic),
        refs(&out),
        "semantic_search는 하이브리드와 같은 길"
    );
}

// 검증: D10
#[tokio::test]
async fn 합은_벡터_0_7과_다시_잰_키워드_0_3이다() {
    assert_eq!(
        (VECTOR_WEIGHT_MICROS, TEXT_WEIGHT_MICROS),
        (700_000, 300_000)
    );
    let f = open(0);
    f.trade("T-vector", "주가가 크게 떨어졌다");
    f.trade("T-both", "하락 추세 손절 하락 하락");
    f.embed_all().await;
    let out = f.hybrid(&SearchRequest::keywords("하락")).await;
    let score = |key: &str| {
        out.results
            .iter()
            .find(|h| h.record_ref == key)
            .and_then(|h| h.score_micros)
            .unwrap_or_else(|| panic!("{key}가 점수와 함께 나와야 한다: {out:?}"))
    };
    let vector_only = score("T-vector");
    assert!(
        (vector_only - VECTOR_WEIGHT_MICROS).abs() <= 1_000,
        "벡터만 맞으면 0.7 × 1: {vector_only}"
    );
    let both = score("T-both");
    assert!(
        both > vector_only && both <= 1_000_000,
        "키워드도 맞으면 0.3 × 다시 잰 키워드가 더해진다: {both}"
    );
    assert_eq!(refs(&out).first().map(String::as_str), Some("T-both"));
}

// 검증: M7 D10
#[tokio::test]
async fn 자동_회상이면_바깥_자료는_벡터_후보에서도_빠진다() {
    let f = open(0);
    f.add(
        KnowledgeKind::MarketNote,
        KnowledgeSource::External,
        "news:1",
        "코스피 급히 떨어져",
    );
    f.trade("T-down", "보유 종목이 떨어졌다");
    f.embed_all().await;
    let explicit = f.hybrid(&SearchRequest::keywords("하락")).await;
    assert!(
        refs(&explicit).contains(&"news:1".to_string()),
        "{explicit:?}"
    );
    let recall = SearchRequest {
        auto_recall: true,
        ..SearchRequest::keywords("하락")
    };
    let out = f.hybrid(&recall).await;
    assert!(!refs(&out).contains(&"news:1".to_string()), "{out:?}");
    assert!(refs(&out).contains(&"T-down".to_string()));
    let only_trades = SearchRequest {
        kinds: vec![KnowledgeKind::MarketNote],
        sources: vec![KnowledgeSource::Team],
        ..SearchRequest::keywords("하락")
    };
    assert!(
        f.hybrid(&only_trades).await.results.is_empty(),
        "종류 · 등급 거르기도 같다"
    );
}

// 검증: D10 V23
#[tokio::test]
async fn 다시_만들기가_끝나지_않았으면_키워드_결과와_reindexing이다() {
    let f = open(0);
    f.trade("T-1", "손절 기준을 늦게 지켰다");
    let out = f.hybrid(&SearchRequest::keywords("손절")).await;
    assert_eq!(code_of(&out).as_deref(), Some("reindexing"), "{out:?}");
    assert_eq!(
        refs(&out),
        vec!["T-1".to_string()],
        "키워드 결과가 함께 온다"
    );
    assert_eq!(
        f.embedder.queries.load(Ordering::SeqCst),
        0,
        "색인이 준비되지 않았으면 질의를 임베딩하지 않는다"
    );
}

// 검증: D10 V23
#[tokio::test]
async fn 질의_임베딩이_안_되면_오류가_아니라_키워드_결과와_코드다() {
    let f = open(0);
    f.trade("T-1", "손절 기준을 늦게 지켰다");
    f.embed_all().await;
    for (mode, code) in [
        (UNAVAILABLE, "embedder_unavailable"),
        (FAILED, "embedder_failed"),
    ] {
        f.embedder.set(mode);
        let out = f.hybrid(&SearchRequest::keywords("손절")).await;
        assert_eq!(code_of(&out).as_deref(), Some(code), "{out:?}");
        assert_eq!(refs(&out), vec!["T-1".to_string()]);
    }
}

// 검증: V23
#[tokio::test]
async fn 포트가_없으면_빈_결과와_no_embedder다() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("k.sqlite")).unwrap();
    let knowledge = Knowledge::new(store, config(0));
    knowledge
        .index(RecordText {
            kind: KnowledgeKind::Trade,
            source: KnowledgeSource::Team,
            record_ref: "T-1".to_string(),
            text: "손절 기준".to_string(),
            at: UnixMillis(T0),
        })
        .unwrap();
    let out = knowledge
        .hybrid_search(&SearchRequest::keywords("손절"), UnixMillis(T0))
        .await
        .unwrap();
    assert!(out.results.is_empty());
    assert_eq!(code_of(&out).as_deref(), Some("no_embedder"));
}

// 검증: D10
#[tokio::test]
async fn 질의는_embed_query를_지난다() {
    let f = open(0);
    f.trade("T-down", "주가가 떨어졌다");
    f.trade("T-up", "주가 상승");
    f.embed_all().await;
    f.embedder.set(QUERY_TURNS_UP);
    let out = f.hybrid(&SearchRequest::keywords("하락")).await;
    assert_eq!(
        refs(&out).first().map(String::as_str),
        Some("T-up"),
        "{out:?}"
    );
    assert!(f.embedder.queries.load(Ordering::SeqCst) >= 1);
}

// 검증: D10
#[tokio::test]
async fn 모두_문턱_아래면_키워드_결과로_물러선다() {
    // 문턱 0.75: 벡터만 맞는 조각(0.7)은 문턱 아래, 키워드만 맞는 조각(벡터 0)도 문턱 아래다.
    let f = open(750_000);
    f.trade("T-down", "주가가 떨어졌다");
    f.trade("T-kw", "손절 원칙 정리");
    f.embed_all().await;
    let out = f.hybrid(&SearchRequest::keywords("하락 손절")).await;
    let keyword = f
        .knowledge
        .search(&SearchRequest::keywords("하락 손절"), UnixMillis(T0))
        .unwrap();
    assert_eq!(out.semantic, SemanticStatus::Available, "{out:?}");
    assert_eq!(refs(&out), refs(&keyword), "키워드 결과 그대로");
    assert!(!refs(&out).contains(&"T-down".to_string()));
}
