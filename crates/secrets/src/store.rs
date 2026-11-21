//! 비밀 보관소: 등록 · 읽기 · 상태 보기 · 가리기.

use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError, RwLock};

use crate::kind::Entry;
use crate::provider::KEY_LINE_PREFIX;
use crate::targets::{Outcome, Targets};
use crate::verify::check_fields;
use crate::{
    Credentials, ENV_FILE, HOME_ENV, KeyKind, MIN_SECRET_CHARS, SecretsError, Verifier, envfile,
    fsio,
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
    /// 키 파일을 읽어 고쳐 쓰는 동안, 그리고 읽은 값을 가리기 대상에 올리는 동안 잡는다
    /// (같은 보관소의 등록이 서로 덮지 않고, 가리기 대상의 "지금 값"이 파일과 어긋나지 않게).
    write_lock: Mutex<()>,
    /// [`SecretStore::redact`]가 가릴 값들: 열 때 읽은 값, 이 보관소가 등록하거나 읽어 준 값,
    /// 확인 중인 값, 확인 · 저장에 실패한 값. 칸마다 개수에 끝이 있다([`crate::targets`]).
    targets: RwLock<Targets>,
}

/// 키 파일에서 한 묶음(종류 · 칸 · VAPID)을 읽는다. 읽기 전에 파일 권한을 좁힌다.
///
/// 칸 하나라도 [`MIN_SECRET_CHARS`]보다 짧으면(손으로 고친 파일) 등록되지 않은 것으로 본다(`Ok(None)`):
/// 가릴 수 없는 값을 내주지 않는다.
fn load_kind(dir: &Path, entry: impl Into<Entry>) -> Result<Option<Credentials>, SecretsError> {
    Ok(load_kind_raw(dir, entry)?.filter(|values| {
        values
            .values()
            .all(|value| value.expose().chars().count() >= MIN_SECRET_CHARS)
    }))
}

/// [`load_kind`]에서 짧은 값 거르기만 뺀 것: 칸이 모두 채워져 있으면 짧아도 돌려준다.
fn load_kind_raw(dir: &Path, entry: impl Into<Entry>) -> Result<Option<Credentials>, SecretsError> {
    let kind = entry.into();
    let file = kind.file_name();
    let Some(text) = fsio::read_private(&dir.join(file), file)? else {
        return Ok(None);
    };
    envfile::read_kind(&text, kind)
}

/// 확인 중인 값. 만들 때 가리기 대상에 넣고, 버려질 때 결과에 맞는 자리로 옮긴다.
///
/// 저장까지 마친 것만 [`Pending::saved`]로 끝낸다. 그 밖의 길(확인 실패 · 저장 실패 ·
/// 등록이 도중에 버려짐)은 모두 실패한 값으로 남아 계속 가려진다.
pub(crate) struct Pending<'a> {
    store: &'a SecretStore,
    kind: Entry,
    values: Credentials,
    outcome: Outcome,
}

impl<'a> Pending<'a> {
    fn begin(store: &'a SecretStore, kind: Entry, values: Credentials) -> Self {
        store.targets_mut(|targets| targets.begin(kind, &values));
        Self {
            store,
            kind,
            values,
            outcome: Outcome::Failed,
        }
    }

    /// 확인 중인 값.
    pub(crate) fn values(&self) -> &Credentials {
        &self.values
    }

    /// 저장을 마쳤다: 값을 그 칸의 지금 값으로 올린다.
    fn saved(mut self) {
        self.outcome = Outcome::Saved;
    }
}

impl Drop for Pending<'_> {
    fn drop(&mut self) {
        let (kind, outcome) = (self.kind, self.outcome);
        self.store
            .targets_mut(|targets| targets.settle(kind, &self.values, outcome));
    }
}

impl SecretStore {
    /// `dir`을 보관 폴더로 연다.
    ///
    /// - 폴더가 없으면 만든다. 폴더 권한을 0700으로 맞춘다(Unix).
    /// - 이미 있는 키 파일의 권한이 0600보다 넓으면 읽기 전에 0600으로 좁힌다(Unix).
    /// - 이미 등록된 값을 읽어 둔다([`SecretStore::redact`]가 쓴다).
    /// - `dir`은 절대 경로여야 한다. 빈 글자나 상대 경로면 아무것도 만들지 않고
    ///   [`SecretsError::HomeNotFound`]로 거부한다(현재 폴더 아래에 키를 쓰지 않는다).
    ///
    /// 키 파일이 깨져 있어도([`SecretsError::Corrupt`]) 여는 것은 된다. 그 종류는
    /// [`SecretStore::read`] · [`SecretStore::status`]가 오류를 내고, 다시 등록하면 고쳐진다.
    pub fn open(dir: impl Into<PathBuf>) -> Result<Self, SecretsError> {
        let dir = dir.into();
        fsio::require_absolute(&dir)?;
        // 3차 2번: 예전 구독 토큰 파일(auth.json)은 보지 않는다. 키 파일만 읽으므로
        // 그 파일은 읽히지도 · 등록된 것으로 보이지도 · 고쳐지지도 않고, 깨져 있어도 상관없다.
        fsio::prepare_dir(&dir)?;
        let mut targets = Targets::default();
        for entry in Entry::all() {
            match load_kind(&dir, entry) {
                Ok(Some(values)) => targets.set_current(entry, &values),
                Ok(None) | Err(SecretsError::Corrupt { .. }) => {}
                Err(other) => return Err(other),
            }
        }
        // 8차: 공급자 키 줄은 어떤 공급자가 선언했는지 몰라도 모두 가린다(모르면 가린다).
        if let Some(text) = fsio::read_private(&dir.join(ENV_FILE), ENV_FILE)? {
            for (name, value) in envfile::read_prefixed(&text, KEY_LINE_PREFIX) {
                if value.chars().count() >= MIN_SECRET_CHARS {
                    targets.set_current_line(name, &value);
                }
            }
        }
        Ok(Self {
            dir,
            write_lock: Mutex::new(()),
            targets: RwLock::new(targets),
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
    /// 2. 값을 가리기 대상에 넣는다. 확인기가 도는 동안에도 [`SecretStore::redact`]가 이 값을 가린다.
    /// 3. `verifier`로 확인한다. 실패하면 **저장하지 않고** [`SecretsError::VerifyFailed`] (E4).
    /// 4. 같은 폴더의 임시 파일(처음부터 0600)에 쓰고 이름을 바꾼다(원자 쓰기). 임시 파일은 남지 않는다.
    ///
    /// 성공하면 값이 없는 [`Registered`]만 돌려준다. 같은 파일의 다른 종류는 그대로 둔다.
    ///
    /// 확인이나 저장에 실패한 값, 등록이 도중에 버려진 값도 이 보관소에서는 계속 가려진다(N8).
    /// 다만 끝없이 쥐지는 않는다: 실패한 값은 칸마다 새것부터
    /// [`REDACT_PREVIOUS_KEPT`](crate::REDACT_PREVIOUS_KEPT)개까지만 남는다.
    pub async fn register<V: Verifier + ?Sized>(
        &self,
        kind: KeyKind,
        values: Credentials,
        verifier: &V,
    ) -> Result<Registered, SecretsError> {
        let pending = self.begin_entry(kind.into(), values)?;
        verifier
            .verify(kind, pending.values())
            .await
            .map_err(SecretsError::VerifyFailed)?;
        self.finish_entry(pending)?;
        Ok(Registered { kind })
    }

    /// 등록 앞부분: 칸 검사를 하고, 지나면 값을 가리기 대상에 넣는다(확인기를 부르기 전에).
    pub(crate) fn begin_entry(
        &self,
        entry: Entry,
        values: Credentials,
    ) -> Result<Pending<'_>, SecretsError> {
        check_fields(entry, &values)?;
        Ok(Pending::begin(self, entry, values))
    }

    /// 등록 뒷부분: 확인을 지난 값을 원자 쓰기로 저장하고 그 칸의 지금 값으로 올린다.
    pub(crate) fn finish_entry(&self, pending: Pending<'_>) -> Result<(), SecretsError> {
        // 저장과 "지금 값" 올리기를 한 잠금 안에서 한다(함께 도는 등록 · 읽기와 순서가 어긋나지 않게).
        let _guard = self.lock_files();
        self.finish_entry_locked(pending)
    }

    /// [`SecretStore::finish_entry`]와 같다. [`SecretStore::lock_files`]를 잡고 부른다.
    pub(crate) fn finish_entry_locked(&self, pending: Pending<'_>) -> Result<(), SecretsError> {
        self.save(pending.kind, &pending.values)?;
        pending.saved();
        Ok(())
    }

    /// 등록된 값을 읽는다. 등록되지 않았으면 `Ok(None)`.
    ///
    /// 키 파일에 손으로 적힌 값이 [`MIN_SECRET_CHARS`]보다 짧으면(칸 하나라도) 등록되지 않은 것으로
    /// 본다: 가릴 수 없는 값은 내주지 않는다. [`SecretStore::status`]도 같다.
    ///
    /// `pi-ai` · `pi-market`이 자격을 읽을 때만 쓴다. 화면 · 응답으로 가는 길에서는 부르지 않는다.
    pub fn read(&self, kind: KeyKind) -> Result<Option<Credentials>, SecretsError> {
        let _guard = self.lock_files();
        self.read_entry_locked(kind.into())
    }

    /// 묶음 하나를 읽고, 내준 값을 지금 값으로 가리기 대상에 올린다.
    /// [`SecretStore::lock_files`]를 잡고 부른다.
    pub(crate) fn read_entry_locked(
        &self,
        kind: Entry,
    ) -> Result<Option<Credentials>, SecretsError> {
        let found = load_kind(&self.dir, kind)?;
        if let Some(values) = &found {
            // 파일이 바깥에서 바뀌었더라도, 내준 값은 그 칸의 지금 값으로 가릴 대상이 된다.
            self.targets_mut(|targets| targets.set_current(kind, values));
        }
        Ok(found)
    }

    /// 키 파일에 그 묶음의 값이 있지만 [`SecretStore::read_entry_locked`]가 내주지 않는가
    /// (손으로 짧게 고친 값). 있는 값을 "없음"으로 보고 덮어쓰지 않으려고 쓴다.
    /// [`SecretStore::lock_files`]를 잡고 부른다.
    pub(crate) fn has_unusable_entry_locked(&self, kind: Entry) -> Result<bool, SecretsError> {
        Ok(load_kind_raw(&self.dir, kind)?.is_some())
    }

    /// 종류별 등록 여부. [`KeyKind::ALL`] 순서로 하나씩 돌려준다. 값 · 길이는 없다.
    ///
    /// [`SecretStore::read`]가 값을 내주는 종류만 등록된 것이다(짧은 값이 든 종류는 `registered: false`).
    pub fn status(&self) -> Result<Vec<KeyStatus>, SecretsError> {
        KeyKind::ALL
            .into_iter()
            .map(|kind| {
                let registered = self.is_registered(kind.into())?;
                Ok(KeyStatus { kind, registered })
            })
            .collect()
    }

    /// 글에서 등록된 비밀 값을 [`REDACTED`](crate::REDACTED)로 바꾼다 (E38 · N8).
    ///
    /// 값이 그대로 든 경우, URL 인코딩(`%XX`, 16진수 대소문자 모두)된 경우,
    /// Base64(표준 · URL 안전, 채움 있음 · 없음)로 든 경우를 모두 가린다.
    /// 비밀 값이 없는 글은 그대로 돌려준다. 이 보관소의 모든 종류 · 모든 칸이 대상이다.
    ///
    /// 칸마다 가리는 값:
    /// - 지금 등록된 값(이 보관소가 저장했거나 키 파일에서 읽은 값)과 확인 중인 값: 언제나.
    /// - 지금 값 바로 앞의 값: 새것부터 [`REDACT_PREVIOUS_KEPT`](crate::REDACT_PREVIOUS_KEPT)개.
    /// - 확인 · 저장에 실패한 값: 새것부터 같은 개수.
    ///
    /// 그보다 오래된 값은 가리기 대상에서 빠진다(N1: 끝없이 늘지 않는다).
    pub fn redact(&self, text: &str) -> String {
        self.targets
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .redact(text)
    }

    /// 묶음 하나가 등록됐는가([`SecretStore::read`]가 값을 내주는가).
    pub(crate) fn is_registered(&self, entry: Entry) -> Result<bool, SecretsError> {
        Ok(load_kind(&self.dir, entry)?.is_some())
    }

    /// 바이트 안에 가리기 대상 값이 든가.
    pub(crate) fn targets_found_in(&self, bytes: &[u8]) -> bool {
        self.targets
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .found_in(bytes)
    }

    /// 키 파일 잠금을 잡는다.
    pub(crate) fn lock_files(&self) -> MutexGuard<'_, ()> {
        self.write_lock
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    /// 가리기 대상을 고친다.
    fn targets_mut(&self, apply: impl FnOnce(&mut Targets)) {
        apply(&mut self.targets.write().unwrap_or_else(PoisonError::into_inner));
    }

    /// 한 종류를 키 파일에 바꿔 쓴다. 같은 파일의 다른 내용은 그대로 둔다.
    /// [`SecretStore::lock_files`]를 잡고 부른다.
    fn save(&self, kind: Entry, values: &Credentials) -> Result<(), SecretsError> {
        fsio::prepare_dir(&self.dir)?;
        let file = kind.file_name();
        let path = self.dir.join(file);
        let old = fsio::read_private(&path, file)?;
        let new_text =
            envfile::replace_kind(old.as_ref().map_or("", |text| text.as_str()), kind, values);
        fsio::write_atomic(&self.dir, file, new_text.as_bytes())
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

    /// 끝나지 않는 확인기.
    struct Never;

    impl Verifier for Never {
        async fn verify(&self, _kind: KeyKind, _values: &Credentials) -> Result<(), VerifyFailure> {
            std::future::pending().await
        }
    }

    #[tokio::test]
    async fn 확인_도중에_버려진_등록의_값도_계속_가린다() {
        let tmp = tempfile::tempdir().unwrap();
        let store = SecretStore::open(tmp.path()).unwrap();
        let value = "시험용-자료-값-0007";
        let text = format!("앞 {value} 뒤");
        let hidden = format!("앞 {} 뒤", crate::REDACTED);

        let finished = tokio::select! {
            biased;
            _ = store.register(KeyKind::Data, Credentials::new().with("api_key", value), &Never) => true,
            () = async { assert_eq!(store.redact(&text), hidden, "확인 중") } => false,
        };

        assert!(!finished);
        assert_eq!(store.redact(&text), hidden, "버려진 뒤");
        assert!(store.read(KeyKind::Data).unwrap().is_none());
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
