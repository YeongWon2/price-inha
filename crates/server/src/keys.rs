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
    store: Arc<SecretStore>,
}

impl BrokerKeys {
    /// 보관소에 잇는다.
    pub fn new(store: Arc<SecretStore>) -> Self {
        Self { store }
    }
}

impl pi_market::CredentialSource for BrokerKeys {
    fn credentials(&self) -> Option<pi_market::Credentials> {
        let values = read_kind(&self.store, KeyKind::Broker)?;
        broker_credentials(&values)
    }
}

/// 보관소에서 한 종류를 읽는다. 없거나 읽지 못하면 `None`(값은 기록하지 않는다).
fn read_kind(store: &SecretStore, kind: KeyKind) -> Option<Credentials> {
    match store.read(kind) {
        Ok(values) => values,
        Err(error) => {
            tracing::warn!(?kind, %error, "키 파일을 읽지 못해 등록되지 않은 것으로 봅니다");
            None
        }
    }
}

/// 칸의 값. 앞뒤 공백 · 줄바꿈을 떼고, 남는 것이 없으면 없는 것으로 본다.
fn field<'a>(values: &'a Credentials, name: &str) -> Option<&'a str> {
    let value = values.get(name)?.expose().trim();
    (!value.is_empty()).then_some(value)
}

fn broker_credentials(values: &Credentials) -> Option<pi_market::Credentials> {
    Some(pi_market::Credentials::new(
        field(values, "client_id")?,
        field(values, "client_secret")?,
    ))
}

/// AI 자격 자리: [`KeyKind::AiApiKey`]는 `Credential::ApiKey`로,
/// [`KeyKind::AiOAuth`]는 `access_token`을 `Credential::OAuthToken`으로 준다.
///
/// 부를 때마다 보관소를 읽는다. 둘 다 없으면 `None`(V31).
///
/// 둘 다 있으면 API 키를 쓴다.
pub struct AiKeys {
    store: Arc<SecretStore>,
}

impl AiKeys {
    /// 보관소에 잇는다.
    pub fn new(store: Arc<SecretStore>) -> Self {
        Self { store }
    }
}

#[async_trait]
impl pi_ai::CredentialSource for AiKeys {
    async fn current(&self) -> Option<pi_ai::Credential> {
        let api_key = read_kind(&self.store, KeyKind::AiApiKey)
            .and_then(|values| field(&values, "api_key").map(pi_ai::Secret::new));
        if let Some(key) = api_key {
            return Some(pi_ai::Credential::ApiKey(key));
        }
        read_kind(&self.store, KeyKind::AiOAuth)
            .and_then(|values| field(&values, "access_token").map(pi_ai::Secret::new))
            .map(pi_ai::Credential::OAuthToken)
    }
}

/// 확인할 때만 쓰는 빈 자격 자리(확인은 넘겨받은 자격으로 한다).
struct NoKeys;

#[async_trait]
impl pi_ai::CredentialSource for NoKeys {
    async fn current(&self) -> Option<pi_ai::Credential> {
        None
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
    upstreams: Upstreams,
}

impl KeyVerifier {
    /// 바깥 서버 설정으로 만든다.
    pub fn new(upstreams: Upstreams) -> Self {
        Self { upstreams }
    }

    async fn verify_broker(&self, values: &Credentials) -> Result<(), VerifyFailure> {
        let credentials = broker_credentials(values).ok_or(VerifyFailure::Rejected)?;
        pi_market::verify(&self.upstreams.toss, &credentials)
            .await
            .map_err(|error| match error {
                pi_market::MarketError::AuthFailed { .. }
                | pi_market::MarketError::IpNotAllowed
                | pi_market::MarketError::Rejected { .. } => VerifyFailure::Rejected,
                // 연결 실패와, 닿았어도 확인을 끝내지 못한 것(한도 · 장애 · 읽을 수 없는 응답).
                _ => VerifyFailure::Unreachable,
            })
    }

    async fn verify_ai(&self, credential: Option<pi_ai::Credential>) -> Result<(), VerifyFailure> {
        let credential = credential.ok_or(VerifyFailure::Rejected)?;
        let provider =
            pi_ai::AnthropicProvider::new(self.upstreams.anthropic.clone(), Arc::new(NoKeys));
        provider
            .verify(&credential)
            .await
            .map_err(|error| match error {
                pi_ai::AiError::Auth { .. } | pi_ai::AiError::InvalidRequest { .. } => {
                    VerifyFailure::Rejected
                }
                _ => VerifyFailure::Unreachable,
            })
    }
}

impl Verifier for KeyVerifier {
    async fn verify(&self, kind: KeyKind, values: &Credentials) -> Result<(), VerifyFailure> {
        let secret = |name: &str| field(values, name).map(pi_ai::Secret::new);
        match kind {
            KeyKind::Broker => self.verify_broker(values).await,
            KeyKind::AiApiKey => {
                let credential = secret("api_key").map(pi_ai::Credential::ApiKey);
                self.verify_ai(credential).await
            }
            KeyKind::AiOAuth => {
                let credential = secret("access_token").map(pi_ai::Credential::OAuthToken);
                self.verify_ai(credential).await
            }
            // 자료 키는 확인할 곳이 아직 없다. 확인하지 못한 키는 저장하지 않는다(E4).
            // 어느 서버로도 보내지 않는다.
            KeyKind::Data => Err(VerifyFailure::Unreachable),
        }
    }
}

/// 가리기 자리에 끼우는 것: `SecretStore::redact`.
pub struct SecretsRedactor {
    store: Arc<SecretStore>,
}

impl SecretsRedactor {
    /// 보관소에 잇는다.
    pub fn new(store: Arc<SecretStore>) -> Self {
        Self { store }
    }
}

impl pi_gate::Redactor for SecretsRedactor {
    fn redact(&self, text: &str) -> String {
        self.store.redact(text)
    }
}
