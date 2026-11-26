//! pi-knowledge: `docs/dev/modules/knowledge.md` 를 따른다. 지식 검색(A12의 검색 부분, T16).
//!
//! 이번 차수는 키워드 · 조건 검색이다. 임베딩은 [`Embedder`] 포트로 받고, 없으면 뜻 검색은
//! "쓸 수 없음"을 결과로 돌려준다(오류가 아니다, D10). 시각은 모두 인자로 받는다.

mod archive;
mod chunk;
mod condition;
mod embed;
pub mod error;
mod hybrid;
mod meta;
mod output;
mod quant;
mod rank;
mod reindex;
mod search;

pub use archive::{ARCHIVE_REF_PREFIX, OldRecord};
pub use chunk::{CHUNK_OVERLAP, CHUNK_TOKENS, INDEX_PREFIX_CHARS, TextChunk, split_chunks};
pub use condition::{Condition, TradeCatalog, TradeRecord};
pub use embed::{EmbedReport, Embedder, SemanticStatus, SharedEmbedder};
pub use error::{EmbedError, Error};
pub use hybrid::{TEXT_WEIGHT_MICROS, VECTOR_WEIGHT_MICROS};
pub use output::{SNIPPET_CHARS, SearchMode, SearchOutput, ToolHit};
pub use quant::{cosine, dequantize, quantize};
pub use reindex::{IndexSettings, ReembedProgress, ReindexPlan, ReindexReason};

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

impl RecordText {
    /// 복기에서 나온 실수 기록. 종류 `Mistake`, 등급 `Team`, 기록 키 = 매매 키.
    /// 글은 `실수 · 매매 {trade_ref} · 종목 {symbol}\n실수: {a, b}\n{note}`.
    /// 앞뒤 공백을 뗀 실수가 하나도 없으면 `None`(실수 없는 복기는 실수 기록을 만들지 않는다).
    pub fn mistakes(
        trade_ref: &str,
        symbol: &str,
        mistakes: &[String],
        note: &str,
        at: UnixMillis,
    ) -> Option<RecordText> {
        let kept: Vec<&str> = mistakes
            .iter()
            .map(|m| m.trim())
            .filter(|m| !m.is_empty())
            .collect();
        if kept.is_empty() {
            return None;
        }
        Some(RecordText {
            kind: KnowledgeKind::Mistake,
            source: KnowledgeSource::Team,
            record_ref: trade_ref.to_string(),
            text: format!(
                "실수 · 매매 {trade_ref} · 종목 {symbol}\n실수: {}\n{note}",
                kept.join(", ")
            ),
            at,
        })
    }

    /// 담당이 쓴 시장 메모. 종류 `MarketNote`, 글은 그대로. 등급은 `Mine` · `Team`만 받고
    /// `External`이면 [`Error::InvalidRecord`](바깥 자료는 [`RecordText::external`]로만 들어온다).
    pub fn market_note(
        note_ref: &str,
        source: KnowledgeSource,
        text: &str,
        at: UnixMillis,
    ) -> Result<RecordText, Error> {
        if source == KnowledgeSource::External {
            return Err(Error::InvalidRecord(
                "바깥 자료는 시장 메모가 아니라 바깥 자료 길로 올린다".to_string(),
            ));
        }
        Ok(RecordText {
            kind: KnowledgeKind::MarketNote,
            source,
            record_ref: note_ref.to_string(),
            text: text.to_string(),
            at,
        })
    }

    /// 뉴스 · 공시 요약(바깥 자료). 종류 `MarketNote`, 등급 `External`, 글은 `{title}\n{summary}\n{link}`,
    /// 시각은 공개 시각. 자동 회상에는 나오지 않는다(M7).
    pub fn external(
        item_ref: &str,
        title: &str,
        summary: &str,
        link: &str,
        published_at: UnixMillis,
    ) -> RecordText {
        RecordText {
            kind: KnowledgeKind::MarketNote,
            source: KnowledgeSource::External,
            record_ref: item_ref.to_string(),
            text: format!("{title}\n{summary}\n{link}"),
            at: published_at,
        }
    }
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
            // 앞서 조각 저장과 메타 저장 사이에 꺼졌으면 빠진 메타를 여기서 채운다.
            for (old, chunk) in existing.iter().zip(&chunks) {
                if meta::origin_of(&self.store, old.id)?.is_none() {
                    let lines = (chunk.start_line, chunk.end_line);
                    meta::remember_chunk(
                        &self.store,
                        old.id,
                        record.kind,
                        &record.record_ref,
                        lines,
                    )?;
                }
            }
            return Ok(existing.iter().map(|c| c.id).collect());
        }
        // 조각과 줄 범위 메타를 한 묶음에서 올린다. 사라진 조각의 메타 키도 같은 묶음에서 지운다.
        let values: Vec<String> = chunks
            .iter()
            .map(|c| meta::chunk_value(record.kind, &record.record_ref, (c.start_line, c.end_line)))
            .collect();
        let ids = self.store.index_record_with_meta(
            NewKnowledgeRecord {
                kind: record.kind,
                source: record.source,
                record_ref: record.record_ref.clone(),
                chunks: chunks.iter().map(|c| c.text.clone()).collect(),
                at: record.at,
            },
            meta::CHUNK_KEY_PREFIX,
            &values,
        )?;
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
        self.run_condition(condition, catalog, false)
    }

    /// 자동 회상용 조건 찾기. [`Knowledge::search_by_condition`]과 같되 바깥 자료(`External`)의 조각은
    /// 건너뛴다(M7). 건너뛴 것은 개수에 들지 않는다.
    pub fn recall_by_condition(
        &self,
        condition: &Condition,
        catalog: &dyn TradeCatalog,
    ) -> Result<SearchOutput, Error> {
        self.run_condition(condition, catalog, true)
    }

    fn run_condition(
        &self,
        condition: &Condition,
        catalog: &dyn TradeCatalog,
        auto_recall: bool,
    ) -> Result<SearchOutput, Error> {
        let mut trades: Vec<TradeRecord> = catalog
            .trades()
            .into_iter()
            .filter(|t| condition.matches(t) && condition.matches_mistake(catalog, &t.record_ref))
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
            let Some(first) = chunks.first() else {
                continue;
            };
            if auto_recall && first.source == KnowledgeSource::External {
                continue;
            }
            results.push(self.tool_hit(first, None)?);
        }
        Ok(SearchOutput {
            results,
            mode: SearchMode::Condition,
            relaxed: false,
            semantic: self.semantic_status(),
        })
    }

    /// 뜻 검색. 임베딩 포트가 없으면 오류가 아니라 빈 결과와 [`SemanticStatus::Unavailable`]
    /// (`code = "no_embedder"`)를 돌려주고 바깥 요청을 하지 않는다(V23). (6차) 포트가 있으면
    /// [`Knowledge::hybrid_search`]`(&SearchRequest::keywords(text), now)`와 같다.
    pub async fn semantic_search(
        &self,
        text: &str,
        now: UnixMillis,
    ) -> Result<SearchOutput, Error> {
        self.hybrid_search(&SearchRequest::keywords(text), now)
            .await
    }

    /// 임베딩이 없는 조각(그 모델 기준)에 임베딩을 `max`개까지 만들어 둔다. 한 번 부를 때 포트는 많아야 한 번 부른다.
    /// (2차, 결정 31) 값은 정수 양자화 바이트([`quantize`])로 둔다. 포트가 없으면 아무것도 하지 않고 `unavailable`이 참이다.
    /// 포트가 실패했거나 개수 · 차원이 약속과 다르면 [`Error::Embedder`]이고 아무것도 두지 않는다.
    pub async fn embed_missing(&self, max: usize) -> Result<EmbedReport, Error> {
        self.run_embed_missing(max).await
    }

    /// 임베딩 다시 만들기의 한 걸음. 끝까지 도는 일 · 쉬는 간격 · 취소는 부르는 쪽이 한다.
    ///
    /// 1. 포트가 없으면 [`ReembedProgress::Unavailable`]: 저장소 · 남은 설정을 바꾸지 않고 바깥 요청 0건(V23).
    /// 2. `current.model` · `current.dims`가 포트의 `model()` · `Some(dims())`와 다르면 [`Error::Embedder`]이고 아무것도 바꾸지 않는다.
    /// 3. [`Knowledge::embed_missing`]`(batch)`를 한 번 부른다(포트 호출 많아야 1번). 모델이 바뀌면 모든 조각이
    ///    새 모델 기준으로 "빠짐"이라 차례로 다시 만들어지고, 글이 바뀐 기록도 같은 길로 다시 만들어진다(M7).
    ///    남은 설정의 모델이 `current.model`과 같고 차원이 둘 다 있는데 값이 다르면 포트를 부르지 않고
    ///    [`Error::Embedder`]다(설정 그대로). 남은 차원이 없으면(끊긴 설정) 막지 않고 이어서 한다.
    ///    `batch`가 0이면 포트를 부르지 않고 설정도 남기지 않으며 `InProgress { embedded: 0 }`이다.
    /// 4. 이번에 둔 것이 있으면 `InProgress`. 빠진 조각이 없으면 그때 처음으로 `record_settings(current)`를 하고 `Done`.
    /// 5. 포트가 실패하면 `Err`이고 남은 설정은 그대로다(다음 부름이 이어서 한다).
    pub async fn reembed_step(
        &self,
        current: &IndexSettings,
        batch: usize,
    ) -> Result<ReembedProgress, Error> {
        let Some(embedder) = &self.embedder else {
            return Ok(ReembedProgress::Unavailable);
        };
        if current.model != embedder.model() || current.dims != Some(embedder.dims()) {
            return Err(Error::Embedder(
                "현재 색인 설정의 모델이나 차원이 붙은 임베딩 공급자와 다르다".to_string(),
            ));
        }
        // 4-2: 모델 이름은 같고 차원만 다르면 저장소가 옛 차원 벡터를 "빠짐"으로 고르지 못한다(모델만 본다).
        // 다 됐다고 하지 않고, 설정도 남기지 않고, 포트도 부르지 않는다.
        // 4-3: 남은 차원이 없으면(끊긴 설정 · 키워드 전용) 막지 않는다. 이어서 만들고 `Done`과 함께 다시 남긴다.
        if let Some(stored) = self.stored_settings()?
            && stored.model == current.model
            && let (Some(stored_dims), Some(current_dims)) = (stored.dims, current.dims)
            && stored_dims != current_dims
        {
            return Err(Error::Embedder(
                "모델 이름은 같은데 차원이 다르다. 차원을 바꾸려면 모델 이름을 바꾸거나 색인을 다시 올려야 한다"
                    .to_string(),
            ));
        }
        // 4-1: 이번에 하나도 만들지 않는 것은 "빠진 것이 없다"가 아니다.
        if batch == 0 {
            return Ok(ReembedProgress::InProgress { embedded: 0 });
        }
        let report = self.run_embed_missing(batch).await?;
        if report.embedded > 0 {
            return Ok(ReembedProgress::InProgress {
                embedded: report.embedded,
            });
        }
        // 다 만들어졌다. 남기기 전의 계획(무엇이 바뀌어 다시 만들었나)을 함께 준다.
        let plan = self.plan_reindex(current, true, false)?;
        self.record_settings(current)?;
        Ok(ReembedProgress::Done {
            embedded: report.embedded,
            plan,
        })
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
