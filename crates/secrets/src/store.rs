//! 비밀 보관소: 등록 · 읽기 · 상태 보기 · 가리기.

use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError, RwLock};

use crate::redact::Patterns;
use crate::verify::check_fields;
use crate::{
    AUTH_FILE, Credentials, HOME_ENV, KeyKind, SecretsError, Verifier, authfile, envfile, fsio,
};

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

/// 비밀 보관소. 폴더 하나를 맡는다.
///
/// `Send + Sync`다. 서버가 하나를 열어 여러 곳에서 함께 쓴다.
/// `Debug`는 폴더 경로만 찍고 값은 찍지 않는다.
pub struct SecretStore {
    dir: PathBuf,
    /// 파일을 읽어 고쳐 쓰는 동안 잡는다(같은 보관소의 등록이 서로 덮지 않게).
    write_lock: Mutex<()>,
    /// [`SecretStore::redact`]가 가릴 값들: 열 때 읽은 값과 이 보관소가 등록하거나 읽어 준 값.
    patterns: RwLock<Patterns>,
}

/// 키 파일에서 한 종류를 읽는다. 읽기 전에 파일 권한을 좁힌다.
fn load_kind(dir: &Path, kind: KeyKind) -> Result<Option<Credentials>, SecretsError> {
    let file = kind.file_name();
    let Some(text) = fsio::read_private(&dir.join(file), file)? else {
        return Ok(None);
    };
    if file == AUTH_FILE {
        authfile::read_kind(&text, kind)
    } else {
        envfile::read_kind(&text, kind)
    }
}

impl SecretStore {
    /// `dir`을 보관 폴더로 연다.
    ///
    /// - 폴더가 없으면 만든다. 폴더 권한을 0700으로 맞춘다(Unix).
    /// - 이미 있는 키 파일의 권한이 0600보다 넓으면 읽기 전에 0600으로 좁힌다(Unix).
    /// - 이미 등록된 값을 읽어 둔다([`SecretStore::redact`]가 쓴다).
    ///
    /// 키 파일이 깨져 있어도([`SecretsError::Corrupt`]) 여는 것은 된다. 그 종류는
    /// [`SecretStore::read`] · [`SecretStore::status`]가 오류를 내고, 다시 등록하면 고쳐진다.
    pub fn open(dir: impl Into<PathBuf>) -> Result<Self, SecretsError> {
        let dir = dir.into();
        fsio::prepare_dir(&dir)?;
        let mut patterns = Patterns::default();
        for kind in KeyKind::ALL {
            match load_kind(&dir, kind) {
                Ok(Some(values)) => values.values().for_each(|v| patterns.add(v.expose())),
                Ok(None) | Err(SecretsError::Corrupt { .. }) => {}
                Err(other) => return Err(other),
            }
        }
        Ok(Self {
            dir,
            write_lock: Mutex::new(()),
            patterns: RwLock::new(patterns),
        })
    }

    /// 프로세스 환경([`HOME_ENV`], 사용자 홈)으로 [`resolve_dir`](crate::resolve_dir)를 불러 그 폴더를 연다.
    pub fn open_default() -> Result<Self, SecretsError> {
        let env_override = std::env::var_os(HOME_ENV);
        let home = std::env::home_dir();
        Self::open(fsio::resolve_dir(env_override.as_deref(), home.as_deref())?)
    }

    /// 보관 폴더 경로.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// 키를 등록한다 (C3 · C16). 이미 있으면 덮어쓴다.
    ///
    /// 순서:
    /// 1. 칸 검사: 빠진 칸 · 없는 칸 · [`MIN_SECRET_CHARS`](crate::MIN_SECRET_CHARS)보다 짧은 값 ·
    ///    줄바꿈이 든 값은 거부한다. 이때는 확인기를 부르지 않는다.
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
        check_fields(kind, &values)?;
        verifier
            .verify(kind, &values)
            .await
            .map_err(SecretsError::VerifyFailed)?;
        self.save(kind, &values)?;
        self.remember(&values);
        Ok(Registered { kind })
    }

    /// 등록된 값을 읽는다. 등록되지 않았으면 `Ok(None)`.
    ///
    /// `pi-ai` · `pi-market`이 자격을 읽을 때만 쓴다. 화면 · 응답으로 가는 길에서는 부르지 않는다.
    pub fn read(&self, kind: KeyKind) -> Result<Option<Credentials>, SecretsError> {
        let found = load_kind(&self.dir, kind)?;
        if let Some(values) = &found {
            // 파일이 바깥에서 바뀌었더라도, 내준 값은 가릴 대상이 된다.
            self.remember(values);
        }
        Ok(found)
    }

    /// 종류별 등록 여부. [`KeyKind::ALL`] 순서로 하나씩 돌려준다. 값 · 길이는 없다.
    pub fn status(&self) -> Result<Vec<KeyStatus>, SecretsError> {
        KeyKind::ALL
            .into_iter()
            .map(|kind| {
                let registered = load_kind(&self.dir, kind)?.is_some();
                Ok(KeyStatus { kind, registered })
            })
            .collect()
    }

    /// 글에서 등록된 비밀 값을 [`REDACTED`](crate::REDACTED)로 바꾼다 (E38 · N8).
    ///
    /// 값이 그대로 든 경우, URL 인코딩(`%XX`, 16진수 대소문자 모두)된 경우,
    /// Base64(표준 · URL 안전, 채움 있음 · 없음)로 든 경우를 모두 가린다.
    /// 비밀 값이 없는 글은 그대로 돌려준다. 이 보관소의 모든 종류 · 모든 칸이 대상이다.
    pub fn redact(&self, text: &str) -> String {
        self.patterns
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .redact(text)
    }

    /// 한 종류를 키 파일에 바꿔 쓴다. 같은 파일의 다른 내용은 그대로 둔다.
    fn save(&self, kind: KeyKind, values: &Credentials) -> Result<(), SecretsError> {
        let _guard = self
            .write_lock
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        fsio::prepare_dir(&self.dir)?;
        let file = kind.file_name();
        let path = self.dir.join(file);
        let new_text = if file == AUTH_FILE {
            // 깨진 auth.json은 새로 쓴다(다시 등록이 되살리는 길이다).
            let old = match fsio::read_private(&path, file) {
                Ok(old) => old,
                Err(SecretsError::Corrupt { .. }) => None,
                Err(other) => return Err(other),
            };
            authfile::replace_kind(old.as_ref().map(|text| text.as_str()), kind, values)
        } else {
            let old = fsio::read_private(&path, file)?;
            envfile::replace_kind(old.as_ref().map_or("", |text| text.as_str()), kind, values)
        };
        fsio::write_atomic(&self.dir, file, new_text.as_bytes())
    }

    /// 값들을 가릴 대상에 더한다.
    fn remember(&self, values: &Credentials) {
        let mut patterns = self
            .patterns
            .write()
            .unwrap_or_else(PoisonError::into_inner);
        values.values().for_each(|v| patterns.add(v.expose()));
    }
}

impl fmt::Debug for SecretStore {
    /// 폴더 경로만 찍는다. 값은 찍지 않는다.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SecretStore")
            .field("dir", &self.dir)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::{ENV_FILE, VerifyFailure};

    struct Pass;

    impl Verifier for Pass {
        async fn verify(&self, _kind: KeyKind, _values: &Credentials) -> Result<(), VerifyFailure> {
            Ok(())
        }
    }

    fn oauth() -> Credentials {
        Credentials::new()
            .with("access_token", "시험용-접근-토큰-0009")
            .with("refresh_token", "시험용-갱신-토큰-0009")
    }

    #[tokio::test]
    async fn 깨진_auth_json은_읽기는_오류고_다시_등록하면_고쳐진다() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join(AUTH_FILE), "{\"ai_oauth\": {\"잘린").unwrap();

        let store = SecretStore::open(tmp.path()).unwrap();
        assert!(matches!(
            store.read(KeyKind::AiOAuth),
            Err(SecretsError::Corrupt { file: AUTH_FILE })
        ));
        assert!(matches!(store.status(), Err(SecretsError::Corrupt { .. })));
        // 다른 파일의 종류는 읽힌다.
        assert!(store.read(KeyKind::Broker).unwrap().is_none());

        store
            .register(KeyKind::AiOAuth, oauth(), &Pass)
            .await
            .unwrap();
        assert!(store.read(KeyKind::AiOAuth).unwrap().is_some());
        assert!(store.status().is_ok());
    }

    #[tokio::test]
    async fn 손으로_넣은_줄과_설명은_등록_뒤에도_남는다() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join(ENV_FILE), "# 내 설명\nMY_OTHER=keep\n").unwrap();
        let store = SecretStore::open(tmp.path()).unwrap();

        let values = Credentials::new().with("api_key", "시험용-자료-값-0009");
        store.register(KeyKind::Data, values, &Pass).await.unwrap();

        let text = fs::read_to_string(tmp.path().join(ENV_FILE)).unwrap();
        assert!(text.starts_with("# 내 설명\nMY_OTHER=keep\n"));
        assert!(store.read(KeyKind::Data).unwrap().is_some());
    }

    #[tokio::test]
    async fn 덮어쓴_옛_값도_이_보관소에서는_계속_가린다() {
        let tmp = tempfile::tempdir().unwrap();
        let store = SecretStore::open(tmp.path()).unwrap();
        for value in ["시험용-자료-값-0008", "시험용-자료-값-0009"] {
            let values = Credentials::new().with("api_key", value);
            store.register(KeyKind::Data, values, &Pass).await.unwrap();
        }
        let out = store.redact("옛 시험용-자료-값-0008 새 시험용-자료-값-0009");
        assert_eq!(out, format!("옛 {0} 새 {0}", crate::REDACTED));
    }
}
