//! pi-secrets: 비밀 보관 (구성 요소 A13). `docs/dev/modules/secrets.md`를 따른다.
//!
//! 증권사 · AI(API 키) · 자료 키를 홈 폴더 아래 파일에 본인만 읽는 권한으로 두고(D3),
//! 등록할 때 확인기로 실제로 닿는지 본 뒤에만 저장하며(C3 · C16 · E4),
//! 바깥으로 나가는 글에서 등록된 값과 확인 중인 값을 가린다(E38 · N8).
//! 가리기 대상은 칸마다 개수에 끝이 있다(N1, [`REDACT_PREVIOUS_KEPT`]).
//! 보관 폴더는 절대 경로만 받는다(현재 폴더 아래에 키를 쓰지 않는다).
//! 구독 토큰은 받지도 쥐지도 않는다(D4: 구독은 공식 Claude Code CLI에 맡긴다). 예전에 쓰던
//! `auth.json`이 보관 폴더에 남아 있어도 읽지 않고 · 등록된 것으로 보지 않고 · 고치지 않는다.
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

mod envfile;
mod error;
mod fsio;
mod kind;
mod legacy;
mod redact;
mod secret;
mod slot;
mod store;
mod targets;
mod verify;

pub use error::{SecretsError, VerifyFailure};
pub use fsio::resolve_dir;
pub use kind::KeyKind;
pub use legacy::legacy_auth_file_present;
pub use secret::{Credentials, SecretString};
pub use slot::{KeySlot, SlotRegistered, SlotStatus};
pub use store::{KeyStatus, Registered, SecretStore};
pub use verify::Verifier;

/// 보관 폴더를 바꾸는 환경 변수 이름.
pub const HOME_ENV: &str = "PRICE_INHA_HOME";

/// 홈 폴더 아래 기본 보관 폴더 이름.
pub const DEFAULT_DIR_NAME: &str = ".price-inha";

/// API 키 파일 이름. `이름=값` 줄로 쓴다. 권한 0600.
pub const ENV_FILE: &str = ".env";

/// 예전(1 · 2차)에 구독 토큰을 두던 파일 이름. 3차부터 쓰지도 읽지도 않는다.
///
/// 이 이름은 이 모듈만 안다(`ARCH-SECRET-FILES`). 화면이 "옛 파일이 남아 있다"고 알릴 때는
/// [`legacy_auth_file_present`]로 있는지만 본다.
pub const LEGACY_AUTH_FILE: &str = "auth.json";

/// 백업이 대상에서 빼야 하는 파일 이름들 (E39 · N8, 결정 2026-11-16 23 · 24번).
///
/// 이 모듈이 키를 두는 파일([`ENV_FILE`]: 모든 키 · 6차 칸 · VAPID 비밀키)과
/// 옛 구독 토큰 파일([`LEGACY_AUTH_FILE`])이다. 백업기는 파일 이름을 직접 쓰지 않고 이것을 쓴다
/// (`ARCH-SECRET-FILES`).
pub const BACKUP_EXCLUDED_FILES: &[&str] = &[ENV_FILE, LEGACY_AUTH_FILE];

/// 비밀 값 자리에 대신 들어가는 글자.
pub const REDACTED: &str = "[가려짐]";

/// 등록할 수 있는 비밀 값의 최소 글자 수(`char` 기준). 이보다 짧으면 가릴 수 없어 거부한다.
/// 키 파일에 손으로 적힌 값이 이보다 짧으면 읽을 때도 등록되지 않은 것으로 본다.
pub const MIN_SECRET_CHARS: usize = 8;

/// 칸마다 가리기 대상에 남겨 두는 **바로 앞 값**의 개수 (N1).
///
/// 같은 칸을 서로 다른 값으로 다시 등록하면, [`SecretStore::redact`]는 그 칸의 지금 값과
/// 바로 앞의 값 이 개수만큼만 가린다. 그보다 오래된 값은 가리기 대상에서 빠진다
/// (오래 돌려도 가리기 대상이 끝없이 늘지 않게). 지금 등록된 값과 확인 중인 값은 이 개수와
/// 상관없이 언제나 가려진다.
///
/// 확인이나 저장에 실패한 값도 칸마다 새것부터 이 개수까지만 쥔다. 그래서 한 칸의 가리기 대상은
/// 지금 값 1개 + 앞 값 이 개수 + 실패한 값 이 개수와, 그때 확인 중인 값을 넘지 않는다.
pub const REDACT_PREVIOUS_KEPT: usize = 3;
