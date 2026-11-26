//! (6차) 뜻 검색 몸통 — 뼈대(red).

use pi_store::UnixMillis;

use crate::{Error, Knowledge, SearchMode, SearchOutput, SearchRequest, SemanticStatus};

/// 합에서 벡터 점수의 몫.
pub const VECTOR_WEIGHT_MICROS: i64 = 700_000;
/// 합에서 다시 잰 키워드 점수의 몫.
pub const TEXT_WEIGHT_MICROS: i64 = 300_000;

impl Knowledge {
    /// 키워드 + 벡터 하이브리드 검색(뼈대).
    pub async fn hybrid_search(
        &self,
        request: &SearchRequest,
        now: UnixMillis,
    ) -> Result<SearchOutput, Error> {
        let _ = (request, now);
        let semantic = match &self.embedder {
            None => self.semantic_status(),
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
}
