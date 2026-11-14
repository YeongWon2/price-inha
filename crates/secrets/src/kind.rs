//! 키 종류와 종류별 칸 · 파일 · 저장 이름.

use crate::ENV_FILE;

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
