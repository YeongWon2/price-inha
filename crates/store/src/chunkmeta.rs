//! 지식 조각 번호 조회 · 메타 키 지우기 · 조각과 조각 메타를 한 묶음으로 올리기 (후보 일괄 처리).
//!
//! knowledge 모듈은 조각마다 `chunk.<번호>` 같은 메타를 둔다. 조각 저장과 메타 저장이 따로 되면
//! 그 사이에 꺼졌을 때 메타 없는 조각이 남는다. 여기의 [`Store::index_record_with_meta`]는 둘을 한 묶음에서 한다.

use rusqlite::{Connection, params};

use crate::knowledge::{
    CHUNK_COLUMNS, ChunkId, KnowledgeChunk, NewKnowledgeRecord, check_record, chunk_from, index,
    kind_code,
};
use crate::{Store, StoreError};

impl Store {
    /// 번호로 조각 한 건. 없으면 `None`.
    pub fn knowledge_chunk(&self, id: ChunkId) -> Result<Option<KnowledgeChunk>, StoreError> {
        self.read(|conn| {
            let mut stmt = conn.prepare(&format!(
                "SELECT {CHUNK_COLUMNS} FROM knowledge_chunks c WHERE c.id = ?1"
            ))?;
            let mut rows = stmt.query([id.0])?;
            rows.next()?.map(chunk_from).transpose()
        })
    }

    /// 메타 키를 지운다. 있어서 지웠으면 참, 없었으면 거짓.
    pub fn remove_knowledge_meta(&self, key: &str) -> Result<bool, StoreError> {
        self.write(|conn| {
            let removed = conn.execute("DELETE FROM knowledge_meta WHERE key = ?1", [key])?;
            Ok(removed > 0)
        })
    }

    /// [`Store::index_record`]와 같고, 같은 묶음 안에서 조각마다 메타를 둔다.
    ///
    /// - 새 조각 `i`의 메타 키는 `key_prefix` 뒤에 그 조각 번호를 붙인 것이고 값은 `values[i]`다.
    /// - 바뀌어 사라진 옛 조각의 같은 꼴 키(`key_prefix` + 옛 번호)는 지운다.
    /// - `key_prefix`가 비었거나 `values`의 개수가 조각 수와 다르면 [`StoreError::InvalidKnowledge`]이고
    ///   아무것도 바꾸지 않는다. 기록 모양 규칙은 [`Store::index_record`]와 같다.
    pub fn index_record_with_meta(
        &self,
        record: NewKnowledgeRecord,
        key_prefix: &str,
        values: &[String],
    ) -> Result<Vec<ChunkId>, StoreError> {
        check_record(&record)?;
        if key_prefix.is_empty() {
            return Err(StoreError::InvalidKnowledge(
                "메타 키 앞붙이가 비었다".to_string(),
            ));
        }
        if values.len() != record.chunks.len() {
            return Err(StoreError::InvalidKnowledge(format!(
                "메타 값 {}개와 조각 {}개가 다르다",
                values.len(),
                record.chunks.len()
            )));
        }
        self.write(|conn| {
            for old in old_ids(conn, &record)? {
                conn.execute(
                    "DELETE FROM knowledge_meta WHERE key = ?1",
                    [format!("{key_prefix}{}", old.0)],
                )?;
            }
            let ids = index(conn, &record)?;
            for (id, value) in ids.iter().zip(values) {
                conn.execute(
                    "INSERT INTO knowledge_meta (key, value) VALUES (?1, ?2)
                     ON CONFLICT (key) DO UPDATE SET value = excluded.value",
                    params![format!("{key_prefix}{}", id.0), value],
                )?;
            }
            Ok(ids)
        })
    }
}

/// 그 기록의 지금 조각 번호들.
fn old_ids(conn: &Connection, record: &NewKnowledgeRecord) -> Result<Vec<ChunkId>, StoreError> {
    let mut stmt =
        conn.prepare("SELECT id FROM knowledge_chunks WHERE kind = ?1 AND record_ref = ?2")?;
    let ids = stmt
        .query_map(params![kind_code(record.kind), record.record_ref], |row| {
            row.get(0).map(ChunkId)
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(ids)
}
