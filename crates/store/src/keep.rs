//! 모의계좌 상태 값(9차 4번, V6)과 접속 키 해시(9차 5번, M6).

use rusqlite::{Connection, OptionalExtension, Row, params};

use crate::codec::{self, insert_returning_seq};
use crate::model::UnixMillis;
use crate::{Store, StoreError};

/// 접속 키 해시에 친 소금의 가장 짧은 길이(바이트).
pub const ACCESS_KEY_SALT_MIN_BYTES: usize = 16;

/// (22차, 결정 32) 모의계좌 상태 값의 `ledger` 안에서 시세 관찰 칸(돈이 아닌 칸) 이름.
pub const PAPER_SNAPSHOT_MARKET_FIELDS: [&str; 3] = ["last_now", "last_quotes", "trade_marks"];

/// 저장된 모의계좌(`pi-paper`) 상태 값 한 벌.
#[derive(Debug, Clone, PartialEq)]
pub struct PaperSnapshot {
    /// 1부터 빠짐없이 커지는 번호.
    pub seq: u64,
    /// `pi-paper`가 내보낸 상태 값(JSON). 넣은 그대로 나온다.
    pub state: serde_json::Value,
    /// 저장한 시각.
    pub at: UnixMillis,
}

/// 접속 키의 해시. 키 원문은 받지 않는다(M6, 2026-11-16 결정 8: 소금을 친 SHA-256).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccessKeyHash {
    /// SHA-256 값(32바이트).
    pub sha256: [u8; 32],
    /// 소금. [`ACCESS_KEY_SALT_MIN_BYTES`]바이트 이상이어야 한다.
    pub salt: Vec<u8>,
}

/// 저장된 지금 접속 키 해시.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccessKeyRecord {
    /// 세대. 처음 둔 키가 1이고, 키를 바꿀 때마다 1씩 커진다(다른 기기의 세션을 끊는 데 쓴다).
    pub generation: u64,
    /// SHA-256 값.
    pub sha256: [u8; 32],
    /// 소금.
    pub salt: Vec<u8>,
    /// 둔 시각.
    pub set_at: UnixMillis,
}

impl Store {
    /// 모의계좌 상태 값을 저장하고 번호를 준다. 앞선 것을 지우지 않고 쌓는다.
    pub fn save_paper_snapshot(
        &self,
        state: &serde_json::Value,
        at: UnixMillis,
    ) -> Result<u64, StoreError> {
        let text = serde_json::to_string(state)?;
        self.write(|conn| {
            insert_returning_seq(
                conn,
                "INSERT INTO paper_snapshots (seq, at, state)
                 SELECT COALESCE(MAX(seq), 0) + 1, ?1, ?2 FROM paper_snapshots
                 RETURNING seq",
                params![at.0, text],
            )
        })
    }

    /// (22차) 거래일 마감의 끝값 한 줄. 뼈대.
    pub fn save_paper_day_end(&self, _at: UnixMillis) -> Result<Option<u64>, StoreError> {
        Ok(None)
    }

    /// 가장 나중에 저장한(번호가 가장 큰) 모의계좌 상태 값. 없으면 없음.
    pub fn latest_paper_snapshot(&self) -> Result<Option<PaperSnapshot>, StoreError> {
        self.read(latest_snapshot)
    }

    /// 접속 키 해시를 새 세대로 둔다(처음이면 1, 그다음부터 지금 세대 + 1). 앞선 세대는 더 쓰이지 않는다.
    ///
    /// 소금이 [`ACCESS_KEY_SALT_MIN_BYTES`]보다 짧으면 [`StoreError::AccessKeySaltTooShort`]이고 그대로다.
    pub fn set_access_key(
        &self,
        hash: AccessKeyHash,
        at: UnixMillis,
    ) -> Result<AccessKeyRecord, StoreError> {
        if hash.salt.len() < ACCESS_KEY_SALT_MIN_BYTES {
            return Err(StoreError::AccessKeySaltTooShort {
                len: hash.salt.len(),
            });
        }
        self.write(|conn| {
            let generation = insert_returning_seq(
                conn,
                "INSERT INTO key_hashes (generation, sha256, salt, set_at)
                 SELECT COALESCE(MAX(generation), 0) + 1, ?1, ?2, ?3 FROM key_hashes
                 RETURNING generation",
                params![hash.sha256.as_slice(), hash.salt.as_slice(), at.0],
            )?;
            Ok(AccessKeyRecord {
                generation,
                sha256: hash.sha256,
                salt: hash.salt,
                set_at: at,
            })
        })
    }

    /// 지금 접속 키 해시(가장 높은 세대). 둔 적이 없으면 없음.
    pub fn access_key(&self) -> Result<Option<AccessKeyRecord>, StoreError> {
        self.read(latest_access_key)
    }
}

fn latest_snapshot(conn: &Connection) -> Result<Option<PaperSnapshot>, StoreError> {
    let found = conn
        .query_row(
            "SELECT seq, at, state FROM paper_snapshots ORDER BY seq DESC LIMIT 1",
            [],
            |row| Ok(snapshot_from(row)),
        )
        .optional()?;
    found.transpose()
}

fn snapshot_from(row: &Row<'_>) -> Result<PaperSnapshot, StoreError> {
    Ok(PaperSnapshot {
        seq: codec::seq(row, 0)?,
        at: UnixMillis(row.get(1)?),
        state: serde_json::from_str(&row.get::<_, String>(2)?)?,
    })
}

fn latest_access_key(conn: &Connection) -> Result<Option<AccessKeyRecord>, StoreError> {
    let found = conn
        .query_row(
            "SELECT generation, sha256, salt, set_at FROM key_hashes
             ORDER BY generation DESC LIMIT 1",
            [],
            |row| Ok(access_key_from(row)),
        )
        .optional()?;
    found.transpose()
}

fn access_key_from(row: &Row<'_>) -> Result<AccessKeyRecord, StoreError> {
    let raw: Vec<u8> = row.get(1)?;
    let sha256: [u8; 32] = raw.as_slice().try_into().map_err(|_| {
        StoreError::Incompatible(format!("접속 키 해시 길이가 이상하다: {}", raw.len()))
    })?;
    Ok(AccessKeyRecord {
        generation: codec::seq(row, 0)?,
        sha256,
        salt: row.get(2)?,
        set_at: UnixMillis(row.get(3)?),
    })
}
