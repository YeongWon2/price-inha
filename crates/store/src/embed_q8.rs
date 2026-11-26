//! (21차, 결정 31) 정수 양자화 임베딩(`knowledge_embeddings_q8`). 바이트는 해석하지 않는다:
//! 앞 4바이트는 벡터의 크기 값(리틀 엔디언 f32), 뒤는 차원마다 i8 하나 — knowledge 모듈이 만들고 푼다.
//! 한 조각에는 한 가지 모양만 남는다(새로 두면 다른 모양의 줄은 지운다).

use rusqlite::{Connection, OptionalExtension, params};

use crate::knowledge::{ChunkEmbedding, ChunkId, decode};
use crate::{Store, StoreError};

impl Store {
    /// 조각의 정수 양자화 임베딩을 두거나 바꾼다. `dims`가 0이거나 바이트 길이가 `dims + 4`가 아니면
    /// [`StoreError::InvalidEmbedding`]. 조각이 없으면 [`StoreError::ChunkNotFound`]. 그 조각의 f32 줄은 지운다.
    pub fn set_chunk_embedding_q8(
        &self,
        id: ChunkId,
        model: &str,
        dims: u32,
        bytes: &[u8],
    ) -> Result<(), StoreError> {
        let expected = usize::try_from(dims)
            .ok()
            .and_then(|d| d.checked_add(4))
            .filter(|len| dims > 0 && *len == bytes.len());
        if expected.is_none() {
            return Err(StoreError::InvalidEmbedding);
        }
        self.write(|conn| {
            let known: Option<i64> = conn
                .query_row(
                    "SELECT id FROM knowledge_chunks WHERE id = ?1",
                    [id.0],
                    |r| r.get(0),
                )
                .optional()?;
            if known.is_none() {
                return Err(StoreError::ChunkNotFound(id));
            }
            conn.execute(
                "DELETE FROM knowledge_embeddings WHERE chunk_id = ?1",
                [id.0],
            )?;
            conn.execute(
                "INSERT INTO knowledge_embeddings_q8 (chunk_id, model, dims, embedding)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT (chunk_id) DO UPDATE
                 SET model = excluded.model, dims = excluded.dims, embedding = excluded.embedding",
                params![id.0, model, i64::from(dims), bytes],
            )?;
            Ok(())
        })
    }
}

/// 조각의 정수 양자화 임베딩(없으면 `None`).
pub(crate) fn read_q8(
    conn: &Connection,
    id: ChunkId,
) -> Result<Option<ChunkEmbedding>, StoreError> {
    let found: Option<(String, i64, Vec<u8>)> = conn
        .query_row(
            "SELECT model, dims, embedding FROM knowledge_embeddings_q8 WHERE chunk_id = ?1",
            [id.0],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?;
    let Some((model, dims, bytes)) = found else {
        return Ok(None);
    };
    let mut embedding = decode(model, dims, bytes)?;
    embedding.quantized = true;
    Ok(Some(embedding))
}
