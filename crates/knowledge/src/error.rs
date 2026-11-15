//! 오류.

use thiserror::Error;

/// 임베딩 공급자(포트 뒤)가 내는 오류.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum EmbedError {
    /// 지금은 쓸 수 없다(키 없음 · 한도 초과 · 연결 안 됨 따위). 까닭 글.
    #[error("임베딩 공급자를 쓸 수 없다: {0}")]
    Unavailable(String),
    /// 요청은 갔으나 실패했다. 까닭 글.
    #[error("임베딩에 실패했다: {0}")]
    Failed(String),
}

/// 지식 검색 모듈의 오류.
#[derive(Debug, Error)]
pub enum Error {
    /// 저장소 오류.
    #[error("저장소 오류: {0}")]
    Store(#[from] pi_store::StoreError),
    /// 색인에 올릴 수 없는 기록(기록 키가 비었거나 글이 비었다).
    #[error("올릴 수 없는 기록: {0}")]
    InvalidRecord(String),
    /// 임베딩 공급자 오류, 또는 공급자가 약속과 다른 답(개수 · 차원)을 줬다.
    #[error("임베딩 오류: {0}")]
    Embedder(String),
}

impl From<EmbedError> for Error {
    fn from(e: EmbedError) -> Self {
        Error::Embedder(e.to_string())
    }
}
