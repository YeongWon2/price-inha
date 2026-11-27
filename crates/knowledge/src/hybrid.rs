//! (6차, 4차 4번) 뜻 검색 몸통: 키워드 + 벡터 하이브리드(원본 OpenClaw `memory_search`,
//! `docs/research/triggers-approval-memory-deep.md` 4.1의 1~9).
//!
//! - 1번: 벡터 후보 · 키워드 후보를 각각 `max × candidate_multiplier`개 뽑는다. 벡터 점수는 현재 모델의 조각
//!   벡터(정수 양자화면 되살린 것)와 질의 벡터의 코사인(0 아래는 0)이다.
//! - 2 · 3번: 키워드 후보 · 점수는 [`crate::Knowledge::search`]와 같다(AND → OR, 순위 없는 적중은 가장 센 것).
//! - 4번: 키워드 점수 다시 재기 `r(s) = s / (1 - s)`, `share = r(s) / max r`, `min(s, share × 가장 높은 벡터 점수)`.
//! - 5번: 합 = [`VECTOR_WEIGHT_MICROS`] × 벡터 + [`TEXT_WEIGHT_MICROS`] × 다시 잰 키워드.
//! - 6 · 8 · 9번: 최신성 감쇠(문턱은 감쇠 전 점수로), MMR, 모두 문턱 아래면 키워드 결과로 물러선다.
//!
//! 쓸 수 없을 때는 오류가 아니라 결과다: 포트 없음 → 빈 결과 + `no_embedder`, 남은 색인 설정의 모델 · 차원이
//! 포트와 다름(다시 만들기가 아직 안 끝남) → 키워드 결과 + `reindexing`, 질의 임베딩이 `Unavailable` →
//! `embedder_unavailable`, `Failed`(또는 차원이 다름) → `embedder_failed`.

use std::collections::BTreeMap;

use pi_store::{ChunkId, KnowledgeChunk, KnowledgeHit, KnowledgeSource, UnixMillis};

use crate::rank::{Candidate, decay_micros, mmr_select, token_set};
use crate::{
    EmbedError, Error, Knowledge, SearchMode, SearchOutput, SearchRequest, SemanticStatus,
    SharedEmbedder,
};

/// 합에서 벡터 점수의 몫(원본 `vectorWeight = 0.7`, 설정이 아니라 코드에 박힌 값).
pub const VECTOR_WEIGHT_MICROS: i64 = 700_000;
/// 합에서 다시 잰 키워드 점수의 몫(원본 `textWeight = 0.3`).
pub const TEXT_WEIGHT_MICROS: i64 = 300_000;

const ONE: i64 = 1_000_000;
/// 조각 벡터를 한 번에 읽는 수.
const PAGE: usize = 512;
/// 점수가 1인 키워드 적중의 `r(s)`(끝없이 큰 값 대신).
const STRONGEST: i128 = i128::MAX / 4;

/// 합칠 후보 하나.
struct Joined {
    chunk: KnowledgeChunk,
    vector: i64,
    keyword: Option<i64>,
}

fn unavailable(code: &str, warning: &str, action: &str) -> SemanticStatus {
    SemanticStatus::Unavailable {
        warning: warning.to_string(),
        action: action.to_string(),
        code: code.to_string(),
    }
}

/// 원본 4번의 `r(s) = s / (1 - s)`(백만분의 1).
fn unsaturate(score: i64) -> i128 {
    let score = i128::from(score.clamp(0, ONE));
    let one = i128::from(ONE);
    if score >= one {
        STRONGEST
    } else {
        score * one / (one - score)
    }
}

/// 원본 4번: 가장 센 키워드 적중 대비 몫으로, 가장 높은 벡터 점수를 넘지 않게 자른다.
fn rescale(score: i64, max_r: i128, max_vector: i64) -> i64 {
    if max_r <= 0 {
        return 0;
    }
    let share = unsaturate(score).saturating_mul(i128::from(ONE)) / max_r;
    let capped = share.saturating_mul(i128::from(max_vector)) / i128::from(ONE);
    score.min(i64::try_from(capped).unwrap_or(i64::MAX))
}

/// 코사인을 백만분의 1 점수로(0 아래는 0).
fn vector_micros(cosine: f32) -> i64 {
    let clamped = f64::from(cosine).clamp(0.0, 1.0);
    // 0~1을 백만 배 했으므로 i64에 그대로 들어간다.
    (clamped * 1_000_000.0).round() as i64
}

impl Knowledge {
    /// 키워드 + 벡터 하이브리드 검색(종류 · 등급 · `auto_recall` 거르기는 [`Knowledge::search`]와 같다).
    /// 쓸 수 없는 경우는 오류가 아니라 결과의 [`SemanticStatus::Unavailable`]이다(모듈 머리말).
    pub async fn hybrid_search(
        &self,
        request: &SearchRequest,
        now: UnixMillis,
    ) -> Result<SearchOutput, Error> {
        let Some(embedder) = &self.embedder else {
            return Ok(SearchOutput {
                results: Vec::new(),
                mode: SearchMode::Semantic,
                relaxed: false,
                semantic: self.semantic_status(),
            });
        };
        let max = request.max.unwrap_or(self.config.max_results);
        if request.text.split_whitespace().next().is_none() || max == 0 {
            return Ok(SearchOutput {
                results: Vec::new(),
                mode: SearchMode::Semantic,
                relaxed: false,
                semantic: SemanticStatus::Available,
            });
        }
        if !self.vectors_ready(embedder)? {
            return self.fall_back(
                request,
                now,
                unavailable(
                    "reindexing",
                    "뜻 검색 색인을 아직 다 만들지 않았다(모델이 바뀌었거나 처음 만드는 중).",
                    "다 만들 때까지 키워드 결과를 쓴다. 잠시 뒤 다시 찾는다.",
                ),
            );
        }
        let query = match embedder.embed_query(&request.text).await {
            Ok(query) if u32::try_from(query.len()).ok() == Some(embedder.dims()) => query,
            Ok(_) | Err(EmbedError::Failed(_)) => {
                return self.fall_back(
                    request,
                    now,
                    unavailable(
                        "embedder_failed",
                        "임베딩 공급자가 질의를 바꾸지 못했다.",
                        "키워드 결과를 쓴다. 공급자 설정을 확인한다.",
                    ),
                );
            }
            Err(EmbedError::Unavailable(_)) => {
                return self.fall_back(
                    request,
                    now,
                    unavailable(
                        "embedder_unavailable",
                        "임베딩 공급자에 지금 닿지 않는다.",
                        "키워드 결과를 쓴다. 공급자가 켜져 있는지 확인한다.",
                    ),
                );
            }
        };
        self.run_hybrid(request, now, (embedder.model(), &query), max)
    }

    /// 남은 색인 설정이 이 포트의 모델 · 차원으로 다 만들어졌나.
    fn vectors_ready(&self, embedder: &SharedEmbedder) -> Result<bool, Error> {
        Ok(self.stored_settings()?.is_some_and(|stored| {
            stored.model == embedder.model() && stored.dims == Some(embedder.dims())
        }))
    }

    /// 키워드 결과에 `semantic`을 바꿔 싣는다.
    fn fall_back(
        &self,
        request: &SearchRequest,
        now: UnixMillis,
        semantic: SemanticStatus,
    ) -> Result<SearchOutput, Error> {
        let mut output = self.run_search(request, now)?;
        output.semantic = semantic;
        Ok(output)
    }

    fn run_hybrid(
        &self,
        request: &SearchRequest,
        now: UnixMillis,
        (model, query): (&str, &[f32]),
        max: usize,
    ) -> Result<SearchOutput, Error> {
        let candidates = max.saturating_mul(self.config.candidate_multiplier.max(1));
        let vectors = self.vector_candidates(model, query, request, candidates)?;
        let (keywords, relaxed) = self.keyword_hits(request, candidates)?;
        let joined = join(vectors, keywords);
        let threshold = self.config.min_score_micros;
        let scored: Vec<(KnowledgeChunk, i64)> = joined
            .into_iter()
            .filter(|(_, total)| *total >= threshold)
            .collect();
        if scored.is_empty() {
            // 원본 9번: 가중 점수가 모두 문턱 아래면 키워드 결과로 물러선다.
            return self.fall_back(request, now, SemanticStatus::Available);
        }
        let mut ranked: Vec<(KnowledgeChunk, i64)> = scored
            .into_iter()
            .map(|(chunk, total)| {
                let age = now.0.saturating_sub(chunk.updated_at.0);
                let decay = decay_micros(age, self.config.half_life_days);
                let shown = i128::from(total) * i128::from(decay) / i128::from(ONE);
                (chunk, i64::try_from(shown).unwrap_or(0))
            })
            .collect();
        ranked.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.id.cmp(&b.0.id)));
        let pool: Vec<Candidate> = ranked
            .iter()
            .map(|(chunk, rank)| Candidate {
                rank: *rank,
                tokens: token_set(&chunk.text),
            })
            .collect();
        let order = mmr_select(&pool, max, self.config.mmr_lambda_micros);
        let mut results = Vec::with_capacity(order.len());
        for index in order {
            let (chunk, shown) = &ranked[index];
            results.push(self.tool_hit(chunk, Some(*shown))?);
        }
        Ok(SearchOutput {
            results,
            mode: SearchMode::Semantic,
            relaxed,
            semantic: SemanticStatus::Available,
        })
    }

    /// 그 모델의 조각 벡터를 쪽마다 훑어 코사인 점수가 높은 `limit`개(0보다 큰 것만).
    fn vector_candidates(
        &self,
        model: &str,
        query: &[f32],
        request: &SearchRequest,
        limit: usize,
    ) -> Result<Vec<(KnowledgeChunk, i64)>, Error> {
        let mut scored: Vec<(KnowledgeChunk, i64)> = Vec::new();
        let mut after: Option<ChunkId> = None;
        loop {
            let page = self.store.chunk_vectors(model, after, PAGE)?;
            let Some(last) = page.last() else { break };
            after = Some(last.chunk.id);
            let full = page.len() == PAGE;
            for item in page {
                if !allowed(request, &item.chunk) {
                    continue;
                }
                let Some(vector) = crate::quant::vector_of(&item.embedding) else {
                    continue;
                };
                if vector.len() != query.len() {
                    continue;
                }
                let score = vector_micros(crate::quant::cosine(query, &vector));
                if score > 0 {
                    scored.push((item.chunk, score));
                }
            }
            if !full {
                break;
            }
        }
        scored.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.id.cmp(&b.0.id)));
        scored.truncate(limit);
        Ok(scored)
    }
}

/// 종류 · 등급 · 자동 회상 거르기([`pi_store::KnowledgeQuery`]와 같은 뜻).
fn allowed(request: &SearchRequest, chunk: &KnowledgeChunk) -> bool {
    if request.auto_recall && chunk.source == KnowledgeSource::External {
        return false;
    }
    (request.kinds.is_empty() || request.kinds.contains(&chunk.kind))
        && (request.sources.is_empty() || request.sources.contains(&chunk.source))
}

/// 벡터 후보와 키워드 후보를 조각마다 합쳐 (조각, 합 점수)로(원본 4 · 5번).
fn join(
    vectors: Vec<(KnowledgeChunk, i64)>,
    keywords: Vec<KnowledgeHit>,
) -> Vec<(KnowledgeChunk, i64)> {
    let max_vector = vectors.iter().map(|(_, v)| *v).max().unwrap_or(0);
    let keyword_score = |hit: &KnowledgeHit| hit.score_micros.unwrap_or(ONE);
    let max_r = keywords
        .iter()
        .map(|hit| unsaturate(keyword_score(hit)))
        .max()
        .unwrap_or(0);
    let mut merged: BTreeMap<ChunkId, Joined> = BTreeMap::new();
    for (chunk, vector) in vectors {
        merged.insert(
            chunk.id,
            Joined {
                chunk,
                vector,
                keyword: None,
            },
        );
    }
    for hit in keywords {
        let text = rescale(keyword_score(&hit), max_r, max_vector);
        merged
            .entry(hit.chunk.id)
            .and_modify(|joined| joined.keyword = Some(text))
            .or_insert(Joined {
                chunk: hit.chunk,
                vector: 0,
                keyword: Some(text),
            });
    }
    merged
        .into_values()
        .map(|joined| {
            let text = joined.keyword.unwrap_or(0);
            let total = (i128::from(VECTOR_WEIGHT_MICROS) * i128::from(joined.vector)
                + i128::from(TEXT_WEIGHT_MICROS) * i128::from(text))
                / i128::from(ONE);
            (joined.chunk, i64::try_from(total).unwrap_or(0))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 다시_잰_키워드_점수는_가장_높은_벡터_점수를_넘지_않는다() {
        let max_r = unsaturate(800_000);
        assert_eq!(rescale(800_000, max_r, 500_000), 500_000);
        assert!(rescale(400_000, max_r, 500_000) < 500_000);
        assert_eq!(rescale(800_000, max_r, 0), 0);
    }
}
