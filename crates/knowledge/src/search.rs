//! 검색의 안쪽: 후보 모으기(AND → OR), 최신성 감쇠, 문턱, MMR, 도구가 돌려주는 모양 만들기.

use std::collections::BTreeMap;

use pi_store::{KnowledgeChunk, KnowledgeHit, KnowledgeQuery, UnixMillis};

use crate::meta::{kind_code, lines_of, source_code};
use crate::rank::{Candidate, decay_micros, mmr_select, token_set};
use crate::{
    Error, Knowledge, SNIPPET_CHARS, SearchMode, SearchOutput, SearchRequest, SemanticStatus,
    ToolHit,
};

const SCORE_MAX: i64 = 1_000_000;

/// 감쇠까지 곱한 적중.
struct Ranked {
    chunk: KnowledgeChunk,
    /// 도구에 내보내는 점수(순위가 없던 적중은 `None`).
    shown: Option<i64>,
    /// 순위를 가리는 점수(순위가 없던 적중은 감쇠 계수만으로).
    rank: i64,
}

impl Knowledge {
    /// 뜻 검색을 쓸 수 있는지(포트가 붙어 있으면 쓸 수 있다).
    pub(crate) fn semantic_status(&self) -> SemanticStatus {
        if self.embedder.is_some() {
            SemanticStatus::Available
        } else {
            SemanticStatus::Unavailable {
                warning: "임베딩 공급자가 붙어 있지 않아 뜻 검색을 쓸 수 없다.".to_string(),
                action: "키워드나 조건으로 찾거나, 임베딩 공급자를 고른 뒤 붙인다.".to_string(),
                code: "no_embedder".to_string(),
            }
        }
    }

    pub(crate) fn run_search(
        &self,
        request: &SearchRequest,
        now: UnixMillis,
    ) -> Result<SearchOutput, Error> {
        let words: Vec<&str> = request.text.split_whitespace().collect();
        let max = request.max.unwrap_or(self.config.max_results);
        let mut output = SearchOutput {
            results: Vec::new(),
            mode: SearchMode::Keyword,
            relaxed: false,
            semantic: self.semantic_status(),
        };
        if words.is_empty() || max == 0 {
            return Ok(output);
        }
        let candidates = max.saturating_mul(self.config.candidate_multiplier.max(1));
        let query = |text: &str| KnowledgeQuery {
            text: text.to_string(),
            kinds: request.kinds.clone(),
            sources: request.sources.clone(),
            auto_recall: request.auto_recall,
            max: candidates,
        };
        let mut hits = self.store.search_knowledge(query(&words.join(" ")))?;
        if hits.is_empty() && words.len() > 1 {
            let mut merged: BTreeMap<i64, KnowledgeHit> = BTreeMap::new();
            for word in &words {
                for hit in self.store.search_knowledge(query(word))? {
                    merge_hit(&mut merged, hit);
                }
            }
            hits = merged.into_values().collect();
            output.relaxed = !hits.is_empty();
        }
        let ranked = self.rank_hits(hits, now);
        let picked = self.pick(ranked, max);
        for hit in picked {
            output.results.push(self.tool_hit(&hit.chunk, hit.shown)?);
        }
        Ok(output)
    }

    /// 감쇠를 곱하고, 점수순(같으면 조각 번호순)으로 늘어놓고, 문턱을 적용한다.
    fn rank_hits(&self, hits: Vec<KnowledgeHit>, now: UnixMillis) -> Vec<Ranked> {
        let mut ranked: Vec<Ranked> = hits
            .into_iter()
            .map(|hit| {
                let age = now.0.saturating_sub(hit.chunk.updated_at.0);
                let decay = decay_micros(age, self.config.half_life_days);
                let scaled = |score: i64| scale(score, decay);
                Ranked {
                    shown: hit.score_micros.map(scaled),
                    rank: scaled(hit.score_micros.unwrap_or(SCORE_MAX)),
                    chunk: hit.chunk,
                }
            })
            .collect();
        ranked.sort_by(|a, b| b.rank.cmp(&a.rank).then(a.chunk.id.cmp(&b.chunk.id)));
        let threshold = self.config.min_score_micros;
        let passing = ranked
            .iter()
            .filter(|r| r.shown.is_none_or(|s| s >= threshold))
            .count();
        if passing > 0 {
            ranked.retain(|r| r.shown.is_none_or(|s| s >= threshold));
        }
        ranked
    }

    /// MMR로 `max`개를 고른다.
    fn pick(&self, ranked: Vec<Ranked>, max: usize) -> Vec<Ranked> {
        let candidates: Vec<Candidate> = ranked
            .iter()
            .map(|r| Candidate {
                rank: r.rank,
                tokens: token_set(&r.chunk.text),
            })
            .collect();
        let order = mmr_select(&candidates, max, self.config.mmr_lambda_micros);
        let mut slots: Vec<Option<Ranked>> = ranked.into_iter().map(Some).collect();
        order.into_iter().filter_map(|i| slots[i].take()).collect()
    }

    /// 저장된 조각을 도구가 돌려주는 모양으로.
    pub(crate) fn tool_hit(
        &self,
        chunk: &KnowledgeChunk,
        score_micros: Option<i64>,
    ) -> Result<ToolHit, Error> {
        let (start_line, end_line) = lines_of(&self.store, chunk)?;
        let kind = kind_code(chunk.kind);
        let range = if start_line == end_line {
            format!("L{start_line}")
        } else {
            format!("L{start_line}-L{end_line}")
        };
        let citation = format!("{kind}/{}#{range}", chunk.record_ref);
        let body: String = chunk.text.chars().take(SNIPPET_CHARS).collect();
        Ok(ToolHit {
            chunk_id: chunk.id.0,
            kind: kind.to_string(),
            source: source_code(chunk.source).to_string(),
            record_ref: chunk.record_ref.clone(),
            start_line,
            end_line,
            score_micros,
            snippet: format!("{body}\n\nSource: {citation}"),
            citation,
        })
    }
}

/// 점수에 계수(백만분의 1)를 곱한다.
fn scale(score: i64, factor_micros: i64) -> i64 {
    let product = i128::from(score) * i128::from(factor_micros) / 1_000_000;
    i64::try_from(product).unwrap_or(0)
}

/// 낱말마다 찾은 것을 조각 하나로 합친다. 점수가 있는 것끼리는 더하되 상한을 넘지 않는다.
fn merge_hit(merged: &mut BTreeMap<i64, KnowledgeHit>, hit: KnowledgeHit) {
    match merged.get_mut(&hit.chunk.id.0) {
        None => {
            merged.insert(hit.chunk.id.0, hit);
        }
        Some(have) => {
            have.score_micros = match (have.score_micros, hit.score_micros) {
                (Some(a), Some(b)) => Some(a.saturating_add(b).min(SCORE_MAX)),
                (a, b) => a.or(b),
            };
        }
    }
}
