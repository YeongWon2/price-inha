//! 키 종류와 종류별 칸 · 파일 · 저장 이름.

use crate::{AUTH_FILE, ENV_FILE};

/// 키 종류 (C3: 증권사 · AI · 자료).
///
/// AI는 저장 파일이 둘이라(API 키는 `.env`, 구독 OAuth 토큰은 `auth.json`) 둘로 나눴다.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeyKind {
    /// 증권사 키. 칸: `client_id`, `client_secret`. `.env`에 둔다.
    Broker,
    /// AI API 키. 칸: `api_key`. `.env`에 둔다.
    AiApiKey,
    /// AI 구독 OAuth 토큰. 칸: `access_token`, `refresh_token`. `auth.json`에 둔다.
    AiOAuth,
    /// 자료 키. 칸: `api_key`. `.env`에 둔다.
    Data,
}

impl KeyKind {
    /// 모든 종류. [`SecretStore::status`](crate::SecretStore::status)가 이 순서로 돌려준다.
    pub const ALL: [KeyKind; 4] = [
        KeyKind::Broker,
        KeyKind::AiApiKey,
        KeyKind::AiOAuth,
        KeyKind::Data,
    ];

    /// 이 종류가 가진 칸 이름. 등록할 때 이 칸이 모두 있어야 하고 다른 칸은 없어야 한다.
    pub const fn fields(self) -> &'static [&'static str] {
        match self {
            KeyKind::Broker => &["client_id", "client_secret"],
            KeyKind::AiApiKey => &["api_key"],
            KeyKind::AiOAuth => &["access_token", "refresh_token"],
            KeyKind::Data => &["api_key"],
        }
    }

    /// 이 종류가 저장되는 파일 이름([`ENV_FILE`] 또는 [`AUTH_FILE`]).
    pub const fn file_name(self) -> &'static str {
        match self {
            KeyKind::AiOAuth => AUTH_FILE,
            KeyKind::Broker | KeyKind::AiApiKey | KeyKind::Data => ENV_FILE,
        }
    }

    /// `.env`에서 쓰는 줄 이름. [`KeyKind::fields`]와 같은 순서다.
    /// `auth.json`에 두는 종류는 비어 있다.
    pub(crate) const fn env_names(self) -> &'static [&'static str] {
        match self {
            KeyKind::Broker => &[
                "PRICE_INHA_BROKER_CLIENT_ID",
                "PRICE_INHA_BROKER_CLIENT_SECRET",
            ],
            KeyKind::AiApiKey => &["PRICE_INHA_AI_API_KEY"],
            KeyKind::Data => &["PRICE_INHA_DATA_API_KEY"],
            KeyKind::AiOAuth => &[],
        }
    }

    /// `auth.json`에서 이 종류의 칸들을 담는 묶음 이름.
    pub(crate) const fn json_key(self) -> &'static str {
        match self {
            KeyKind::Broker => "broker",
            KeyKind::AiApiKey => "ai_api_key",
            KeyKind::AiOAuth => "ai_oauth",
            KeyKind::Data => "data",
        }
    }
}
