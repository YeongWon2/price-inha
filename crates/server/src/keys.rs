//! 조립: `pi-secrets`를 `pi-market` · `pi-ai`의 자격 자리와 확인기에 잇는다.

use std::sync::{Arc, Mutex, PoisonError};
use std::time::Instant;

use async_trait::async_trait;
use pi_ai::AnthropicConfig;
use pi_data::DataConfig;
use pi_market::TossConfig;
use pi_secrets::{Credentials, KeyKind, KeySlot, SecretStore, Verifier, VerifyFailure};

use crate::retry::{LOG_EVERY, LogGate};

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
    /// 읽지 못했다는 경고를 묶는다(키가 없는 동안 2초마다 읽는다).
    quiet: Quiet,
}

impl BrokerKeys {
    /// 보관소에 잇는다.
    pub fn new(store: Arc<SecretStore>) -> Self {
        Self {
            store,
            quiet: Quiet::default(),
        }
    }
}

impl pi_market::CredentialSource for BrokerKeys {
    fn credentials(&self) -> Option<pi_market::Credentials> {
        let values = read_kind(&self.store, KeyKind::Broker, &self.quiet)?;
        broker_credentials(&values)
    }
}

/// 되풀이되는 같은 경고를 묶는 자리(자격 자리는 `&self`로 불리므로 잠금 안에 둔다).
struct Quiet(Mutex<LogGate>);

impl Default for Quiet {
    fn default() -> Self {
        Self(Mutex::new(LogGate::new(LOG_EVERY)))
    }
}

impl Quiet {
    /// 이 실패를 지금 기록할 것인가.
    fn should_log(&self, what: &str) -> bool {
        // 잠금을 쥔 채 죽은 스레드가 있어도 묶는 값은 온전하다.
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .should_log(what, Instant::now())
    }

    /// 실패가 풀렸다.
    fn clear(&self) {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clear();
    }
}

/// 보관소에서 한 종류를 읽는다. 없거나 읽지 못하면 `None`(값은 기록하지 않는다).
///
/// 읽지 못한 경고는 실패가 바뀔 때와 [`LOG_EVERY`]마다만 남긴다(같은 경고를 끝없이 찍지 않는다).
fn read_kind(store: &SecretStore, kind: KeyKind, quiet: &Quiet) -> Option<Credentials> {
    match store.read(kind) {
        Ok(values) => {
            quiet.clear();
            values
        }
        Err(error) => {
            if quiet.should_log(&error.to_string()) {
                tracing::warn!(
                    ?kind,
                    %error,
                    "키 파일을 읽지 못해 등록되지 않은 것으로 봅니다(같은 실패는 1분에 한 번만 기록합니다)"
                );
            }
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

/// 등록 여부만 보는 자리(설정 상태 · 시작 조건). 값은 내주지 않는다.
///
/// 자격 자리([`BrokerKeys`] · [`AiKeys`])가 값을 내주는 것만 등록된 것으로 본다
/// (칸이 비었거나 읽지 못한 키 파일은 등록되지 않은 것이다).
pub(crate) struct KeyPresence {
    store: Arc<SecretStore>,
    /// 읽지 못했다는 경고를 종류마다 묶는다([`KeyKind::ALL`] 순서).
    quiet: [Quiet; KeyKind::ALL.len()],
}

impl KeyPresence {
    pub(crate) fn new(store: Arc<SecretStore>) -> Self {
        Self {
            store,
            quiet: std::array::from_fn(|_| Quiet::default()),
        }
    }

    /// 그 종류가 등록돼 있는가.
    pub(crate) fn has(&self, kind: KeyKind) -> bool {
        let quiet = KeyKind::ALL
            .iter()
            .position(|known| *known == kind)
            .and_then(|at| self.quiet.get(at));
        let Some(quiet) = quiet else {
            return false;
        };
        read_kind(&self.store, kind, quiet).is_some_and(|values| match kind {
            KeyKind::Broker => broker_credentials(&values).is_some(),
            KeyKind::AiApiKey | KeyKind::Data => field(&values, "api_key").is_some(),
        })
    }
}

/// AI 자격 자리: [`KeyKind::AiApiKey`]를 `Credential::ApiKey`로 준다(6차: 구독은 자격이 아니라 CLI다).
///
/// 부를 때마다 보관소를 읽는다. 없으면 `None`(V31).
pub struct AiKeys {
    store: Arc<SecretStore>,
    /// 읽지 못했다는 경고를 묶는다.
    quiet_api_key: Quiet,
}

impl AiKeys {
    /// 보관소에 잇는다.
    pub fn new(store: Arc<SecretStore>) -> Self {
        Self {
            store,
            quiet_api_key: Quiet::default(),
        }
    }
}

#[async_trait]
impl pi_ai::CredentialSource for AiKeys {
    async fn current(&self) -> Option<pi_ai::Credential> {
        read_kind(&self.store, KeyKind::AiApiKey, &self.quiet_api_key)
            .and_then(|values| field(&values, "api_key").map(pi_ai::Secret::new))
            .map(pi_ai::Credential::ApiKey)
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
/// | [`KeyKind::Data`] | 확인할 곳이 아직 없다 | 언제나 실패(저장하지 않는다) |
/// | (8차) `KeySlot::{Dart, Ecos, Fred}` | `pi_data::verify_slot`([`KeyVerifier::with_data`]의 주소) | 주소가 없으면 `Unreachable` |
/// | (8차) `KeySlot::BackupGithubToken` | `pi_backup::verify_token`([`KeyVerifier::with_github`]의 주소) | 주소가 없으면 `Unreachable` |
///
/// 그 밖의 실패(한도 초과 · 장애 등)도 `Ok`가 아니다(확인하지 못한 키는 저장하지 않는다, E4).
pub struct KeyVerifier {
    upstreams: Upstreams,
    /// (8차) 자료 출처 주소. 없으면 자료 칸은 확인하지 않는다(어느 서버로도 보내지 않고 저장하지 않는다).
    data: Option<DataConfig>,
    /// (8차) GitHub API 주소. 없으면 토큰은 확인하지 않는다.
    github_api_base: Option<String>,
}

impl KeyVerifier {
    /// 바깥 서버 설정으로 만든다.
    pub fn new(upstreams: Upstreams) -> Self {
        Self {
            upstreams,
            data: None,
            github_api_base: None,
        }
    }

    /// (8차) 자료 칸(DART · ECOS · FRED)을 이 주소로 확인한다(앱의 자료 설정과 같은 주소).
    pub fn with_data(mut self, data: DataConfig) -> Self {
        self.data = Some(data);
        self
    }

    /// (8차) 백업 GitHub 토큰을 이 API 주소로 확인한다(백업 설정의 주소).
    pub fn with_github(mut self, api_base: impl Into<String>) -> Self {
        self.github_api_base = Some(api_base.into());
        self
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
            // 자료 키는 확인할 곳이 아직 없다. 확인하지 못한 키는 저장하지 않는다(E4).
            // 어느 서버로도 보내지 않는다.
            KeyKind::Data => Err(VerifyFailure::Unreachable),
        }
    }

    async fn verify_slot(&self, slot: KeySlot, values: &Credentials) -> Result<(), VerifyFailure> {
        if slot == KeySlot::BackupGithubToken {
            let api_base = self
                .github_api_base
                .as_deref()
                .ok_or(VerifyFailure::Unreachable)?;
            let token = slot
                .fields()
                .first()
                .and_then(|name| values.get(name))
                .ok_or(VerifyFailure::Rejected)?;
            return pi_backup::verify_token(api_base, token).await;
        }
        let data = self.data.as_ref().ok_or(VerifyFailure::Unreachable)?;
        pi_data::verify_slot(data, slot, values).await
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

/// (W5 2차) 에이전트 실행기의 가리기 자리(`pi_ai::Redactor`)에도 같은 보관소를 끼운다.
impl pi_ai::Redactor for SecretsRedactor {
    fn redact(&self, text: &str) -> String {
        self.store.redact(text)
    }
}
