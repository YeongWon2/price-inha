//! (2차, 결정 31) 오래된 바깥 자료 조각을 요약 하나로 바꾼다. 원문은 부르는 쪽이 보관 파일로 빼낸 뒤에 부른다.
//! 원본 동작: OpenClaw 3.6 — 기억은 "요약, 원문 기록 아님"(`docs/research/openclaw.md`).

use pi_store::{ChunkId, UnixMillis};

use crate::{Error, Knowledge, RecordText};

/// 오래된 기록 하나(조각 글을 순서대로 이어 붙인 것).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OldRecord {
    /// 기록 키.
    pub record_ref: String,
    /// 조각 글(순서대로 줄바꿈으로 이은 것).
    pub text: String,
    /// 올린 시각(조각 가운데 가장 늦은 것).
    pub at: UnixMillis,
}

impl Knowledge {
    /// 바깥 자료(`market_note` · `external`) 가운데 `before` 전에 올린 기록들. 오래된 것부터 `max`개.
    /// 보관 요약 기록(키가 `archive:`로 시작)은 들지 않는다.
    pub fn old_external_records(
        &self,
        before: UnixMillis,
        max: usize,
    ) -> Result<Vec<OldRecord>, Error> {
        let _ = (before, max);
        todo!()
    }

    /// `old`의 기록들을 지우고(조각 · 임베딩 · 조각 메타) 요약 기록 하나를 올린다. 요약의 조각 번호를 준다.
    pub fn replace_with_summary(
        &self,
        old: &[String],
        summary: RecordText,
    ) -> Result<Vec<ChunkId>, Error> {
        let _ = (old, summary);
        todo!()
    }
}
