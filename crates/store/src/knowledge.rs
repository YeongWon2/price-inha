//! 지식(11차 1번, D10 · M7). 원본: OpenClaw 기억 색인
//! (`docs/research/triggers-approval-memory-deep.md` 4.1).
//!
//! 기록(매매 기록 · 복기 · 실수 기록 · 시장 메모)을 조각으로 나눠 FTS5 trigram 색인에 둔다.
//! 임베딩은 조각마다 BLOB으로 둔다(뜻 검색 자리만: 임베딩을 만드는 공급자는 이 저장소 밖이다).
//!
//! 표(구현이 이 이름으로 만든다): `knowledge_chunks`(조각) · `knowledge_embeddings` · `knowledge_meta` ·
//! FTS5 가상 표(`tokenize='trigram'`). 조각 표는 조각 번호가 겹치는 `INSERT OR REPLACE`를 거부한다
//! (`knowledge_chunks_no_replace`: 대체가 FTS 색인을 어긋나게 하므로).
//!
//! 칸(테스트가 SQL로 확인한다): `knowledge_chunks(id INTEGER PRIMARY KEY, kind, source, record_ref, ordinal,
//! text, updated_at)`(글자 값은 [`KnowledgeKind`] · [`KnowledgeSource`]의 괄호 안 글자),
//! `knowledge_embeddings(chunk_id, model, dims, embedding BLOB)`(조각을 지우면 함께 지워진다: 외래 키 설정이 꺼진 연결에서도 트리거로),
//! `knowledge_meta(key, value)`. 조각표를 SQL로 고치거나 지워도 FTS 색인이 어긋나지 않는다.
//! 이 표들에는 지우기를 막는 `BEFORE DELETE` 트리거를 두지 않는다(조각은 다시 색인하며 지워진다).

use crate::model::UnixMillis;
use crate::{Store, StoreError};

/// 조각 번호. 저장소가 1부터 붙이고 다시 쓰지 않는다.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ChunkId(pub i64);

/// 기록의 종류(M7 "기록").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KnowledgeKind {
    /// 매매 기록 (`'trade'`)
    Trade,
    /// 복기 (`'review'`)
    Review,
    /// 실수 기록 (`'mistake'`)
    Mistake,
    /// 시장 메모 (`'market_note'`)
    MarketNote,
}

impl KnowledgeKind {
    /// 모든 종류.
    pub const ALL: [KnowledgeKind; 4] = [
        KnowledgeKind::Trade,
        KnowledgeKind::Review,
        KnowledgeKind::Mistake,
        KnowledgeKind::MarketNote,
    ];
}

/// 출처 등급(M7). 바깥 자료는 자동 회상에 넣지 않는다.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KnowledgeSource {
    /// 내가 쓴 것 (`'mine'`)
    Mine,
    /// 팀이 쓴 것 (`'team'`)
    Team,
    /// 바깥 자료 (`'external'`)
    External,
}

impl KnowledgeSource {
    /// 모든 등급.
    pub const ALL: [KnowledgeSource; 3] = [
        KnowledgeSource::Mine,
        KnowledgeSource::Team,
        KnowledgeSource::External,
    ];
}

/// 한 기록을 색인에 올리라는 요청. `(kind, record_ref)`가 기록의 정체다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewKnowledgeRecord {
    /// 기록 종류.
    pub kind: KnowledgeKind,
    /// 출처 등급.
    pub source: KnowledgeSource,
    /// 기록을 가리키는 글자(예: 매매 번호). 비면 안 된다.
    pub record_ref: String,
    /// 조각들의 글. 순서가 조각 순서(0부터)다. 조각은 비어 있으면 안 된다.
    pub chunks: Vec<String>,
    /// 올린 시각.
    pub at: UnixMillis,
}

/// 저장된 조각 한 건.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KnowledgeChunk {
    /// 조각 번호.
    pub id: ChunkId,
    /// 기록 종류.
    pub kind: KnowledgeKind,
    /// 출처 등급.
    pub source: KnowledgeSource,
    /// 기록을 가리키는 글자.
    pub record_ref: String,
    /// 기록 안의 조각 순서(0부터).
    pub ordinal: u32,
    /// 조각의 글.
    pub text: String,
    /// 올린 시각.
    pub updated_at: UnixMillis,
}

/// 키워드 검색 요청.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KnowledgeQuery {
    /// 질의. 공백으로 나눈 낱말이 **모두**(AND) 들어 있는 조각을 찾는다. 앞뒤 공백만이면 결과가 없다.
    pub text: String,
    /// 이 종류만. 비면 모든 종류.
    pub kinds: Vec<KnowledgeKind>,
    /// 이 등급만. 비면 모든 등급.
    pub sources: Vec<KnowledgeSource>,
    /// 참이면 자동 회상용이다: 바깥 자료([`KnowledgeSource::External`])는 `sources`에 있어도 나오지 않는다.
    pub auto_recall: bool,
    /// 가장 많이 줄 개수. 0이면 빈 목록.
    pub max: usize,
}

/// 검색 적중 하나.
#[derive(Debug, Clone, PartialEq)]
pub struct KnowledgeHit {
    /// 적중한 조각.
    pub chunk: KnowledgeChunk,
    /// 0 초과 1 이하의 점수. 클수록 잘 맞는다(`relevance = -rank`, `점수 = relevance / (1 + relevance)`).
    /// 모든 낱말이 3글자 미만이어서 부분 문자열(LIKE)로만 찾은 적중은 순위가 없어 `None`이다.
    pub score: Option<f64>,
}

/// 조각의 임베딩.
#[derive(Debug, Clone, PartialEq)]
pub struct ChunkEmbedding {
    /// 만든 모델 이름.
    pub model: String,
    /// 벡터. BLOB에는 `f32` 리틀 엔디언으로 둔다(바이트 수 = 차원 × 4).
    pub vector: Vec<f32>,
}

impl Store {
    /// 기록을 색인에 올린다. 같은 `(kind, record_ref)`가 이미 있으면 그 조각들을 모두 지우고
    /// (임베딩과 함께: 기록이 바뀌면 다시 만든다, M7) 새 조각으로 바꾼다. 새 조각은 새 번호를 받고 임베딩이 없다.
    ///
    /// - `chunks`가 비면 그 기록의 조각을 모두 지우고 빈 목록을 돌려준다.
    /// - `record_ref`가 비었거나 조각 글이 빈 글자이면 [`StoreError::InvalidKnowledge`]이고 아무것도 바꾸지 않는다.
    /// - 돌려주는 번호는 조각 순서대로다.
    pub fn index_record(&self, record: NewKnowledgeRecord) -> Result<Vec<ChunkId>, StoreError> {
        let _ = record;
        todo!("11차 구현")
    }

    /// 한 기록의 조각들. 조각 순서다. 없으면 빈 목록.
    pub fn knowledge_chunks(
        &self,
        kind: KnowledgeKind,
        record_ref: &str,
    ) -> Result<Vec<KnowledgeChunk>, StoreError> {
        let _ = (kind, record_ref);
        todo!("11차 구현")
    }

    /// 키워드 검색. 3글자(글자 단위) 이상 낱말은 trigram `MATCH`로, 3글자 미만 낱말(한글 두 글자
    /// "매수" · "손절")은 부분 문자열로 찾는다. 모든 낱말이 맞아야 한다. 영문은 대소문자를 가리지 않는다.
    /// 순서는 점수가 높은 것부터이고, 점수가 없는 것은 그 뒤에 조각 번호 순이다.
    pub fn search_knowledge(&self, query: KnowledgeQuery) -> Result<Vec<KnowledgeHit>, StoreError> {
        let _ = query;
        todo!("11차 구현")
    }

    /// 조각의 임베딩을 두거나 바꾼다. 벡터가 비었거나 유한하지 않은 값(NaN · 무한)이 있으면
    /// [`StoreError::InvalidEmbedding`]. 조각이 없으면 [`StoreError::ChunkNotFound`].
    pub fn set_chunk_embedding(
        &self,
        id: ChunkId,
        model: &str,
        vector: &[f32],
    ) -> Result<(), StoreError> {
        let _ = (id, model, vector);
        todo!("11차 구현")
    }

    /// 조각의 임베딩. 없으면 `None`이다. 둔 그대로(비트까지) 나온다.
    pub fn chunk_embedding(&self, id: ChunkId) -> Result<Option<ChunkEmbedding>, StoreError> {
        let _ = id;
        todo!("11차 구현")
    }

    /// 그 모델의 임베딩이 없는 조각의 번호를 번호 순서로 `max`개까지(다시 만들 것의 목록).
    pub fn chunks_missing_embedding(
        &self,
        model: &str,
        max: usize,
    ) -> Result<Vec<ChunkId>, StoreError> {
        let _ = (model, max);
        todo!("11차 구현")
    }

    /// 색인의 메타(공급자 · 모델 · 조각 설정 따위)를 두거나 바꾼다. 키가 비면 [`StoreError::InvalidKnowledge`].
    pub fn set_knowledge_meta(&self, key: &str, value: &str) -> Result<(), StoreError> {
        let _ = (key, value);
        todo!("11차 구현")
    }

    /// 색인의 메타 한 값. 없으면 `None`.
    pub fn knowledge_meta(&self, key: &str) -> Result<Option<String>, StoreError> {
        let _ = key;
        todo!("11차 구현")
    }
}
