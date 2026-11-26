//! (22차) 모델별 조각 벡터 훑기: 뜻 검색(knowledge 하이브리드)이 "그 모델의 임베딩이 있는 조각"을 번호 순서로
//! 쪽마다 읽는다. 조각(번호 · 종류 · 등급 · 기록 키 · 시각 · 글)과 그 임베딩(옛 f32 줄 또는 정수 양자화 줄)을 함께 준다.
//! 판은 그대로다(새 표 없음, 읽기만).

use rusqlite::params;

use crate::knowledge::{
    CHUNK_COLUMNS, ChunkEmbedding, ChunkId, KnowledgeChunk, chunk_from, decode,
};
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
    /// 그 모델의 임베딩이 있는 조각을 번호 순서로 `max`개까지 준다. `after`가 있으면 그 번호보다 큰 것만
    /// (쪽 넘기기). 다른 모델의 임베딩만 있는 조각은 나오지 않는다. `max`가 0이면 빈 목록.
    pub fn chunk_vectors(
        &self,
        model: &str,
        after: Option<ChunkId>,
        max: usize,
    ) -> Result<Vec<ChunkVector>, StoreError> {
        if max == 0 {
            return Ok(Vec::new());
        }
        let after = after.map_or(0, |id| id.0);
        let limit = i64::try_from(max).unwrap_or(i64::MAX);
        self.read(|conn| {
            let mut stmt = conn.prepare(&format!(
                "SELECT {CHUNK_COLUMNS}, e.model, e.dims, e.embedding, e.quantized
                 FROM knowledge_chunks c
                 JOIN (SELECT chunk_id, model, dims, embedding, 0 AS quantized
                         FROM knowledge_embeddings WHERE model = ?1
                       UNION ALL
                       SELECT chunk_id, model, dims, embedding, 1 AS quantized
                         FROM knowledge_embeddings_q8 WHERE model = ?1) e
                   ON e.chunk_id = c.id
                 WHERE c.id > ?2
                 ORDER BY c.id LIMIT ?3"
            ))?;
            let mut rows = stmt.query(params![model, after, limit])?;
            let mut found: Vec<ChunkVector> = Vec::new();
            while let Some(row) = rows.next()? {
                let chunk = chunk_from(row)?;
                // 한 조각에 두 모양이 함께 남지 않지만, 남았다면 먼저 나온 하나만 쓴다.
                if found.last().is_some_and(|last| last.chunk.id == chunk.id) {
                    continue;
                }
                let mut embedding = decode(row.get(7)?, row.get(8)?, row.get(9)?)?;
                embedding.quantized = row.get::<_, i64>(10)? == 1;
                found.push(ChunkVector { chunk, embedding });
            }
            Ok(found)
        })
    }
}
