//! (2차, 결정 31) 오래된 바깥 자료 조각을 요약 하나로 바꾼다. 원문은 부르는 쪽이 보관 파일로 빼낸 뒤에 부른다.
//! 원본 동작: OpenClaw 3.6 — 기억은 "요약, 원문 기록 아님"(`docs/research/openclaw.md`).

use pi_store::{ChunkId, KnowledgeKind, KnowledgeSource, NewKnowledgeRecord, UnixMillis};

use crate::{Error, Knowledge, RecordText, meta};

/// 보관 요약 기록의 키 앞붙이(이 키의 기록은 다시 고르지 않는다).
pub const ARCHIVE_REF_PREFIX: &str = "archive:";

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
    /// 보관 요약 기록(키가 [`ARCHIVE_REF_PREFIX`]로 시작)은 들지 않는다.
    pub fn old_external_records(
        &self,
        before: UnixMillis,
        max: usize,
    ) -> Result<Vec<OldRecord>, Error> {
        let window = max.saturating_mul(4).max(64);
        let chunks = self.store.knowledge_chunks_older(
            KnowledgeKind::MarketNote,
            KnowledgeSource::External,
            before,
            window,
        )?;
        let mut refs: Vec<String> = Vec::new();
        for chunk in chunks {
            if chunk.record_ref.starts_with(ARCHIVE_REF_PREFIX) || refs.contains(&chunk.record_ref)
            {
                continue;
            }
            refs.push(chunk.record_ref);
            if refs.len() >= max {
                break;
            }
        }
        let mut out = Vec::new();
        for record_ref in refs {
            let parts = self
                .store
                .knowledge_chunks(KnowledgeKind::MarketNote, &record_ref)?;
            let at = parts.iter().map(|c| c.updated_at).max().unwrap_or(before);
            let text: Vec<&str> = parts.iter().map(|c| c.text.as_str()).collect();
            out.push(OldRecord {
                record_ref,
                text: text.join("\n"),
                at,
            });
        }
        Ok(out)
    }

    /// `old`의 기록들을 지우고(조각 · 임베딩 · 조각 메타) 요약 기록 하나를 올린다. 요약의 조각 번호를 준다.
    pub fn replace_with_summary(
        &self,
        old: &[String],
        summary: RecordText,
    ) -> Result<Vec<ChunkId>, Error> {
        for record_ref in old {
            self.store.index_record_with_meta(
                NewKnowledgeRecord {
                    kind: KnowledgeKind::MarketNote,
                    source: KnowledgeSource::External,
                    record_ref: record_ref.clone(),
                    chunks: Vec::new(),
                    at: summary.at,
                },
                meta::CHUNK_KEY_PREFIX,
                &[],
            )?;
        }
        self.index(summary)
    }
}
