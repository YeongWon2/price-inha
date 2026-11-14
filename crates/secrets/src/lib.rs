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

use std::ffi::OsStr;
use std::fmt;
use std::future::Future;
use std::path::{Path, PathBuf};

use zeroize::{Zeroize, ZeroizeOnDrop};

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
    /// 모든 종류. [`SecretStore::status`]가 이 순서로 돌려준다.
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
}

/// 비밀 값 하나를 담는 타입.
///
/// - `Debug` · `Display`는 [`REDACTED`]만 찍는다. 값 · 값의 일부 · 길이가 나오지 않는다.
/// - `Serialize`를 구현하지 않는다(응답 · 기록에 실수로 실리지 않게).
/// - 꺼내는 길은 [`SecretString::expose`] 하나다.
/// - 버려질 때 메모리를 지운다.
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct SecretString {
    inner: String,
}

impl SecretString {
    /// 값을 감싼다. 길이 · 글자 검사는 [`SecretStore::register`]가 한다.
    pub fn new(value: impl Into<String>) -> Self {
        let _ = value;
        todo!()
    }

    /// 값을 꺼낸다. 실제 서버에 보내는 자리에서만 쓴다.
    /// 꺼낸 글자를 기록 · 오류 · 응답에 넣지 않는다.
    pub fn expose(&self) -> &str {
        let _ = &self.inner;
        todo!()
    }
}

impl fmt::Debug for SecretString {
    /// [`REDACTED`]만 찍는다.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let _ = f;
        todo!()
    }
}

impl fmt::Display for SecretString {
    /// [`REDACTED`]만 찍는다.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let _ = f;
        todo!()
    }
}

/// 한 종류의 칸 이름 → 비밀 값 묶음. 등록할 때 넘기고, 읽을 때 돌려받는다.
///
/// `Debug`는 칸 이름과 [`REDACTED`]만 찍는다. `Serialize`를 구현하지 않는다.
#[derive(Clone, Default)]
pub struct Credentials {
    entries: Vec<(String, SecretString)>,
}

impl Credentials {
    /// 빈 묶음.
    pub fn new() -> Self {
        todo!()
    }

    /// 칸 하나를 더한다. 같은 칸을 다시 주면 나중 값이 남는다.
    /// 칸 이름이 그 종류에 맞는지는 [`SecretStore::register`]가 본다.
    #[must_use]
    pub fn with(self, field: &str, value: impl Into<String>) -> Self {
        let _ = (field, value);
        todo!()
    }

    /// 칸의 값. 없는 칸이면 `None`.
    pub fn get(&self, field: &str) -> Option<&SecretString> {
        let _ = (field, &self.entries);
        todo!()
    }

    /// 들어 있는 칸 이름들(넣은 순서).
    pub fn field_names(&self) -> Vec<&str> {
        todo!()
    }
}

impl fmt::Debug for Credentials {
    /// 칸 이름과 [`REDACTED`]만 찍는다.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let _ = f;
        todo!()
    }
}

/// 확인기가 내는 실패. 자유 글자를 담지 않는다(오류 글에 키가 섞이지 않게).
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum VerifyFailure {
    /// 서버에 닿았지만 키가 맞지 않다.
    #[error("키가 맞지 않습니다")]
    Rejected,
    /// 서버에 닿지 못했다(연결 · 시간 초과 등).
    #[error("확인할 서버에 닿지 못했습니다")]
    Unreachable,
}

/// 그 키로 실제로 닿는지 확인한다 (C3 · E4).
///
/// 실제 증권사 · AI에 닿는 확인기는 `pi-market` · `pi-ai`가 구현한다. 이 모듈에는 trait만 있다.
/// `async fn verify(&self, kind, values) -> Result<(), VerifyFailure>`로 구현하면 된다.
pub trait Verifier: Send + Sync {
    /// `Ok(())`면 저장한다. `Err`면 저장하지 않는다.
    ///
    /// `values`는 칸 검사(빠진 칸 · 짧은 값 · 못 쓰는 글자)를 지난 것만 온다.
    fn verify(
        &self,
        kind: KeyKind,
        values: &Credentials,
    ) -> impl Future<Output = Result<(), VerifyFailure>> + Send;
}

/// 등록 성공 표시. 값은 없다 (C3: "키 값은 돌려주지 않는다").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Registered {
    /// 등록된 종류.
    pub kind: KeyKind,
}

/// 한 종류의 등록 상태. 값 · 값의 일부 · 길이는 없다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyStatus {
    /// 종류.
    pub kind: KeyKind,
    /// 등록됐는가.
    pub registered: bool,
}

/// 이 모듈의 오류. 어떤 변형도 비밀 값을 담지 않는다.
/// 칸 이름은 [`KeyKind::fields`]의 `&'static str`만 쓴다(호출자가 준 글자를 되돌려 싣지 않는다).
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum SecretsError {
    /// E4 키 확인 실패. 저장하지 않았다.
    #[error("키 확인에 실패해 저장하지 않았습니다: {0}")]
    VerifyFailed(#[source] VerifyFailure),
    /// 그 종류에 있어야 할 칸이 없다.
    #[error("칸이 빠졌습니다: {field}")]
    MissingField {
        /// 빠진 칸.
        field: &'static str,
    },
    /// 그 종류에 없는 칸이 들어왔다. 들어온 이름은 싣지 않는다.
    #[error("이 키 종류에 없는 칸이 들어왔습니다")]
    UnknownField,
    /// 값이 [`MIN_SECRET_CHARS`]보다 짧다(가릴 수 없다).
    #[error("값이 너무 짧습니다(최소 {min}자): {field}")]
    TooShort {
        /// 짧은 값이 든 칸.
        field: &'static str,
        /// 최소 글자 수.
        min: usize,
    },
    /// 값에 파일에 쓸 수 없는 글자(줄바꿈 등)가 있다.
    #[error("값에 쓸 수 없는 글자가 있습니다: {field}")]
    InvalidValue {
        /// 문제의 칸.
        field: &'static str,
    },
    /// 홈 폴더를 알 수 없고 [`HOME_ENV`]도 없다.
    #[error("보관 폴더를 정할 수 없습니다")]
    HomeNotFound,
    /// 키 파일 내용을 읽을 수 없다. 파일 내용은 싣지 않는다.
    #[error("키 파일을 읽을 수 없는 모양입니다: {file}")]
    Corrupt {
        /// 파일 이름([`ENV_FILE`] 또는 [`AUTH_FILE`]).
        file: &'static str,
    },
    /// 파일 · 폴더 작업 실패. 종류만 싣는다.
    #[error("파일 작업에 실패했습니다({op}): {kind}")]
    Io {
        /// 하던 일(예: "쓰기").
        op: &'static str,
        /// 실패 종류.
        kind: std::io::ErrorKind,
    },
}

/// 보관 폴더 경로를 정한다. 환경을 직접 읽지 않고 받은 값으로만 정한다(테스트가 환경을 바꾸지 않게).
///
/// - `env_override`: [`HOME_ENV`]의 값. 있고 비어 있지 않으면 그 경로가 보관 폴더다.
/// - `home_dir`: 사용자 홈 폴더. `env_override`가 없거나 비었으면 `home_dir/`[`DEFAULT_DIR_NAME`].
/// - 둘 다 없으면 [`SecretsError::HomeNotFound`].
pub fn resolve_dir(
    env_override: Option<&OsStr>,
    home_dir: Option<&Path>,
) -> Result<PathBuf, SecretsError> {
    let _ = (env_override, home_dir);
    todo!()
}

/// 비밀 보관소. 폴더 하나를 맡는다.
///
/// `Send + Sync`다. 서버가 하나를 열어 여러 곳에서 함께 쓴다.
/// `Debug`는 폴더 경로만 찍고 값은 찍지 않는다.
pub struct SecretStore {
    dir: PathBuf,
}

impl SecretStore {
    /// `dir`을 보관 폴더로 연다.
    ///
    /// - 폴더가 없으면 만든다. 폴더 권한을 0700으로 맞춘다(Unix).
    /// - 이미 있는 키 파일의 권한이 0600보다 넓으면 읽기 전에 0600으로 좁힌다(Unix).
    /// - 이미 등록된 값을 읽어 둔다([`SecretStore::redact`]가 쓴다).
    pub fn open(dir: impl Into<PathBuf>) -> Result<Self, SecretsError> {
        let _ = dir;
        todo!()
    }

    /// 프로세스 환경([`HOME_ENV`], 사용자 홈)으로 [`resolve_dir`]를 불러 그 폴더를 연다.
    pub fn open_default() -> Result<Self, SecretsError> {
        todo!()
    }

    /// 보관 폴더 경로.
    pub fn dir(&self) -> &Path {
        let _ = &self.dir;
        todo!()
    }

    /// 키를 등록한다 (C3 · C16). 이미 있으면 덮어쓴다.
    ///
    /// 순서:
    /// 1. 칸 검사: 빠진 칸 · 없는 칸 · [`MIN_SECRET_CHARS`]보다 짧은 값 · 줄바꿈이 든 값은 거부한다.
    ///    이때는 확인기를 부르지 않는다.
    /// 2. `verifier`로 확인한다. 실패하면 **저장하지 않고** [`SecretsError::VerifyFailed`] (E4).
    /// 3. 같은 폴더의 임시 파일(처음부터 0600)에 쓰고 이름을 바꾼다(원자 쓰기). 임시 파일은 남지 않는다.
    ///
    /// 성공하면 값이 없는 [`Registered`]만 돌려준다. 같은 파일의 다른 종류는 그대로 둔다.
    pub async fn register<V: Verifier + ?Sized>(
        &self,
        kind: KeyKind,
        values: Credentials,
        verifier: &V,
    ) -> Result<Registered, SecretsError> {
        let _ = (kind, values, verifier);
        todo!()
    }

    /// 등록된 값을 읽는다. 등록되지 않았으면 `Ok(None)`.
    ///
    /// `pi-ai` · `pi-market`이 자격을 읽을 때만 쓴다. 화면 · 응답으로 가는 길에서는 부르지 않는다.
    pub fn read(&self, kind: KeyKind) -> Result<Option<Credentials>, SecretsError> {
        let _ = kind;
        todo!()
    }

    /// 종류별 등록 여부. [`KeyKind::ALL`] 순서로 하나씩 돌려준다. 값 · 길이는 없다.
    pub fn status(&self) -> Result<Vec<KeyStatus>, SecretsError> {
        todo!()
    }

    /// 글에서 등록된 비밀 값을 [`REDACTED`]로 바꾼다 (E38 · N8).
    ///
    /// 값이 그대로 든 경우, URL 인코딩(`%XX`, 16진수 대소문자 모두)된 경우,
    /// Base64(표준 · URL 안전, 채움 있음 · 없음)로 든 경우를 모두 가린다.
    /// 비밀 값이 없는 글은 그대로 돌려준다. 이 보관소의 모든 종류 · 모든 칸이 대상이다.
    pub fn redact(&self, text: &str) -> String {
        let _ = text;
        todo!()
    }
}

impl fmt::Debug for SecretStore {
    /// 폴더 경로만 찍는다. 값은 찍지 않는다.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let _ = f;
        todo!()
    }
}
