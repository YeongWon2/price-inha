//! 조립: `pi-secrets`를 `pi-market` · `pi-ai`의 자격 자리와 확인기에 잇는다.

use std::sync::Arc;

use async_trait::async_trait;
use pi_ai::AnthropicConfig;
use pi_market::TossConfig;
use pi_secrets::{Credentials, KeyKind, SecretStore, Verifier, VerifyFailure};

/// 바깥 서버 설정. 테스트는 가짜 서버 주소를 넣는다.
#[derive(Debug, Clone, Default)]
pub struct Upstreams {
    /// 토스증권.
    pub toss: TossConfig,
    /// Anthropic.
    pub anthropic: AnthropicConfig,
}

/// 증권사 자격 자리: `pi-secrets`의 [`KeyKind::Broker`]를 읽어 준다.
///
/// 부를 때마다 보관소를 읽는다. 등록되지 않았거나 읽지 못하면 `None`(V31).
pub struct BrokerKeys {
    _store: Arc<SecretStore>,
}

impl BrokerKeys {
    /// 보관소에 잇는다.
    pub fn new(store: Arc<SecretStore>) -> Self {
        todo!()
    }
}

impl pi_market::CredentialSource for BrokerKeys {
    fn credentials(&self) -> Option<pi_market::Credentials> {
        todo!()
    }
}

/// AI 자격 자리: [`KeyKind::AiApiKey`]는 `Credential::ApiKey`로,
/// [`KeyKind::AiOAuth`]는 `access_token`을 `Credential::OAuthToken`으로 준다.
///
/// 부를 때마다 보관소를 읽는다. 둘 다 없으면 `None`(V31).
pub struct AiKeys {
    _store: Arc<SecretStore>,
}

impl AiKeys {
    /// 보관소에 잇는다.
    pub fn new(store: Arc<SecretStore>) -> Self {
        todo!()
    }
}

#[async_trait]
impl pi_ai::CredentialSource for AiKeys {
    async fn current(&self) -> Option<pi_ai::Credential> {
        todo!()
    }
}

/// 키 확인기: `SecretStore::register`에 끼운다.
///
/// | 종류 | 확인 | 실패를 옮기는 법 |
/// |---|---|---|
/// | [`KeyKind::Broker`] | `pi_market::verify`(토큰 발급 한 번) | 연결 실패는 [`VerifyFailure::Unreachable`], 인증 실패는 [`VerifyFailure::Rejected`] |
/// | [`KeyKind::AiApiKey`] | `AnthropicProvider::verify`(`ApiKey`) | 연결 실패 · 시간 초과는 `Unreachable`, 인증 실패는 `Rejected` |
/// | [`KeyKind::AiOAuth`] | `AnthropicProvider::verify`(`OAuthToken`, `access_token`) | 위와 같다 |
/// | [`KeyKind::Data`] | 확인할 곳이 아직 없다 | 언제나 실패(저장하지 않는다) |
///
/// 그 밖의 실패(한도 초과 · 장애 등)도 `Ok`가 아니다(확인하지 못한 키는 저장하지 않는다, E4).
pub struct KeyVerifier {
    _upstreams: Upstreams,
}

impl KeyVerifier {
    /// 바깥 서버 설정으로 만든다.
    pub fn new(upstreams: Upstreams) -> Self {
        todo!()
    }
}

impl Verifier for KeyVerifier {
    async fn verify(&self, kind: KeyKind, values: &Credentials) -> Result<(), VerifyFailure> {
        todo!()
    }
}

/// 가리기 자리에 끼우는 것: `SecretStore::redact`.
pub struct SecretsRedactor {
    _store: Arc<SecretStore>,
}

impl SecretsRedactor {
    /// 보관소에 잇는다.
    pub fn new(store: Arc<SecretStore>) -> Self {
        todo!()
    }
}

impl pi_gate::Redactor for SecretsRedactor {
    fn redact(&self, text: &str) -> String {
        todo!()
    }
}
