//! pi-secrets: 비밀 보관 (구성 요소 A13). `docs/dev/modules/secrets.md`를 따른다.
//!
//! 증권사 · AI · 자료 키를 홈 폴더 아래 파일에 본인만 읽는 권한으로 두고(D3),
//! 등록할 때 확인기로 실제로 닿는지 본 뒤에만 저장하며(C3 · C16 · E4),
//! 바깥으로 나가는 글에서 등록된 값을 가린다(E38 · N8).
//!
//! # 값이 새지 않게 하는 모양
//!
//! - 비밀 값은 [`SecretString`]에만 담긴다. `Debug` · `Display`는 [`REDACTED`]만 찍고,
//!   `Serialize`가 없으며, 꺼내는 길은 [`SecretString::expose`] 하나다.
//! - [`SecretStore::register`]는 값이 없는 [`Registered`]만 돌려준다.
//! - [`SecretStore::status`]는 종류별 등록 여부만 알려 준다. 값 · 값의 일부 · 길이는 없다.
//! - [`SecretsError`]와 [`VerifyFailure`]는 자유 글자를 담지 않는다. 칸 이름은 `&'static str`이다.
//!
//! # 쓰는 법 (`pi-ai`, `pi-market`)
//!
//! ```no_run
//! use pi_secrets::{Credentials, KeyKind, SecretStore, Verifier, VerifyFailure};
//!
//! struct BrokerVerifier;
//!
//! impl Verifier for BrokerVerifier {
//!     async fn verify(&self, _kind: KeyKind, values: &Credentials) -> Result<(), VerifyFailure> {
//!         let _secret = values.get("client_secret").ok_or(VerifyFailure::Rejected)?.expose();
//!         // 이 키로 실제 서버에 닿는지 본다. 닿지 않으면 Err.
//!         Ok(())
//!     }
//! }
//!
//! # async fn run() -> Result<(), pi_secrets::SecretsError> {
//! let store = SecretStore::open_default()?;
//! let values = Credentials::new()
//!     .with("client_id", "...")
//!     .with("client_secret", "...");
//! store.register(KeyKind::Broker, values, &BrokerVerifier).await?;
//!
//! if let Some(found) = store.read(KeyKind::Broker)? {
//!     let _id = found.get("client_id").map(|v| v.expose());
//! }
//! let safe = store.redact("바깥으로 나갈 글");
//! # let _ = safe;
//! # Ok(())
//! # }
//! ```

mod authfile;
mod envfile;
mod error;
mod fsio;
mod kind;
mod redact;
mod secret;
mod store;
mod verify;

pub use error::{SecretsError, VerifyFailure};
pub use fsio::resolve_dir;
pub use kind::KeyKind;
pub use secret::{Credentials, SecretString};
pub use store::{KeyStatus, Registered, SecretStore};
pub use verify::Verifier;

/// 보관 폴더를 바꾸는 환경 변수 이름.
pub const HOME_ENV: &str = "PRICE_INHA_HOME";

/// 홈 폴더 아래 기본 보관 폴더 이름.
pub const DEFAULT_DIR_NAME: &str = ".price-inha";

/// API 키 파일 이름. `이름=값` 줄로 쓴다. 권한 0600.
pub const ENV_FILE: &str = ".env";

/// 구독 OAuth 토큰 파일 이름. 권한 0600.
pub const AUTH_FILE: &str = "auth.json";

/// 비밀 값 자리에 대신 들어가는 글자.
pub const REDACTED: &str = "[가려짐]";

/// 등록할 수 있는 비밀 값의 최소 글자 수(`char` 기준). 이보다 짧으면 가릴 수 없어 거부한다.
pub const MIN_SECRET_CHARS: usize = 8;
