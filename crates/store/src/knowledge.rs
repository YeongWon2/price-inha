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

use rusqlite::types::Value;
use rusqlite::{Connection, OptionalExtension, Row, params, params_from_iter};

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
    /// 0 초과 1_000_000 이하의 점수(백만분의 1 단위 정수). 클수록 잘 맞는다
    /// (`relevance = -rank`, `점수 = relevance / (1 + relevance)`를 정수로 셈한다). store 안에서는 실수를 쓰지 않는다.
    /// 모든 낱말이 3글자 미만이어서 부분 문자열(LIKE)로만 찾은 적중은 순위가 없어 `None`이다.
    pub score_micros: Option<i64>,
}

/// 조각의 임베딩.
#[derive(Debug, Clone, PartialEq)]
pub struct ChunkEmbedding {
    /// 만든 모델 이름.
    pub model: String,
    /// 차원 수.
    pub dims: u32,
    /// 벡터의 바이트. 받은 그대로 BLOB에 둔다(리틀 엔디언 부동소수 묶음은 knowledge 모듈이 만든다). 길이 = 차원 × 4.
    /// (21차) `quantized`가 참이면 정수 양자화 바이트(크기 값 4바이트 + 차원마다 1바이트, 길이 = 차원 + 4).
    pub bytes: Vec<u8>,
    /// (21차, 결정 31) 정수 양자화 바이트인가([`Store::set_chunk_embedding_q8`]로 둔 것).
    pub quantized: bool,
}

impl Store {
    /// 기록을 색인에 올린다. 같은 `(kind, record_ref)`가 이미 있으면 그 조각들을 모두 지우고
    /// (임베딩과 함께: 기록이 바뀌면 다시 만든다, M7) 새 조각으로 바꾼다. 새 조각은 새 번호를 받고 임베딩이 없다.
    ///
    /// - `chunks`가 비면 그 기록의 조각을 모두 지우고 빈 목록을 돌려준다.
    /// - `record_ref`가 비었거나 조각 글이 빈 글자이면 [`StoreError::InvalidKnowledge`]이고 아무것도 바꾸지 않는다.
    /// - 돌려주는 번호는 조각 순서대로다.
    pub fn index_record(&self, record: NewKnowledgeRecord) -> Result<Vec<ChunkId>, StoreError> {
        check_record(&record)?;
        self.write(|conn| index(conn, &record))
    }

    /// 한 기록의 조각들. 조각 순서다. 없으면 빈 목록.
    pub fn knowledge_chunks(
        &self,
        kind: KnowledgeKind,
        record_ref: &str,
    ) -> Result<Vec<KnowledgeChunk>, StoreError> {
        self.read(|conn| {
            let mut stmt = conn.prepare(&format!(
                "SELECT {CHUNK_COLUMNS} FROM knowledge_chunks c
                 WHERE c.kind = ?1 AND c.record_ref = ?2 ORDER BY c.ordinal"
            ))?;
            let mut rows = stmt.query(params![kind_code(kind), record_ref])?;
            let mut out = Vec::new();
            while let Some(row) = rows.next()? {
                out.push(chunk_from(row)?);
            }
            Ok(out)
        })
    }

    /// 키워드 검색. 3글자(글자 단위) 이상 낱말은 trigram `MATCH`로, 3글자 미만 낱말(한글 두 글자
    /// "매수" · "손절")은 부분 문자열로 찾는다. 모든 낱말이 맞아야 한다. 영문은 대소문자를 가리지 않는다.
    /// 순서는 점수가 높은 것부터이고, 점수가 없는 것은 그 뒤에 조각 번호 순이다.
    pub fn search_knowledge(&self, query: KnowledgeQuery) -> Result<Vec<KnowledgeHit>, StoreError> {
        let words: Vec<&str> = query.text.split_whitespace().collect();
        if words.is_empty() || query.max == 0 {
            return Ok(Vec::new());
        }
        self.read(|conn| search(conn, &words, &query))
    }

    /// 조각의 임베딩을 두거나 바꾼다. 바이트는 해석하지 않고 그대로 둔다. `dims`가 0이거나
    /// 바이트 길이가 `dims × 4`가 아니면 [`StoreError::InvalidEmbedding`]. 조각이 없으면 [`StoreError::ChunkNotFound`].
    pub fn set_chunk_embedding(
        &self,
        id: ChunkId,
        model: &str,
        dims: u32,
        bytes: &[u8],
    ) -> Result<(), StoreError> {
        let expected = usize::try_from(dims)
            .ok()
            .and_then(|d| d.checked_mul(4))
            .filter(|len| dims > 0 && *len == bytes.len());
        if expected.is_none() {
            return Err(StoreError::InvalidEmbedding);
        }
        let dims = i64::from(dims);
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
                "INSERT INTO knowledge_embeddings (chunk_id, model, dims, embedding)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT (chunk_id) DO UPDATE
                 SET model = excluded.model, dims = excluded.dims, embedding = excluded.embedding",
                params![id.0, model, dims, bytes],
            )?;
            Ok(())
        })
    }

    /// 조각의 임베딩. 없으면 `None`이다. 둔 그대로(비트까지) 나온다.
    pub fn chunk_embedding(&self, id: ChunkId) -> Result<Option<ChunkEmbedding>, StoreError> {
        self.read(|conn| {
            let found: Option<(String, i64, Vec<u8>)> = conn
                .query_row(
                    "SELECT model, dims, embedding FROM knowledge_embeddings WHERE chunk_id = ?1",
                    [id.0],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
                .optional()?;
            found
                .map(|(model, dims, bytes)| decode(model, dims, bytes))
                .transpose()
        })
    }

    /// 그 모델의 임베딩이 없는 조각의 번호를 번호 순서로 `max`개까지(다시 만들 것의 목록).
    pub fn chunks_missing_embedding(
        &self,
        model: &str,
        max: usize,
    ) -> Result<Vec<ChunkId>, StoreError> {
        self.read(|conn| {
            let mut stmt = conn.prepare(
                "SELECT c.id FROM knowledge_chunks c
                 WHERE NOT EXISTS (SELECT 1 FROM knowledge_embeddings e
                                   WHERE e.chunk_id = c.id AND e.model = ?1)
                 ORDER BY c.id LIMIT ?2",
            )?;
            let ids = stmt
                .query_map(params![model, limit_of(max)], |row| row.get(0).map(ChunkId))?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(ids)
        })
    }

    /// 색인의 메타(공급자 · 모델 · 조각 설정 따위)를 두거나 바꾼다. 키가 비면 [`StoreError::InvalidKnowledge`].
    pub fn set_knowledge_meta(&self, key: &str, value: &str) -> Result<(), StoreError> {
        if key.is_empty() {
            return Err(StoreError::InvalidKnowledge("메타 키가 비었다".to_string()));
        }
        self.write(|conn| {
            conn.execute(
                "INSERT INTO knowledge_meta (key, value) VALUES (?1, ?2)
                 ON CONFLICT (key) DO UPDATE SET value = excluded.value",
                params![key, value],
            )?;
            Ok(())
        })
    }

    /// 색인의 메타 한 값. 없으면 `None`.
    pub fn knowledge_meta(&self, key: &str) -> Result<Option<String>, StoreError> {
        self.read(|conn| {
            Ok(conn
                .query_row(
                    "SELECT value FROM knowledge_meta WHERE key = ?1",
                    [key],
                    |r| r.get(0),
                )
                .optional()?)
        })
    }
}

// ───────────── 안에서만 쓰는 것 ─────────────

pub(crate) const CHUNK_COLUMNS: &str =
    "c.id, c.kind, c.source, c.record_ref, c.ordinal, c.text, c.updated_at";

pub(crate) fn kind_code(kind: KnowledgeKind) -> &'static str {
    match kind {
        KnowledgeKind::Trade => "trade",
        KnowledgeKind::Review => "review",
        KnowledgeKind::Mistake => "mistake",
        KnowledgeKind::MarketNote => "market_note",
    }
}

fn source_code(source: KnowledgeSource) -> &'static str {
    match source {
        KnowledgeSource::Mine => "mine",
        KnowledgeSource::Team => "team",
        KnowledgeSource::External => "external",
    }
}

fn kind_from(code: &str) -> Result<KnowledgeKind, StoreError> {
    KnowledgeKind::ALL
        .into_iter()
        .find(|kind| kind_code(*kind) == code)
        .ok_or_else(|| StoreError::Incompatible(format!("모르는 지식 종류 값: {code}")))
}

fn source_from(code: &str) -> Result<KnowledgeSource, StoreError> {
    KnowledgeSource::ALL
        .into_iter()
        .find(|source| source_code(*source) == code)
        .ok_or_else(|| StoreError::Incompatible(format!("모르는 지식 등급 값: {code}")))
}

fn limit_of(max: usize) -> i64 {
    i64::try_from(max).unwrap_or(i64::MAX)
}

/// `CHUNK_COLUMNS` 순서의 줄을 조각으로 읽는다.
pub(crate) fn chunk_from(row: &Row<'_>) -> Result<KnowledgeChunk, StoreError> {
    let ordinal: i64 = row.get(4)?;
    Ok(KnowledgeChunk {
        id: ChunkId(row.get(0)?),
        kind: kind_from(&row.get::<_, String>(1)?)?,
        source: source_from(&row.get::<_, String>(2)?)?,
        record_ref: row.get(3)?,
        ordinal: u32::try_from(ordinal)
            .map_err(|_| StoreError::Incompatible(format!("조각 순서 값이 이상하다: {ordinal}")))?,
        text: row.get(5)?,
        updated_at: UnixMillis(row.get(6)?),
    })
}

/// 색인에 올릴 기록의 모양을 본다: 기록 키와 조각 글이 비면 안 된다.
pub(crate) fn check_record(record: &NewKnowledgeRecord) -> Result<(), StoreError> {
    if record.record_ref.is_empty() {
        return Err(StoreError::InvalidKnowledge("기록 키가 비었다".to_string()));
    }
    if let Some(index) = record.chunks.iter().position(|text| text.trim().is_empty()) {
        return Err(StoreError::InvalidKnowledge(format!(
            "{index}번째 조각이 비었다"
        )));
    }
    Ok(())
}

pub(crate) fn index(
    conn: &Connection,
    record: &NewKnowledgeRecord,
) -> Result<Vec<ChunkId>, StoreError> {
    let kind = kind_code(record.kind);
    // 지우기 트리거가 색인과 임베딩도 함께 지운다.
    conn.execute(
        "DELETE FROM knowledge_chunks WHERE kind = ?1 AND record_ref = ?2",
        params![kind, record.record_ref],
    )?;
    let mut ids = Vec::with_capacity(record.chunks.len());
    for (ordinal, text) in record.chunks.iter().enumerate() {
        let ordinal = i64::try_from(ordinal)
            .map_err(|_| StoreError::InvalidKnowledge("조각이 너무 많다".to_string()))?;
        conn.execute(
            "INSERT INTO knowledge_chunks (kind, source, record_ref, ordinal, text, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                kind,
                source_code(record.source),
                record.record_ref,
                ordinal,
                text,
                record.at.0
            ],
        )?;
        ids.push(ChunkId(conn.last_insert_rowid()));
    }
    Ok(ids)
}

/// 검색 문법 글자를 글자 그대로 찾도록 낱말을 한 구절(`"..."`)로 감싼다.
fn phrase(word: &str) -> String {
    format!("\"{}\"", word.replace('"', "\"\""))
}

fn search(
    conn: &Connection,
    words: &[&str],
    query: &KnowledgeQuery,
) -> Result<Vec<KnowledgeHit>, StoreError> {
    let (long, short): (Vec<&str>, Vec<&str>) =
        words.iter().partition(|word| word.chars().count() >= 3);
    let mut args: Vec<Value> = Vec::new();
    let mut conditions: Vec<String> = Vec::new();
    let from = if long.is_empty() {
        "knowledge_chunks c".to_string()
    } else {
        args.push(Value::Text(
            long.iter().map(|w| phrase(w)).collect::<Vec<_>>().join(" "),
        ));
        conditions.push("knowledge_fts MATCH ?1".to_string());
        "knowledge_fts JOIN knowledge_chunks c ON c.id = knowledge_fts.rowid".to_string()
    };
    for word in short {
        args.push(Value::Text((*word).to_string()));
        conditions.push(format!("instr(lower(c.text), lower(?{})) > 0", args.len()));
    }
    if !query.kinds.is_empty() {
        let codes = query.kinds.iter().map(|k| kind_code(*k).to_string());
        conditions.push(in_list("c.kind", codes, &mut args));
    }
    if !query.sources.is_empty() {
        let codes = query.sources.iter().map(|s| source_code(*s).to_string());
        conditions.push(in_list("c.source", codes, &mut args));
    }
    if query.auto_recall {
        conditions.push("c.source <> 'external'".to_string());
    }
    args.push(Value::Integer(limit_of(query.max)));
    // 순서는 점수를 1e-4 단위로 맞춰 본다: 작은 문서 모음에서 글 길이만으로 생기는 아주 작은 차이는 같은 점수로 보고 조각 번호 순이다.
    let (rank, order) = if long.is_empty() {
        ("NULL", "c.id")
    } else {
        (
            "CAST(-knowledge_fts.rank * 1000000 AS INTEGER)",
            "round(knowledge_fts.rank, 4), c.id",
        )
    };
    let sql = format!(
        "SELECT {CHUNK_COLUMNS}, {rank} FROM {from} WHERE {} ORDER BY {order} LIMIT ?{}",
        conditions.join(" AND "),
        args.len()
    );
    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt.query(params_from_iter(args.iter()))?;
    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        let score = row.get::<_, Option<i64>>(7)?.map(score_of);
        out.push(KnowledgeHit {
            chunk: chunk_from(row)?,
            score_micros: score,
        });
    }
    Ok(out)
}

/// `c.col IN (?n, ...)` 조각을 만들고 값을 `args`에 더한다.
fn in_list(column: &str, codes: impl Iterator<Item = String>, args: &mut Vec<Value>) -> String {
    let mut marks = Vec::new();
    for code in codes {
        args.push(Value::Text(code));
        marks.push(format!("?{}", args.len()));
    }
    format!("{column} IN ({})", marks.join(", "))
}

/// FTS5의 `rank`(작을수록 잘 맞는 음수)를 정수로 옮긴 관련도(`-rank`의 백만 배)를
/// `관련도 / (1 + 관련도)`의 백만분의 1 단위 점수(0 초과 1_000_000 이하)로 바꾼다.
fn score_of(relevance_micros: i64) -> i64 {
    let relevance = i128::from(relevance_micros.max(1));
    let one = 1_000_000_i128;
    let score = relevance * one / (one + relevance);
    i64::try_from(score.max(1)).unwrap_or(1_000_000)
}

fn decode(model: String, dims: i64, bytes: Vec<u8>) -> Result<ChunkEmbedding, StoreError> {
    let dims = u32::try_from(dims)
        .map_err(|_| StoreError::Incompatible(format!("임베딩 차원 값이 이상하다: {dims}")))?;
    Ok(ChunkEmbedding {
        model,
        dims,
        bytes,
        quantized: false,
    })
}
