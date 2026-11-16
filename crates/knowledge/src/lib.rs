//! pi-knowledge: `docs/dev/modules/knowledge.md` 를 따른다. 지식 검색(A12의 검색 부분, T16).
//!
//! 이번 차수는 키워드 · 조건 검색이다. 임베딩은 [`Embedder`] 포트로 받고, 없으면 뜻 검색은
//! "쓸 수 없음"을 결과로 돌려준다(오류가 아니다, D10). 시각은 모두 인자로 받는다.

mod chunk;
mod condition;
mod embed;
pub mod error;
mod meta;
mod output;
mod rank;
mod reindex;
mod search;

pub use chunk::{CHUNK_OVERLAP, CHUNK_TOKENS, INDEX_PREFIX_CHARS, TextChunk, split_chunks};
pub use condition::{Condition, TradeCatalog, TradeRecord};
pub use embed::{EmbedReport, Embedder, SemanticStatus, SharedEmbedder};
pub use error::{EmbedError, Error};
pub use output::{SNIPPET_CHARS, SearchMode, SearchOutput, ToolHit};
pub use reindex::{IndexSettings, ReindexPlan, ReindexReason};

use pi_store::{ChunkId, KnowledgeKind, KnowledgeSource, NewKnowledgeRecord, Store, UnixMillis};

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
        SearchConfig {
            max_results: 6,
            candidate_multiplier: 4,
            half_life_days: 30,
            min_score_micros: 350_000,
            mmr_lambda_micros: 700_000,
        }
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
        if record.record_ref.is_empty() {
            return Err(Error::InvalidRecord("기록 키가 비었다".to_string()));
        }
        let prefix: String = record.text.chars().take(INDEX_PREFIX_CHARS).collect();
        let chunks = split_chunks(&prefix, CHUNK_TOKENS, CHUNK_OVERLAP);
        if chunks.is_empty() {
            return Err(Error::InvalidRecord("글에 낱말이 없다".to_string()));
        }
        let existing = self
            .store
            .knowledge_chunks(record.kind, &record.record_ref)?;
        let unchanged = existing.len() == chunks.len()
            && existing
                .iter()
                .zip(&chunks)
                .all(|(old, new)| old.text == new.text && old.source == record.source);
        if unchanged {
            return Ok(existing.iter().map(|c| c.id).collect());
        }
        let ids = self.store.index_record(NewKnowledgeRecord {
            kind: record.kind,
            source: record.source,
            record_ref: record.record_ref.clone(),
            chunks: chunks.iter().map(|c| c.text.clone()).collect(),
            at: record.at,
        })?;
        for (id, chunk) in ids.iter().zip(&chunks) {
            let lines = (chunk.start_line, chunk.end_line);
            meta::remember_chunk(&self.store, *id, record.kind, &record.record_ref, lines)?;
        }
        Ok(ids)
    }

    /// 키워드 검색. 모든 낱말(AND)이 맞는 조각을 찾고, 없으면 하나라도(OR) 맞는 것으로 풀어 다시 찾는다.
    /// 3글자 이상 낱말은 trigram으로, 3글자 미만(예: "매수" · "손절")은 부분 문자열로 찾는다.
    /// 점수에 최신성 감쇠(반감기, `now` 기준 조각의 올린 시각)를 곱하고 문턱을 적용한 뒤 MMR로 고른다.
    /// 질의가 비었거나 개수가 0이면 빈 결과다. 바깥 요청을 하지 않는다(V23).
    pub fn search(&self, request: &SearchRequest, now: UnixMillis) -> Result<SearchOutput, Error> {
        self.run_search(request, now)
    }

    /// 조건으로 찾기. 조건에 맞는 매매마다 그 매매 기록(종류 `trade`)의 첫 조각을 하나씩, 닫힌 시각이
    /// 가장 최근인 것부터(같으면 기록 키 순서) 준다. 색인에 없는 매매는 건너뛴다. 적중의 점수는 없다.
    pub fn search_by_condition(
        &self,
        condition: &Condition,
        catalog: &dyn TradeCatalog,
    ) -> Result<SearchOutput, Error> {
        let mut trades: Vec<TradeRecord> = catalog
            .trades()
            .into_iter()
            .filter(|t| condition.matches(t))
            .collect();
        trades.sort_by(|a, b| {
            b.closed_at
                .0
                .cmp(&a.closed_at.0)
                .then_with(|| a.record_ref.cmp(&b.record_ref))
        });
        let max = condition.max.unwrap_or(self.config.max_results);
        let mut results = Vec::new();
        for trade in trades {
            if results.len() >= max {
                break;
            }
            let chunks = self
                .store
                .knowledge_chunks(KnowledgeKind::Trade, &trade.record_ref)?;
            if let Some(first) = chunks.first() {
                results.push(self.tool_hit(first, None)?);
            }
        }
        Ok(SearchOutput {
            results,
            mode: SearchMode::Condition,
            relaxed: false,
            semantic: self.semantic_status(),
        })
    }

    /// 자동 회상용 조건 찾기. [`Knowledge::search_by_condition`]과 같되 바깥 자료는 나오지 않는다(M7).
    pub fn recall_by_condition(
        &self,
        condition: &Condition,
        catalog: &dyn TradeCatalog,
    ) -> Result<SearchOutput, Error> {
        self.search_by_condition(condition, catalog)
    }

    /// 뜻 검색. 임베딩 포트가 없으면 오류가 아니라 빈 결과와 [`SemanticStatus::Unavailable`]
    /// (`code = "no_embedder"`)를 돌려주고 바깥 요청을 하지 않는다(V23).
    pub async fn semantic_search(
        &self,
        text: &str,
        now: UnixMillis,
    ) -> Result<SearchOutput, Error> {
        let _ = (text, now);
        let semantic = match &self.embedder {
            None => self.semantic_status(),
            // 1차 범위 밖: 조각의 벡터를 훑어 견주는 길은 공급자를 고른 뒤에 붙인다(결정 2026-11-16 11번).
            Some(_) => SemanticStatus::Unavailable {
                warning: "뜻 검색은 아직 붙이지 않았다.".to_string(),
                action: "키워드나 조건으로 찾는다.".to_string(),
                code: "semantic_not_wired".to_string(),
            },
        };
        Ok(SearchOutput {
            results: Vec::new(),
            mode: SearchMode::Semantic,
            relaxed: false,
            semantic,
        })
    }

    /// 임베딩이 없는 조각(그 모델 기준)에 임베딩을 `max`개까지 만들어 둔다. 한 번 부를 때 포트는 많아야 한 번 부른다.
    /// 값은 리틀 엔디언 f32 묶음 바이트로 둔다. 포트가 없으면 아무것도 하지 않고 `unavailable`이 참이다.
    /// 포트가 실패했거나 개수 · 차원이 약속과 다르면 [`Error::Embedder`]이고 아무것도 두지 않는다.
    pub async fn embed_missing(&self, max: usize) -> Result<EmbedReport, Error> {
        self.run_embed_missing(max).await
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
        let Some(stored) = self.stored_settings()? else {
            return Ok(ReindexPlan::Rebuild(ReindexReason::FirstTime));
        };
        if stored.dims.is_some() && !provider_available {
            return Ok(ReindexPlan::SkipProviderUnavailable);
        }
        if force {
            return Ok(ReindexPlan::Rebuild(ReindexReason::Forced));
        }
        Ok(match changed_reason(&stored, current) {
            Some(reason) => ReindexPlan::Rebuild(reason),
            None => ReindexPlan::Keep,
        })
    }

    /// 색인의 설정을 남긴다(다시 열어도 남는다).
    pub fn record_settings(&self, settings: &IndexSettings) -> Result<(), Error> {
        meta::save_settings(&self.store, settings)
    }

    /// 남아 있는 색인 설정. 없으면 `None`.
    pub fn stored_settings(&self) -> Result<Option<IndexSettings>, Error> {
        meta::load_settings(&self.store)
    }
}

/// 남은 설정과 현재 설정이 다른 첫 까닭(공급자 · 모델 · 차원 · 조각 설정 · 색인 방식 · 범위 순서).
fn changed_reason(stored: &IndexSettings, current: &IndexSettings) -> Option<ReindexReason> {
    if stored.provider != current.provider {
        Some(ReindexReason::ProviderChanged)
    } else if stored.model != current.model {
        Some(ReindexReason::ModelChanged)
    } else if stored.dims != current.dims {
        Some(ReindexReason::DimsChanged)
    } else if stored.chunk_tokens != current.chunk_tokens
        || stored.chunk_overlap != current.chunk_overlap
    {
        Some(ReindexReason::ChunkSettingsChanged)
    } else if stored.tokenizer != current.tokenizer {
        Some(ReindexReason::TokenizerChanged)
    } else if stored.scope_hash != current.scope_hash {
        Some(ReindexReason::ScopeChanged)
    } else {
        None
    }
}
