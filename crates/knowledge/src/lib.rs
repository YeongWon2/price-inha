//! pi-knowledge: `docs/dev/modules/knowledge.md` 를 따른다. 지식 검색(A12의 검색 부분, T16).
//!
//! 이번 차수는 키워드 · 조건 검색이다. 임베딩은 [`Embedder`] 포트로 받고, 없으면 뜻 검색은
//! "쓸 수 없음"을 결과로 돌려준다(오류가 아니다, D10). 시각은 모두 인자로 받는다.

mod chunk;
mod condition;
mod embed;
pub mod error;
mod output;
mod reindex;

pub use chunk::{CHUNK_OVERLAP, CHUNK_TOKENS, INDEX_PREFIX_CHARS, TextChunk, split_chunks};
pub use condition::{Condition, TradeCatalog, TradeRecord};
pub use embed::{EmbedReport, Embedder, SemanticStatus, SharedEmbedder};
pub use error::{EmbedError, Error};
pub use output::{SNIPPET_CHARS, SearchMode, SearchOutput, ToolHit};
pub use reindex::{IndexSettings, ReindexPlan, ReindexReason};

use pi_store::{ChunkId, KnowledgeKind, KnowledgeSource, Store, UnixMillis};

/// 검색 설정(원본 기본값: 4.1). 가중 · 문턱은 백만분의 1 정수다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchConfig {
    /// 가장 많이 줄 개수(기본 6).
    pub max_results: usize,
    /// 후보 배수(기본 4): 최종 개수의 몇 배를 먼저 뽑나.
    pub candidate_multiplier: usize,
    /// 최신성 반감기, 일 단위(기본 30). 점수가 이 기간마다 반으로 준다.
    pub half_life_days: u32,
    /// 점수 문턱(기본 350_000). 모두 문턱 아래면 걸러 내지 않고 그대로 둔다(원본 9번).
    pub min_score_micros: i64,
    /// MMR의 람다(기본 700_000). 1_000_000이면 점수순 그대로.
    pub mmr_lambda_micros: i64,
}

impl Default for SearchConfig {
    fn default() -> Self {
        todo!()
    }
}

/// 색인에 올릴 기록(글 한 덩어리). 조각으로 나누는 것은 이 모듈이다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordText {
    /// 기록 종류.
    pub kind: KnowledgeKind,
    /// 출처 등급.
    pub source: KnowledgeSource,
    /// 기록 키. 비면 안 된다.
    pub record_ref: String,
    /// 글. 낱말이 없으면 안 된다. 앞 [`INDEX_PREFIX_CHARS`]자만 색인한다.
    pub text: String,
    /// 올린 시각.
    pub at: UnixMillis,
}

/// 키워드 검색 요청.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchRequest {
    /// 질의(공백으로 나눈 낱말).
    pub text: String,
    /// 이 종류만. 비면 모두.
    pub kinds: Vec<KnowledgeKind>,
    /// 이 등급만. 비면 모두.
    pub sources: Vec<KnowledgeSource>,
    /// 참이면 자동 회상: 바깥 자료는 나오지 않는다(M7).
    pub auto_recall: bool,
    /// 가장 많이 줄 개수. `None`이면 설정의 기본 개수.
    pub max: Option<usize>,
}

impl SearchRequest {
    /// 종류 · 등급을 거르지 않는 명시 검색(자동 회상 아님).
    pub fn keywords(text: &str) -> SearchRequest {
        SearchRequest {
            text: text.to_string(),
            kinds: Vec::new(),
            sources: Vec::new(),
            auto_recall: false,
            max: None,
        }
    }
}

/// 지식 검색 창구. 저장소 손잡이 하나와 설정을 가진다. 임베딩 포트는 있을 수도 없을 수도 있다.
pub struct Knowledge {
    store: Store,
    config: SearchConfig,
    embedder: Option<SharedEmbedder>,
}

impl Knowledge {
    /// 임베딩 없이(키워드 · 조건 검색만) 연다. 다시 열어도(같은 파일로 새 손잡이) 색인이 이어진다.
    pub fn new(store: Store, config: SearchConfig) -> Knowledge {
        Knowledge {
            store,
            config,
            embedder: None,
        }
    }

    /// 임베딩 포트를 붙인다.
    pub fn with_embedder(mut self, embedder: SharedEmbedder) -> Knowledge {
        self.embedder = Some(embedder);
        self
    }

    /// 기록을 색인에 올린다(앞 8,192자만 → 400/80 조각 → 저장소). 조각 번호를 순서대로 돌려준다.
    ///
    /// - 같은 `(kind, record_ref)`의 글이 이전과 같으면 아무것도 바꾸지 않고 지금 번호를 돌려준다(임베딩도 남는다).
    /// - 글이 바뀌었으면 이전 조각과 임베딩을 지우고 새 번호로 다시 올린다(M7).
    /// - `record_ref`가 비었거나 글에 낱말이 없으면 [`Error::InvalidRecord`]이고 아무것도 바꾸지 않는다.
    pub fn index(&self, record: RecordText) -> Result<Vec<ChunkId>, Error> {
        let _ = (&self.store, record);
        todo!()
    }

    /// 키워드 검색. 모든 낱말(AND)이 맞는 조각을 찾고, 없으면 하나라도(OR) 맞는 것으로 풀어 다시 찾는다.
    /// 3글자 이상 낱말은 trigram으로, 3글자 미만(예: "매수" · "손절")은 부분 문자열로 찾는다.
    /// 점수에 최신성 감쇠(반감기, `now` 기준 조각의 올린 시각)를 곱하고 문턱을 적용한 뒤 MMR로 고른다.
    /// 질의가 비었거나 개수가 0이면 빈 결과다. 바깥 요청을 하지 않는다(V23).
    pub fn search(&self, request: &SearchRequest, now: UnixMillis) -> Result<SearchOutput, Error> {
        let _ = (&self.config, request, now);
        todo!()
    }

    /// 조건으로 찾기. 조건에 맞는 매매마다 그 매매 기록(종류 `trade`)의 첫 조각을 하나씩, 닫힌 시각이
    /// 가장 최근인 것부터(같으면 기록 키 순서) 준다. 색인에 없는 매매는 건너뛴다. 적중의 점수는 없다.
    pub fn search_by_condition(
        &self,
        condition: &Condition,
        catalog: &dyn TradeCatalog,
    ) -> Result<SearchOutput, Error> {
        let _ = (condition, catalog);
        todo!()
    }

    /// 뜻 검색. 임베딩 포트가 없으면 오류가 아니라 빈 결과와 [`SemanticStatus::Unavailable`]
    /// (`code = "no_embedder"`)를 돌려주고 바깥 요청을 하지 않는다(V23).
    pub async fn semantic_search(
        &self,
        text: &str,
        now: UnixMillis,
    ) -> Result<SearchOutput, Error> {
        let _ = (text, now, &self.embedder);
        todo!()
    }

    /// 임베딩이 없는 조각(그 모델 기준)에 임베딩을 `max`개까지 만들어 둔다. 한 번 부를 때 포트는 많아야 한 번 부른다.
    /// 값은 리틀 엔디언 f32 묶음 바이트로 둔다. 포트가 없으면 아무것도 하지 않고 `unavailable`이 참이다.
    /// 포트가 실패했거나 개수 · 차원이 약속과 다르면 [`Error::Embedder`]이고 아무것도 두지 않는다.
    pub async fn embed_missing(&self, max: usize) -> Result<EmbedReport, Error> {
        let _ = max;
        todo!()
    }

    /// 다시 색인할지 정한다. 규칙: 남은 설정이 없으면 `Rebuild(FirstTime)`; 남은 설정에 벡터가 있는데
    /// (`dims`가 있음) `provider_available`이 거짓이면 `SkipProviderUnavailable`(`force`여도); `force`면
    /// `Rebuild(Forced)`; 공급자 · 모델 · 차원 · 조각 설정 · 색인 방식 · 범위 순서로 다른 첫 것이 까닭; 모두 같으면 `Keep`.
    pub fn plan_reindex(
        &self,
        current: &IndexSettings,
        provider_available: bool,
        force: bool,
    ) -> Result<ReindexPlan, Error> {
        let _ = (current, provider_available, force);
        todo!()
    }

    /// 색인의 설정을 남긴다(다시 열어도 남는다).
    pub fn record_settings(&self, settings: &IndexSettings) -> Result<(), Error> {
        let _ = settings;
        todo!()
    }

    /// 남아 있는 색인 설정. 없으면 `None`.
    pub fn stored_settings(&self) -> Result<Option<IndexSettings>, Error> {
        todo!()
    }
}
