//! (22차) 모델별 조각 벡터 훑기 — 뼈대(red).

use crate::knowledge::{ChunkEmbedding, ChunkId, KnowledgeChunk};
use crate::{Store, StoreError};

/// 임베딩이 있는 조각 한 건.
#[derive(Debug, Clone, PartialEq)]
pub struct ChunkVector {
    /// 조각.
    pub chunk: KnowledgeChunk,
    /// 그 조각의 임베딩(그 모델의 것).
    pub embedding: ChunkEmbedding,
}

impl Store {
    /// 그 모델의 임베딩이 있는 조각을 번호 순서로 `max`개까지(뼈대).
    pub fn chunk_vectors(
        &self,
        model: &str,
        after: Option<ChunkId>,
        max: usize,
    ) -> Result<Vec<ChunkVector>, StoreError> {
        let _ = (self, model, after, max);
        Ok(Vec::new())
    }
}
