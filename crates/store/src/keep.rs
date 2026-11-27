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
    fn paper_tail_lock(&self) -> std::sync::MutexGuard<'_, Option<serde_json::Value>> {
        self.paper_tail
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// 모의계좌 상태 값을 저장하고 번호를 준다. 앞선 것을 지우지 않고 쌓는다.
    ///
    /// (22차, 결정 32) 바뀐 때만 남긴다: 바로 앞 줄과 [`PAPER_SNAPSHOT_MARKET_FIELDS`](시세 관찰 칸)만 빼고 같으면
    /// 새 줄을 쓰지 않고 앞 줄의 번호를 준다. 그 상태는 메모리에 두었다가 [`Store::save_paper_day_end`]가 그날
    /// 끝값으로 남긴다(꺼지면 그 사이의 시세 관찰 칸만 잃는다 — 돈 · 보유 · 주문 · 체결은 늘 남아 있다).
    pub fn save_paper_snapshot(
        &self,
        state: &serde_json::Value,
        at: UnixMillis,
    ) -> Result<u64, StoreError> {
        let text = serde_json::to_string(state)?;
        let mut tail = self.paper_tail_lock();
        let saved = self.write(|conn| {
            if let Some(latest) = latest_snapshot(conn)?
                && same_money(&latest.state, state)
            {
                return Ok((latest.seq, false));
            }
            insert_snapshot(conn, at, &text).map(|seq| (seq, true))
        })?;
        *tail = if saved.1 { None } else { Some(state.clone()) };
        Ok(saved.0)
    }

    /// (22차, 결정 32) 거래일 마감의 끝값 한 줄: 쓰지 않고 둔 가장 나중 상태(없으면 마지막 줄의 상태)를 `at`에 새 줄로
    /// 남긴다. 같아도 쓴다. 저장한 상태가 하나도 없으면 아무것도 쓰지 않고 `None`.
    pub fn save_paper_day_end(&self, at: UnixMillis) -> Result<Option<u64>, StoreError> {
        let mut tail = self.paper_tail_lock();
        let pending = tail.clone();
        let seq = self.write(|conn| {
            let text = match &pending {
                Some(state) => serde_json::to_string(state)?,
                None => match latest_snapshot(conn)? {
                    Some(latest) => serde_json::to_string(&latest.state)?,
                    None => return Ok(None),
                },
            };
            insert_snapshot(conn, at, &text).map(Some)
        })?;
        *tail = None;
        Ok(seq)
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

fn insert_snapshot(conn: &Connection, at: UnixMillis, text: &str) -> Result<u64, StoreError> {
    insert_returning_seq(
        conn,
        "INSERT INTO paper_snapshots (seq, at, state)
         SELECT COALESCE(MAX(seq), 0) + 1, ?1, ?2 FROM paper_snapshots
         RETURNING seq",
        params![at.0, text],
    )
}

/// 시세 관찰 칸(`ledger` 안의 [`PAPER_SNAPSHOT_MARKET_FIELDS`])을 빼고 같은가.
fn same_money(a: &serde_json::Value, b: &serde_json::Value) -> bool {
    without_market(a) == without_market(b)
}

fn without_market(state: &serde_json::Value) -> serde_json::Value {
    let mut copy = state.clone();
    if let Some(ledger) = copy
        .get_mut("ledger")
        .and_then(serde_json::Value::as_object_mut)
    {
        for field in PAPER_SNAPSHOT_MARKET_FIELDS {
            ledger.remove(field);
        }
    }
    copy
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
