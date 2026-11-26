//! (21차, 결정 31) 정수 양자화 임베딩(`knowledge_embeddings_q8`). 바이트는 해석하지 않는다:
//! 앞 4바이트는 벡터의 크기 값(리틀 엔디언 f32), 뒤는 차원마다 i8 하나 — knowledge 모듈이 만들고 푼다.
//! 한 조각에는 한 가지 모양만 남는다(새로 두면 다른 모양의 줄은 지운다).

use crate::knowledge::ChunkId;
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
        let _ = (id, model, dims, bytes);
        todo!()
    }
}
