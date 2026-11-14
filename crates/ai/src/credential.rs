//! 자격 한 자리(D4). API 키와 OAuth 토큰이 같은 자리에 끼워진다.

use std::fmt;

use async_trait::async_trait;

/// 비밀 값이 있던 자리에 대신 찍는 글.
pub(crate) const REDACTED: &str = "[가려짐]";

/// 가려진 비밀 값. `Debug`로 찍어도 값이 나오지 않는다.
#[derive(Clone, PartialEq, Eq)]
pub struct Secret(String);

impl Secret {
    /// 비밀 값을 감싼다.
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// 감싼 값을 꺼낸다. 인증 헤더를 만들 때만 쓴다.
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Secret {
    /// 값 대신 가려진 글을 찍는다.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(REDACTED)
    }
}

/// AI 공급자에 내는 자격. API 키와 구독 OAuth 토큰이 같은 자리에 끼워진다(D4).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Credential {
    /// API 키. `x-api-key` 헤더로 나간다.
    ApiKey(Secret),
    /// 이미 발급된 OAuth 접근 토큰. `Authorization: Bearer` 헤더로 나간다.
    OAuthToken(Secret),
}

impl Credential {
    /// 종류와 상관없이 안에 든 비밀 값.
    pub(crate) fn secret(&self) -> &Secret {
        match self {
            Self::ApiKey(secret) | Self::OAuthToken(secret) => secret,
        }
    }
}

/// 지금 쓸 자격을 주는 곳. 등록된 자격이 없으면 `None`.
#[async_trait]
pub trait CredentialSource: Send + Sync {
    /// 지금 쓸 자격을 돌려준다.
    async fn current(&self) -> Option<Credential>;
}
