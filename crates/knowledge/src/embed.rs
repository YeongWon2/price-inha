//! 임베딩 포트. 실제 공급자는 이번 범위가 아니다(결정 2026-11-16 11번).

use std::sync::Arc;

use async_trait::async_trait;
use serde::Serialize;

use crate::EmbedError;

/// 글을 벡터로 바꾸는 포트. 어댑터(pi-ai 쪽)가 구현하고 조립하는 곳이 넣는다.
#[async_trait]
pub trait Embedder: Send + Sync {
    /// 모델 이름(저장소에 조각별로 남는다).
    fn model(&self) -> &str;
    /// 벡터의 차원 수.
    fn dims(&self) -> u32;
    /// 글마다 벡터 하나(같은 순서 · 같은 개수 · 길이 `dims`).
    async fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, EmbedError>;
}

/// 뜻 검색을 쓸 수 있는지(검색 도구의 결과에 늘 실린다).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum SemanticStatus {
    /// 쓸 수 있다.
    Available,
    /// 쓸 수 없다. 오류가 아니라 결과다: 까닭 글 · 경고 · 무엇을 하면 되는지 · 코드.
    Unavailable {
        /// 까닭.
        warning: String,
        /// 무엇을 하면 되는지.
        action: String,
        /// 기계가 읽는 코드(임베딩이 없으면 `no_embedder`).
        code: String,
    },
}

/// 임베딩을 만든 결과.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EmbedReport {
    /// 새로 임베딩을 둔 조각 수.
    pub embedded: usize,
    /// 임베딩 공급자가 없어서 아무것도 하지 않았으면 참.
    pub unavailable: bool,
}

/// 공급자 손잡이(공유).
pub type SharedEmbedder = Arc<dyn Embedder>;

impl crate::Knowledge {
    /// 임베딩이 없는 조각에 임베딩을 둔다. 포트를 한 번만 부르고, 받은 것을 모두 확인한 뒤에 둔다.
    pub(crate) async fn run_embed_missing(&self, max: usize) -> Result<EmbedReport, crate::Error> {
        let Some(embedder) = &self.embedder else {
            return Ok(EmbedReport {
                embedded: 0,
                unavailable: true,
            });
        };
        let ids = self.store.chunks_missing_embedding(embedder.model(), max)?;
        let mut targets: Vec<(pi_store::ChunkId, String)> = Vec::new();
        for id in ids {
            // 이 모듈을 거치지 않고 올라온 조각은 어느 기록인지 몰라 건너뛴다.
            let Some(origin) = crate::meta::origin_of(&self.store, id)? else {
                continue;
            };
            let chunks = self
                .store
                .knowledge_chunks(origin.kind, &origin.record_ref)?;
            if let Some(chunk) = chunks.into_iter().find(|c| c.id == id) {
                targets.push((id, chunk.text));
            }
        }
        if targets.is_empty() {
            return Ok(EmbedReport {
                embedded: 0,
                unavailable: false,
            });
        }
        let texts: Vec<String> = targets.iter().map(|(_, text)| text.clone()).collect();
        let vectors = embedder.embed(&texts).await?;
        let dims = embedder.dims();
        let width = usize::try_from(dims).unwrap_or(usize::MAX);
        if vectors.len() != targets.len() || vectors.iter().any(|v| v.len() != width) {
            return Err(crate::Error::Embedder(
                "임베딩 공급자가 약속한 개수나 차원과 다르게 답했다".to_string(),
            ));
        }
        for ((id, _), vector) in targets.iter().zip(&vectors) {
            let bytes: Vec<u8> = vector.iter().flat_map(|v| v.to_le_bytes()).collect();
            self.store
                .set_chunk_embedding(*id, embedder.model(), dims, &bytes)?;
        }
        Ok(EmbedReport {
            embedded: targets.len(),
            unavailable: false,
        })
    }
}
