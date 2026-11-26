//! (21차, 결정 31) 오래된 기록을 보관 파일로 빼내고 DB에서 덜어 내는 좁은 길.
//!
//! 원본 동작: OpenClaw 압축(`docs/research/openclaw.md` 3.4 — 오래된 대화를 요약으로 바꾸고 원문은 따로 둔다).
//!
//! - 덜어 낼 수 있는 것은 [`ArchiveCategory`]의 표 · 종류뿐이다(감사 기록 — 주문 · 체결 · 승인 · 규칙 변경 ·
//!   접속 · 매매 상태 · 복기 · 결정 메모 · 지침 — 은 들지 않는다).
//! - 순서: [`Store::archive_export`]로 줄을 JSON 줄로 받는다 → 부르는 쪽이 파일로 쓰고 확인한다 →
//!   [`Store::archive_commit`]이 같은 묶음 안에서 다시 골라 줄 수 · 지문을 견주고, 보관 묶음 한 줄(요약)을 남긴 뒤
//!   그 줄들을 지운다. 지우기 트리거(판 12)는 보관 묶음이 덮는 줄만 지우게 한다.
//! - 표마다 가장 마지막 줄(이벤트: 가장 큰 번호, 방 로그: 방마다 가장 큰 번호)은 남긴다: 번호가 빠짐없이
//!   이어지게(다음 번호를 `MAX + 1`로 정한다).

use rusqlite::params;

use crate::archive_sql::{BATCH_COLUMNS, batch_from, candidate_filter, commit, export};
use crate::knowledge::{
    CHUNK_COLUMNS, KnowledgeChunk, KnowledgeKind, KnowledgeSource, chunk_from, kind_code,
    source_code,
};
use crate::model::UnixMillis;
use crate::{Store, StoreError};

/// 덜어 낼 수 있는 기록의 묶음.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ArchiveCategory {
    /// 팀 방 대화(`room_events`의 담당 글 · 활동 · 차례 기록).
    Room,
    /// 화면용 이벤트(`events`의 진행 · 연출 · 걷기 · 사무실 말풍선 · 시세 상태).
    Screen,
    /// AI 사용 세부(`events`의 `team.ai_used`).
    AiUsage,
}

/// 내보낸 줄들. 줄마다 JSON 객체 한 개(끝 줄바꿈 없음).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchiveExport {
    /// 묶음.
    pub category: ArchiveCategory,
    /// 기간 시작(포함).
    pub from_at: UnixMillis,
    /// 기간 끝(뺌).
    pub to_at: UnixMillis,
    /// 고른 줄의 가장 큰 rowid. 커밋은 이 이하의 줄만 다시 고른다. 고른 줄이 없으면 0.
    pub upto_rowid: i64,
    /// JSON 줄들(rowid 순서).
    pub lines: Vec<String>,
    /// 줄들의 지문([`fingerprint`]).
    pub fingerprint: String,
}

/// 남길 보관 묶음.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewArchiveBatch {
    /// 묶음.
    pub category: ArchiveCategory,
    /// 기간 이름(예: `2026-W07`).
    pub period: String,
    /// 기간 시작(포함).
    pub from_at: UnixMillis,
    /// 기간 끝(뺌).
    pub to_at: UnixMillis,
    /// 내보낼 때 받은 가장 큰 rowid.
    pub upto_rowid: i64,
    /// 파일에 쓴 줄 수.
    pub row_count: u64,
    /// 내보낼 때 받은 지문.
    pub fingerprint: String,
    /// 보관 파일 이름(폴더 없이).
    pub file: String,
    /// 보관 파일의 SHA-256(소문자 16진수 64글자).
    pub file_sha256: String,
    /// 요약(JSON 글).
    pub summary: String,
    /// 만든 시각. 기간 끝보다 이르면 안 된다.
    pub created_at: UnixMillis,
}

/// 남은 보관 묶음.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchiveBatch {
    /// 번호.
    pub id: i64,
    /// 묶음 값.
    pub batch: NewArchiveBatch,
}

impl ArchiveCategory {
    /// 모든 묶음.
    pub const ALL: [ArchiveCategory; 3] = [
        ArchiveCategory::Room,
        ArchiveCategory::Screen,
        ArchiveCategory::AiUsage,
    ];

    /// 저장 글자(`room` · `screen` · `ai_usage`).
    pub fn code(self) -> &'static str {
        match self {
            ArchiveCategory::Room => "room",
            ArchiveCategory::Screen => "screen",
            ArchiveCategory::AiUsage => "ai_usage",
        }
    }

    /// 담긴 표 이름.
    pub fn table(self) -> &'static str {
        match self {
            ArchiveCategory::Room => "room_events",
            ArchiveCategory::Screen | ArchiveCategory::AiUsage => "events",
        }
    }

    /// 덜어 낼 수 있는 종류들.
    pub fn kinds(self) -> &'static [&'static str] {
        match self {
            ArchiveCategory::Room => &ROOM_KINDS,
            ArchiveCategory::Screen => &SCREEN_KINDS,
            ArchiveCategory::AiUsage => &AI_USAGE_KINDS,
        }
    }

    /// 저장 글자로 묶음을 찾는다.
    pub fn from_code(code: &str) -> Option<ArchiveCategory> {
        ArchiveCategory::ALL.into_iter().find(|c| c.code() == code)
    }

    /// 그 표의 그 종류가 이 묶음에서 덜어 낼 수 있는 것인가.
    pub fn covers(table: &str, kind: &str) -> bool {
        ArchiveCategory::ALL
            .iter()
            .any(|c| c.table() == table && c.kinds().contains(&kind))
    }
}

/// 팀 방 대화: 담당 글 · 활동 · 차례 기록. 사용자 글(`message.user`)과 방 설정(시스템) 줄은 남긴다.
const ROOM_KINDS: [&str; 12] = [
    "message.member",
    "member.unavailable",
    "room.activity",
    "room.assignment",
    "room.continued",
    "room.routed",
    "turn.started",
    "turn.settled",
    "turn.failed",
    "turn.cancelled",
    "turn.deferred",
    "turn.reassigned",
];

/// 화면용 이벤트: 담당 모습 · 걷기(`office.agent`) · 응답 객체 · 말풍선 · 진행 · 연출 · 시세 상태와 묶음.
/// 사용자 글 · 공지 · 승인 · 결정 메모 · 부르기는 남긴다.
const SCREEN_KINDS: [&str; 9] = [
    "office.agent",
    "office.chat",
    "office.talk",
    "office.progress",
    "office.effect",
    "tick",
    "feed_status",
    "quotes",
    "quotes_stale",
];

/// AI 사용 세부.
const AI_USAGE_KINDS: [&str; 1] = ["team.ai_used"];

/// 줄들의 지문: 줄마다 `줄 + "\n"`을 이어 붙인 바이트의 FNV-1a 64(소문자 16진수 16글자).
pub fn fingerprint(lines: &[String]) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for line in lines {
        for byte in line.bytes().chain(std::iter::once(b'\n')) {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    format!("{hash:016x}")
}

impl Store {
    /// 그 기간(`from_at` 이상 `to_at` 미만)의 덜어 낼 줄을 JSON 줄로 준다. 표의 마지막 줄은 빠진다.
    pub fn archive_export(
        &self,
        category: ArchiveCategory,
        from_at: UnixMillis,
        to_at: UnixMillis,
    ) -> Result<ArchiveExport, StoreError> {
        self.read(|conn| export(conn, category, (from_at, to_at), None))
    }

    /// 보관 묶음을 남기고 그 줄들을 지운다. 다시 고른 줄의 수 · 지문이 다르면
    /// [`StoreError::ArchiveMismatch`]이고 아무것도 바꾸지 않는다.
    pub fn archive_commit(&self, batch: NewArchiveBatch) -> Result<ArchiveBatch, StoreError> {
        self.write(|conn| commit(conn, batch))
    }

    /// 남은 보관 묶음 전체(번호 순서).
    pub fn archive_batches(&self) -> Result<Vec<ArchiveBatch>, StoreError> {
        self.read(|conn| {
            let mut stmt = conn.prepare(&format!(
                "SELECT {BATCH_COLUMNS} FROM archive_batches ORDER BY id"
            ))?;
            let rows = stmt.query_map([], batch_from)?;
            let mut out = Vec::new();
            for row in rows {
                out.push(row??);
            }
            Ok(out)
        })
    }

    /// 그 묶음에서 아직 덜어 내지 않은(덜어 낼 수 있는) 가장 이른 시각.
    pub fn archive_oldest_at(
        &self,
        category: ArchiveCategory,
    ) -> Result<Option<UnixMillis>, StoreError> {
        self.read(|conn| {
            let sql = format!(
                "SELECT MIN(at) FROM {} WHERE {}",
                category.table(),
                candidate_filter(category)
            );
            let at: Option<i64> = conn.query_row(&sql, [], |row| row.get(0))?;
            Ok(at.map(UnixMillis))
        })
    }

    /// 그 종류 · 등급의 조각 가운데 `before` 전에 올린 것. 올린 시각 · 번호 순서로 `max`개까지.
    pub fn knowledge_chunks_older(
        &self,
        kind: KnowledgeKind,
        source: KnowledgeSource,
        before: UnixMillis,
        max: usize,
    ) -> Result<Vec<KnowledgeChunk>, StoreError> {
        let limit = i64::try_from(max).unwrap_or(i64::MAX);
        self.read(|conn| {
            let mut stmt = conn.prepare(&format!(
                "SELECT {CHUNK_COLUMNS} FROM knowledge_chunks c
                 WHERE c.kind = ?1 AND c.source = ?2 AND c.updated_at < ?3
                 ORDER BY c.updated_at, c.id LIMIT ?4"
            ))?;
            let mut rows = stmt.query(params![
                kind_code(kind),
                source_code(source),
                before.0,
                limit
            ])?;
            let mut out = Vec::new();
            while let Some(row) = rows.next()? {
                out.push(chunk_from(row)?);
            }
            Ok(out)
        })
    }
}
