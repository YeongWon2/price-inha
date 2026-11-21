//! 다시 색인하는 조건(메타 비교: `docs/research/triggers-approval-memory-deep.md` 4.1).

/// 색인의 정체. 저장소 메타에 남기고 다음에 현재 설정과 견준다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexSettings {
    /// 임베딩 공급자 이름(없으면 `"none"`).
    pub provider: String,
    /// 모델 이름. 키워드 전용이면 빈 글자(`""`).
    pub model: String,
    /// 벡터 차원. 키워드 전용이면 `None`.
    pub dims: Option<u32>,
    /// 조각 크기.
    pub chunk_tokens: usize,
    /// 조각 겹침.
    pub chunk_overlap: usize,
    /// 글 색인 방식(`"trigram"`).
    pub tokenizer: String,
    /// 어떤 기록을 색인하나를 가리키는 해시 글자.
    pub scope_hash: String,
}

/// 다시 색인해야 하는 까닭.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReindexReason {
    /// 색인에 남은 설정이 없다(새 색인).
    FirstTime,
    /// 공급자가 바뀜.
    ProviderChanged,
    /// 모델이 바뀜.
    ModelChanged,
    /// 벡터 차원이 바뀜.
    DimsChanged,
    /// 조각 크기나 겹침이 바뀜.
    ChunkSettingsChanged,
    /// 색인 방식이 바뀜.
    TokenizerChanged,
    /// 색인 범위가 바뀜.
    ScopeChanged,
    /// 사람이 강제했다.
    Forced,
}

/// 다시 색인할지에 대한 판단.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReindexPlan {
    /// 그대로 둔다.
    Keep,
    /// 다시 만든다.
    Rebuild(ReindexReason),
    /// 공급자가 죽어 있어 벡터가 있는 색인을 키워드 전용으로 덮어쓰지 않는다. 그대로 둔다.
    SkipProviderUnavailable,
}

/// 임베딩 다시 만들기 한 걸음의 결과([`crate::Knowledge::reembed_step`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReembedProgress {
    /// 임베딩 포트가 없다. 아무것도 바꾸지 않았고 바깥 요청도 없다(V23).
    Unavailable,
    /// 이번에 임베딩을 둔 조각이 있다. 아직 끝나지 않았다(남은 설정은 그대로다).
    InProgress {
        /// 이번에 둔 조각 수.
        embedded: usize,
    },
    /// 빠진 조각이 없다. 이때 처음으로 현재 설정을 남겼다.
    Done {
        /// 이번에 둔 조각 수.
        embedded: usize,
        /// 그때의 `plan_reindex(current, true, false)` 결과.
        plan: ReindexPlan,
    },
}
