//! 8차에 더한 길: 공급자가 이름과 자기 키 칸을 선언한다 (결정 2026-11-21 2 · 3 · 4번, D9 · C3 · E4 · E38 · N8).
//!
//! 원본 동작 근거: Hermes `ProviderProfile`의 `name` · `env_vars`(같은 이름은 last-writer-wins) —
//! `docs/research/hermes-agent.md` 4.1. 보관 방식(`.env` 줄, 0600, 원자 쓰기)은 옛 칸과 같다.
//!
//! 옛 [`KeyKind`](crate::KeyKind) · [`KeySlot`](crate::KeySlot)과 그 등록 · 읽기 · 상태는 그대로 두고,
//! 새 공급자는 [`ProviderKeys`] 선언 하나로 붙는다. 선언은 그 공급자를 가진 모듈이 상수로 둔다.
//!
//! 줄 이름은 `PRICE_INHA_KEY__{공급자}__{칸}`(대문자, 구분은 밑줄 둘)이다. 이름 규칙이 밑줄 둘을
//! 막으므로 두 공급자의 줄 이름이 겹치지 않고, 옛 줄(`PRICE_INHA_BROKER_…` 등)과도 겹치지 않는다.
//! 보관소를 열 때 `.env`의 `PRICE_INHA_KEY__`로 시작하는 줄 값은 어떤 공급자가 선언했는지 몰라도
//! 모두 가리기 대상이다(8자 미만 값은 빼고).

use crate::kind::Entry;
use crate::{Credentials, SecretStore, SecretsError, Verifier};

/// 공급자 키 줄 이름의 머리. 열 때 이 머리로 시작하는 줄 값은 모두 가리기 대상이다.
pub(crate) const KEY_LINE_PREFIX: &str = "PRICE_INHA_KEY__";

/// 공급자 이름 · 칸 이름의 가장 긴 길이.
const MAX_NAME_CHARS: usize = 32;

/// 한 공급자가 가질 수 있는 칸의 가장 많은 개수.
const MAX_FIELDS: usize = 8;

/// 이름 규칙: 소문자 영문 · 숫자 · 밑줄, 영문으로 시작, 밑줄로 끝나지 않고 밑줄 둘이 붙지 않음, 1~32자.
fn is_valid_name(name: &str) -> bool {
    let bytes = name.as_bytes();
    let Some(first) = bytes.first() else {
        return false;
    };
    bytes.len() <= MAX_NAME_CHARS
        && first.is_ascii_lowercase()
        && bytes
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'_')
        && !name.ends_with('_')
        && !name.contains("__")
}

/// 한 공급자의 키 선언: 공급자 이름(공급자 ID)과 자기 키 칸 이름들.
///
/// 이름 규칙([`ProviderKeys::check`]): 공급자 이름과 칸 이름은 소문자 영문 · 숫자 · 밑줄 하나,
/// 영문으로 시작, 밑줄로 끝나지 않고 밑줄 둘이 붙지 않음, 1~32자. 칸은 1~8개이고 겹치지 않는다.
/// 등록 · 읽기 · 상태 보기는 언제나 이 검사를 먼저 하고, 어기면 [`SecretsError::InvalidProviderKeys`]다.
///
/// ```
/// use pi_secrets::ProviderKeys;
/// const VOYAGE_KEYS: ProviderKeys = ProviderKeys::new("voyage", &["api_key"]);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ProviderKeys {
    provider: &'static str,
    fields: &'static [&'static str],
}

impl ProviderKeys {
    /// 선언을 만든다. 상수로 선언할 수 있게 여기서는 검사하지 않는다([`ProviderKeys::check`]).
    pub const fn new(provider: &'static str, fields: &'static [&'static str]) -> Self {
        Self { provider, fields }
    }

    /// 공급자 이름(공급자 ID).
    pub fn provider(&self) -> &'static str {
        self.provider
    }

    /// 이 공급자의 키 칸 이름들. 등록할 때 이 칸이 모두 있어야 하고 다른 칸은 없어야 한다.
    pub fn fields(&self) -> &'static [&'static str] {
        self.fields
    }

    /// 이름 규칙을 검사한다. 어기면 [`SecretsError::InvalidProviderKeys`](값 · 이름을 싣지 않는다).
    pub fn check(&self) -> Result<(), SecretsError> {
        let fields = self.fields;
        let valid = is_valid_name(self.provider)
            && (1..=MAX_FIELDS).contains(&fields.len())
            && fields.iter().all(|field| is_valid_name(field))
            && fields
                .iter()
                .enumerate()
                .all(|(at, field)| !fields[..at].contains(field));
        if valid {
            Ok(())
        } else {
            Err(SecretsError::InvalidProviderKeys)
        }
    }

    /// 그 칸의 `.env` 줄 이름: `PRICE_INHA_KEY__{공급자}__{칸}`(대문자).
    /// 선언에 없는 칸이거나 선언이 이름 규칙을 어기면 `None`.
    pub fn env_name(&self, field: &str) -> Option<String> {
        if self.check().is_err() || !self.fields.contains(&field) {
            return None;
        }
        Some(format!(
            "{KEY_LINE_PREFIX}{}__{}",
            self.provider.to_ascii_uppercase(),
            field.to_ascii_uppercase()
        ))
    }
}

/// 공급자 키 등록 성공 표시. 값은 없다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProviderRegistered {
    /// 등록된 공급자 이름.
    pub provider: &'static str,
}

/// 한 공급자의 등록 상태. 값 · 값의 일부 · 길이는 없다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProviderStatus {
    /// 공급자 이름.
    pub provider: &'static str,
    /// 칸이 모두 [`MIN_SECRET_CHARS`](crate::MIN_SECRET_CHARS)자 이상으로 적혀 있는가.
    pub registered: bool,
}

impl SecretStore {
    /// 공급자 키를 등록한다. 이미 있으면 그 공급자의 줄만 바꾼다
    /// (다른 줄 · 설명 · 모르는 줄은 글자 그대로 둔다).
    ///
    /// [`SecretStore::register_slot`]과 같은 순서다: 이름 규칙([`ProviderKeys::check`]) → 칸 검사
    /// (빠진 칸 · 모르는 칸 · 8자 미만 · 못 쓰는 글자, 이때는 확인기를 부르지 않는다) →
    /// **확인기를 부르기 전에** 가리기 대상에 넣기 → [`Verifier::verify_provider`]로 확인
    /// (실패하면 저장하지 않고 [`SecretsError::VerifyFailed`], 값은 계속 가린다) → `.env` 원자 쓰기(0600).
    /// 성공하면 값이 없는 [`ProviderRegistered`]만 돌려준다.
    pub async fn register_provider<V: Verifier + ?Sized>(
        &self,
        keys: ProviderKeys,
        values: Credentials,
        verifier: &V,
    ) -> Result<ProviderRegistered, SecretsError> {
        keys.check()?;
        let pending = self.begin_entry(Entry::Provider(keys), values)?;
        verifier
            .verify_provider(keys, pending.values())
            .await
            .map_err(SecretsError::VerifyFailed)?;
        self.finish_entry(pending)?;
        Ok(ProviderRegistered {
            provider: keys.provider(),
        })
    }

    /// 등록된 공급자 키를 읽는다.
    ///
    /// 칸이 모두 있고 모두 [`MIN_SECRET_CHARS`](crate::MIN_SECRET_CHARS)자 이상이어야 `Some`.
    /// 하나라도 없거나 짧으면 `None`. 그 공급자의 줄이 깨졌으면 [`SecretsError::Corrupt`]
    /// (다른 공급자 · 옛 칸 읽기에는 지장 없다).
    pub fn read_provider(&self, keys: ProviderKeys) -> Result<Option<Credentials>, SecretsError> {
        keys.check()?;
        let _guard = self.lock_files();
        self.read_entry_locked(Entry::Provider(keys))
    }

    /// 넘긴 선언 순서대로 공급자별 등록 여부. 값 · 값의 일부 · 길이는 없다.
    ///
    /// 옛 [`SecretStore::status`] · [`SecretStore::slot_status`]에는 공급자 칸이 나오지 않는다.
    pub fn provider_status(
        &self,
        keys: &[ProviderKeys],
    ) -> Result<Vec<ProviderStatus>, SecretsError> {
        keys.iter()
            .map(|&keys| {
                keys.check()?;
                let registered = self.is_registered(Entry::Provider(keys))?;
                Ok(ProviderStatus {
                    provider: keys.provider(),
                    registered,
                })
            })
            .collect()
    }
}
