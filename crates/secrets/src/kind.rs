//! 키 종류와 종류별 칸 · 파일 · 저장 이름.

use std::borrow::Cow;

use crate::{ENV_FILE, KeySlot, ProviderKeys};

/// 키 종류 (C3: 증권사 · AI · 자료).
///
/// AI 키는 API 키 하나다. 구독은 공식 Claude Code CLI에 맡기므로 이 보관소는 구독 토큰을
/// 받지도 쥐지도 않는다(D4, 2026-11-15 결정). 구독 토큰 종류로 등록하는 길은 없다.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeyKind {
    /// 증권사 키. 칸: `client_id`, `client_secret`. `.env`에 둔다.
    Broker,
    /// AI API 키. 칸: `api_key`. `.env`에 둔다.
    AiApiKey,
    /// 자료 키. 칸: `api_key`. `.env`에 둔다.
    Data,
}

impl KeyKind {
    /// 모든 종류. [`SecretStore::status`](crate::SecretStore::status)가 이 순서로 돌려준다.
    pub const ALL: [KeyKind; 3] = [KeyKind::Broker, KeyKind::AiApiKey, KeyKind::Data];

    /// 이 종류가 가진 칸 이름. 등록할 때 이 칸이 모두 있어야 하고 다른 칸은 없어야 한다.
    pub const fn fields(self) -> &'static [&'static str] {
        match self {
            KeyKind::Broker => &["client_id", "client_secret"],
            KeyKind::AiApiKey => &["api_key"],
            KeyKind::Data => &["api_key"],
        }
    }

    /// 이 종류가 저장되는 파일 이름. 모든 종류가 [`ENV_FILE`]에 둔다.
    pub const fn file_name(self) -> &'static str {
        match self {
            KeyKind::Broker | KeyKind::AiApiKey | KeyKind::Data => ENV_FILE,
        }
    }

    /// `.env`에서 쓰는 줄 이름. [`KeyKind::fields`]와 같은 순서다.
    pub(crate) const fn env_names(self) -> &'static [&'static str] {
        match self {
            KeyKind::Broker => &[
                "PRICE_INHA_BROKER_CLIENT_ID",
                "PRICE_INHA_BROKER_CLIENT_SECRET",
            ],
            KeyKind::AiApiKey => &["PRICE_INHA_AI_API_KEY"],
            KeyKind::Data => &["PRICE_INHA_DATA_API_KEY"],
        }
    }
}

/// 키 파일에 두는 묶음 하나: 기존 종류 · 6차 칸 · VAPID 비밀키. 이 모듈 안에서만 쓴다.
///
/// 모두 [`ENV_FILE`]의 `이름=값` 줄로 두고, 칸 검사 · 가리기 대상 · 원자 쓰기를 함께 쓴다.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Entry {
    /// 기존 종류.
    Kind(KeyKind),
    /// 6차 칸.
    Slot(KeySlot),
    /// 웹 푸시 VAPID 비밀키. 칸: `private_key`.
    Vapid,
    /// 8차 공급자 키. 칸과 줄 이름은 선언([`ProviderKeys`])이 정한다.
    /// 이름 규칙([`ProviderKeys::check`])을 지난 선언만 여기에 온다.
    Provider(ProviderKeys),
}

impl Entry {
    /// VAPID 비밀키의 칸 이름.
    pub(crate) const VAPID_FIELD: &'static str = "private_key";

    /// 열 때 읽어 가리기 대상에 올리는 모든 묶음.
    pub(crate) fn all() -> impl Iterator<Item = Entry> {
        KeyKind::ALL
            .into_iter()
            .map(Entry::Kind)
            .chain(KeySlot::ALL.into_iter().map(Entry::Slot))
            .chain([Entry::Vapid])
    }

    /// 칸 이름. 등록할 때 이 칸이 모두 있어야 하고 다른 칸은 없어야 한다.
    pub(crate) fn fields(self) -> &'static [&'static str] {
        match self {
            Entry::Kind(kind) => kind.fields(),
            Entry::Slot(slot) => slot.fields(),
            Entry::Vapid => &[Entry::VAPID_FIELD],
            Entry::Provider(keys) => keys.fields(),
        }
    }

    /// `.env`에서 쓰는 줄 이름. [`Entry::fields`]와 같은 순서다.
    pub(crate) fn env_names(self) -> Vec<Cow<'static, str>> {
        if let Entry::Provider(keys) = self {
            // 이름 규칙을 지난 선언만 오므로 칸마다 줄 이름이 있다.
            return keys
                .fields()
                .iter()
                .filter_map(|field| keys.env_name(field))
                .map(Cow::Owned)
                .collect();
        }
        self.static_env_names()
            .iter()
            .map(|&name| Cow::Borrowed(name))
            .collect()
    }

    /// 줄 이름이 고정된 묶음(공급자 밖)의 줄 이름.
    const fn static_env_names(self) -> &'static [&'static str] {
        match self {
            Entry::Kind(kind) => kind.env_names(),
            Entry::Slot(KeySlot::Dart) => &["PRICE_INHA_DART_API_KEY"],
            Entry::Slot(KeySlot::Ecos) => &["PRICE_INHA_ECOS_API_KEY"],
            Entry::Slot(KeySlot::Fred) => &["PRICE_INHA_FRED_API_KEY"],
            Entry::Slot(KeySlot::BackupGithubToken) => &["PRICE_INHA_BACKUP_GITHUB_TOKEN"],
            Entry::Vapid => &["PRICE_INHA_VAPID_PRIVATE_KEY"],
            Entry::Provider(_) => &[],
        }
    }

    /// 저장되는 파일 이름. 모두 [`ENV_FILE`]이다.
    pub(crate) const fn file_name(self) -> &'static str {
        match self {
            Entry::Kind(kind) => kind.file_name(),
            Entry::Slot(_) | Entry::Vapid | Entry::Provider(_) => ENV_FILE,
        }
    }
}

impl From<KeyKind> for Entry {
    fn from(kind: KeyKind) -> Self {
        Entry::Kind(kind)
    }
}

impl From<KeySlot> for Entry {
    fn from(slot: KeySlot) -> Self {
        Entry::Slot(slot)
    }
}
