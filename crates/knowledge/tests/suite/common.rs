//! 시험 도구: 임시 폴더의 실제 저장소, 가짜 임베딩, 가짜 매매 요약, 글 만들기.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use async_trait::async_trait;
use pi_knowledge::{
    Condition, EmbedError, Embedder, Knowledge, RecordText, SearchConfig, SearchOutput,
    SearchRequest, TradeCatalog, TradeRecord,
};
use pi_store::{KnowledgeKind, KnowledgeSource, Store, UnixMillis};
use rust_decimal::Decimal;
use tempfile::TempDir;

/// 시험의 "지금".
pub const T0: i64 = 1_800_000_000_000;
/// 하루(밀리초).
pub const DAY: i64 = 86_400_000;

/// 가짜 임베딩: 글자 수와 정해진 값으로 길이 3의 벡터를 준다. 부른 횟수를 센다.
pub struct FakeEmbedder {
    calls: AtomicUsize,
    mode: EmbedMode,
}

#[derive(Clone, Copy)]
enum EmbedMode {
    Good,
    Failing,
    WrongDims,
}

impl FakeEmbedder {
    pub const MODEL: &'static str = "fake-embed-1";

    pub fn good() -> Arc<FakeEmbedder> {
        Arc::new(FakeEmbedder {
            calls: AtomicUsize::new(0),
            mode: EmbedMode::Good,
        })
    }
    pub fn failing() -> Arc<FakeEmbedder> {
        Arc::new(FakeEmbedder {
            calls: AtomicUsize::new(0),
            mode: EmbedMode::Failing,
        })
    }
    pub fn wrong_dims() -> Arc<FakeEmbedder> {
        Arc::new(FakeEmbedder {
            calls: AtomicUsize::new(0),
            mode: EmbedMode::WrongDims,
        })
    }
    pub fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
    /// 글에 대해 이 가짜가 주는 벡터.
    pub fn vector_of(text: &str) -> Vec<f32> {
        vec![text.chars().count() as f32, 1.0, 0.5]
    }
    /// 그 벡터의 저장 바이트(리틀 엔디언 f32).
    pub fn bytes_of(text: &str) -> Vec<u8> {
        Self::vector_of(text)
            .iter()
            .flat_map(|v| v.to_le_bytes())
            .collect()
    }
}

#[async_trait]
impl Embedder for FakeEmbedder {
    fn model(&self) -> &str {
        Self::MODEL
    }
    fn dims(&self) -> u32 {
        3
    }
    async fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, EmbedError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        match self.mode {
            EmbedMode::Good => Ok(texts.iter().map(|t| Self::vector_of(t)).collect()),
            EmbedMode::Failing => Err(EmbedError::Unavailable("한도 초과".to_string())),
            EmbedMode::WrongDims => Ok(texts.iter().map(|_| vec![1.0, 2.0]).collect()),
        }
    }
}

/// 가짜 매매 요약.
pub struct FakeCatalog(pub Vec<TradeRecord>);

impl TradeCatalog for FakeCatalog {
    fn trades(&self) -> Vec<TradeRecord> {
        self.0.clone()
    }
}

/// 매매 요약 한 건.
pub fn trade(record_ref: &str, symbol: &str, closed_at: i64, pnl: Option<i64>) -> TradeRecord {
    TradeRecord {
        record_ref: record_ref.to_string(),
        symbol: symbol.to_string(),
        closed_at: UnixMillis(closed_at),
        pnl: pnl.map(Decimal::from),
    }
}

/// 조건: 아무것도 안 거르는 것.
pub fn any_condition() -> Condition {
    Condition::default()
}

/// 시험 설정: 기본 설정에서 문턱만 없앤다(작은 시험 자료에서는 점수가 문턱 아래로 나오기 쉽다).
pub fn test_config() -> SearchConfig {
    SearchConfig {
        min_score_micros: 0,
        ..SearchConfig::default()
    }
}

/// 임시 폴더의 저장소와 지식 창구.
pub struct Fixture {
    pub dir: TempDir,
    pub store: Store,
    pub knowledge: Knowledge,
}

impl Fixture {
    pub fn new() -> Fixture {
        Fixture::with_config(test_config())
    }

    pub fn with_config(config: SearchConfig) -> Fixture {
        let dir = TempDir::new().expect("임시 폴더");
        let store = Store::open(dir.path().join("pi.db")).expect("저장소 열기");
        let knowledge = Knowledge::new(store.clone(), config);
        Fixture {
            dir,
            store,
            knowledge,
        }
    }

    pub fn with_embedder(embedder: Arc<FakeEmbedder>) -> Fixture {
        let mut f = Fixture::new();
        f.knowledge = Knowledge::new(f.store.clone(), test_config()).with_embedder(embedder);
        f
    }

    /// 같은 파일을 새 손잡이로 다시 연다(앞 손잡이는 버린다).
    pub fn reopen(self) -> Fixture {
        let Fixture {
            dir,
            store,
            knowledge,
        } = self;
        drop(knowledge);
        drop(store);
        let store = Store::open(dir.path().join("pi.db")).expect("다시 열기");
        let knowledge = Knowledge::new(store.clone(), test_config());
        Fixture {
            dir,
            store,
            knowledge,
        }
    }

    /// 내가 쓴 매매 기록 하나를 올린다.
    pub fn add_trade(&self, record_ref: &str, text: &str) {
        self.add(
            KnowledgeKind::Trade,
            KnowledgeSource::Mine,
            record_ref,
            text,
            T0,
        );
    }

    pub fn add(
        &self,
        kind: KnowledgeKind,
        source: KnowledgeSource,
        record_ref: &str,
        text: &str,
        at: i64,
    ) {
        self.knowledge
            .index(record(kind, source, record_ref, text, at))
            .expect("색인");
    }

    /// 키워드로 찾는다("지금"은 T0).
    pub fn find(&self, text: &str) -> SearchOutput {
        self.knowledge
            .search(&SearchRequest::keywords(text), UnixMillis(T0))
            .expect("검색")
    }
}

pub fn record(
    kind: KnowledgeKind,
    source: KnowledgeSource,
    record_ref: &str,
    text: &str,
    at: i64,
) -> RecordText {
    RecordText {
        kind,
        source,
        record_ref: record_ref.to_string(),
        text: text.to_string(),
        at: UnixMillis(at),
    }
}

/// 결과의 기록 키들(순위 순서).
pub fn refs(out: &SearchOutput) -> Vec<String> {
    out.results.iter().map(|h| h.record_ref.clone()).collect()
}

/// 한 줄에 낱말 하나씩, `w0001`부터 `n`개. 낱말 수 = 줄 수.
pub fn word_lines(n: usize) -> String {
    (1..=n)
        .map(|i| format!("w{i:04}"))
        .collect::<Vec<_>>()
        .join("\n")
}
