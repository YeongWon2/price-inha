//! 6차에 더한 칸: 자료 출처별 키(D9) · 백업 GitHub 토큰(D7) · VAPID 비밀키(결정 2026-11-16 24번),
//! 그리고 바이트 안에 등록된 값이 있는지 보는 검사(E39 · N8).
//!
//! [`KeyKind`](crate::KeyKind)에 변형을 더하지 않는다: 조립(`crates/server`)이 그 enum을 빠짐없이
//! `match`하므로 변형을 더하면 깨진다. 그래서 새 칸은 따로 [`KeySlot`]으로 두고, 등록 · 읽기 ·
//! 상태 보기도 따로 둔다. 확인 규칙은 다른 키와 같다(칸 검사 → 가리기 대상에 넣기 → 확인 → 원자 쓰기).

use crate::kind::Entry;
use crate::{Credentials, ENV_FILE, SecretStore, SecretString, SecretsError, Verifier};

/// 6차에 더한 칸 (D9 · D7).
///
/// 모든 칸은 [`ENV_FILE`](crate::ENV_FILE)에 둔다(권한 0600, 원자 쓰기). 백업은
/// [`BACKUP_EXCLUDED_FILES`](crate::BACKUP_EXCLUDED_FILES)를 대상에서 뺀다.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum KeySlot {
    /// DART 오픈API 키(국내 공시 · 재무). 칸: `api_key`.
    Dart,
    /// 한국은행 ECOS 키(거시). 칸: `api_key`.
    Ecos,
    /// 미국 FRED 키(거시). 칸: `api_key`.
    Fred,
    /// 백업을 올리는 GitHub fine-grained 토큰 하나(D7, 결정 23번). 칸: `token`.
    BackupGithubToken,
}

impl KeySlot {
    /// 모든 칸. [`SecretStore::slot_status`]가 이 순서로 돌려준다.
    pub const ALL: [KeySlot; 4] = [
        KeySlot::Dart,
        KeySlot::Ecos,
        KeySlot::Fred,
        KeySlot::BackupGithubToken,
    ];

    /// 자료 출처 칸(D9, 결정 18번).
    pub const DATA_SOURCES: [KeySlot; 3] = [KeySlot::Dart, KeySlot::Ecos, KeySlot::Fred];

    /// 이 칸이 가진 칸 이름. 등록할 때 이 칸이 모두 있어야 하고 다른 칸은 없어야 한다.
    pub const fn fields(self) -> &'static [&'static str] {
        match self {
            KeySlot::Dart | KeySlot::Ecos | KeySlot::Fred => &["api_key"],
            KeySlot::BackupGithubToken => &["token"],
        }
    }
}

/// 칸 등록 성공 표시. 값은 없다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SlotRegistered {
    /// 등록된 칸.
    pub slot: KeySlot,
}

/// 한 칸의 등록 상태. 값 · 값의 일부 · 길이는 없다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SlotStatus {
    /// 칸.
    pub slot: KeySlot,
    /// 등록됐는가.
    pub registered: bool,
}

impl SecretStore {
    /// 6차 칸을 등록한다 (D9 · D7). 이미 있으면 덮어쓴다.
    ///
    /// [`SecretStore::register`]와 같은 순서다: 칸 검사([`KeySlot::fields`], 짧은 값 · 못 쓰는 글자 거부,
    /// 이때는 확인기를 부르지 않는다) → 가리기 대상에 넣기 → [`Verifier::verify_slot`]으로 확인
    /// (실패하면 저장하지 않고 [`SecretsError::VerifyFailed`]) → 원자 쓰기.
    /// 성공하면 값이 없는 [`SlotRegistered`]만 돌려준다. 다른 칸과 [`KeyKind`](crate::KeyKind)의 값은 그대로 둔다.
    pub async fn register_slot<V: Verifier + ?Sized>(
        &self,
        slot: KeySlot,
        values: Credentials,
        verifier: &V,
    ) -> Result<SlotRegistered, SecretsError> {
        let pending = self.begin_entry(slot.into(), values)?;
        verifier
            .verify_slot(slot, pending.values())
            .await
            .map_err(SecretsError::VerifyFailed)?;
        self.finish_entry(pending)?;
        Ok(SlotRegistered { slot })
    }

    /// 등록된 칸을 읽는다. 없으면 `None`. 값은 [`SecretString`]에 담겨 나온다.
    ///
    /// 쓰는 곳: 자료 어댑터(출처별 키), 백업기(GitHub 토큰). 키 파일에 손으로 적힌
    /// [`MIN_SECRET_CHARS`](crate::MIN_SECRET_CHARS) 미만 값은 등록되지 않은 것으로 본다.
    pub fn read_slot(&self, slot: KeySlot) -> Result<Option<Credentials>, SecretsError> {
        let _guard = self.lock_files();
        self.read_entry_locked(slot.into())
    }

    /// 칸별 등록 여부([`KeySlot::ALL`] 순서). 값 · 값의 일부 · 길이는 없다.
    pub fn slot_status(&self) -> Result<Vec<SlotStatus>, SecretsError> {
        KeySlot::ALL
            .into_iter()
            .map(|slot| {
                let registered = self.is_registered(slot.into())?;
                Ok(SlotStatus { slot, registered })
            })
            .collect()
    }

    /// 웹 푸시 VAPID 비밀키 (결정 2026-11-16 24번).
    ///
    /// 이미 있으면 그 값을 돌려주고 `generate`를 부르지 않는다. 없으면(처음 켤 때) `generate`로
    /// 만든 값을 키 파일에 원자 쓰기로 두고(0600) 돌려준다. 그 파일은
    /// [`BACKUP_EXCLUDED_FILES`](crate::BACKUP_EXCLUDED_FILES)에 들어 있어 백업에서 빠진다.
    /// 값은 가리기 대상이다([`SecretStore::redact`] · [`SecretStore::contains_registered`]).
    ///
    /// 키 파일에 손으로 적은 값이 있는데 [`MIN_SECRET_CHARS`](crate::MIN_SECRET_CHARS)보다 짧거나 줄이
    /// 깨졌으면 [`SecretsError::Corrupt`]로 알리고 `generate`를 부르지 않으며 파일을 덮어쓰지 않는다.
    ///
    /// 만든 값이 [`MIN_SECRET_CHARS`](crate::MIN_SECRET_CHARS)보다 짧으면 [`SecretsError::TooShort`],
    /// 못 쓰는 글자가 있으면 [`SecretsError::InvalidValue`]로 거부하고 저장하지 않는다.
    pub fn vapid_private_key(
        &self,
        generate: impl FnOnce() -> SecretString,
    ) -> Result<SecretString, SecretsError> {
        let field = Entry::VAPID_FIELD;
        // 읽기 · 만들기 · 저장을 한 잠금 안에서 한다(함께 처음 켜져도 값이 둘이 되지 않게).
        let _guard = self.lock_files();
        if let Some(found) = self.read_entry_locked(Entry::Vapid)?
            && let Some(value) = found.get(field)
        {
            return Ok(value.clone());
        }
        // 손으로 짧게 고친 값이 있으면 "없음"이 아니다: 덮어쓰지 않고 깨졌다고 알린다.
        if self.has_unusable_entry_locked(Entry::Vapid)? {
            return Err(SecretsError::Corrupt { file: ENV_FILE });
        }
        let value = generate();
        let mut values = Credentials::new();
        values.set(field, value.clone());
        // 칸 검사(짧은 값 · 못 쓰는 글자)에 걸리면 저장하지 않는다.
        let pending = self.begin_entry(Entry::Vapid, values)?;
        self.finish_entry_locked(pending)?;
        Ok(value)
    }

    /// 바이트 안에 등록된 비밀 값이 들어 있는가 (E39 · N8). 값은 돌려주지 않는다.
    ///
    /// [`SecretStore::redact`]가 가리는 값 모두(키 파일에서 읽은 값, 등록 · 확인 중인 값, 6차 칸,
    /// VAPID 비밀키)를 그대로 · URL 인코딩 · Base64 모양으로 찾는다. 바이트가 UTF-8이 아니어도 된다
    /// (예: SQLite 스냅샷). 백업기는 이것이 `true`면 올리지 않는다.
    ///
    /// 값 하나마다 한 번씩 훑는다(첫 글자가 나오는 자리에서만 맞춰 본다). 대상 밖: 76자마다
    /// 줄바꿈을 넣은 Base64(MIME 모양)는 줄바꿈이 값 가운데 들어가 못 찾는다. 백업기는 압축 **전에**
    /// 검사한다(압축하면 어느 모양이든 찾을 수 없다).
    pub fn contains_registered(&self, bytes: &[u8]) -> bool {
        self.targets_found_in(bytes)
    }
}
