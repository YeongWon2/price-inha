//! 모의계좌 상태 값(9차 4번, V6)과 접속 키 해시(9차 5번, M6).

use crate::model::UnixMillis;
use crate::{Store, StoreError};

/// 접속 키 해시에 친 소금의 가장 짧은 길이(바이트).
pub const ACCESS_KEY_SALT_MIN_BYTES: usize = 16;

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
        let _ = (state, at);
        todo!()
    }

    /// 가장 나중에 저장한(번호가 가장 큰) 모의계좌 상태 값. 없으면 없음.
    pub fn latest_paper_snapshot(&self) -> Result<Option<PaperSnapshot>, StoreError> {
        todo!()
    }

    /// 접속 키 해시를 새 세대로 둔다(처음이면 1, 그다음부터 지금 세대 + 1). 앞선 세대는 더 쓰이지 않는다.
    ///
    /// 소금이 [`ACCESS_KEY_SALT_MIN_BYTES`]보다 짧으면 [`StoreError::AccessKeySaltTooShort`]이고 그대로다.
    pub fn set_access_key(
        &self,
        hash: AccessKeyHash,
        at: UnixMillis,
    ) -> Result<AccessKeyRecord, StoreError> {
        let _ = (hash, at);
        todo!()
    }

    /// 지금 접속 키 해시(가장 높은 세대). 둔 적이 없으면 없음.
    pub fn access_key(&self) -> Result<Option<AccessKeyRecord>, StoreError> {
        todo!()
    }
}
