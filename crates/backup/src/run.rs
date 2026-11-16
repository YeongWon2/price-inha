//! 한 번 올리기와 상태 기록 (D7 · C13 · E15 · E39, 원본 동작: OpenClaw 올리기 순서 · Hermes 스냅샷).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use pi_secrets::{KeySlot, SecretStore, SecretString};
use pi_store::{Store, UnixMillis};

use crate::remote::Remote;
use crate::{
    BackupError, Bundle, BundleFile, FailureKind, MANIFEST_VERSION, Manifest, ManifestFile,
    REMOTE_DIR, SNAPSHOT_FILE, backup_file_name, parse_backup_file_name, plan_retention, seal,
    sha256_hex, status,
};

/// 백업 설정. 저장소 이름과 토큰은 사용자가 화면에서 등록한다(결정 2026-11-16 23번).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackupSettings {
    /// GitHub API 기본 주소(끝에 `/` 없이). 운영에서는 GitHub의 주소, 시험에서는 로컬 가짜 서버.
    pub api_base: String,
    /// 비공개 저장소 이름 `소유자/이름`.
    pub repo: String,
    /// age 수신자 공개키(`age1…`). 서버에는 공개키만 둔다.
    pub recipient: String,
    /// 암호화된 로컬 사본과 상태 파일을 두는 폴더. 없으면 만든다.
    pub local_dir: PathBuf,
    /// 평문 스냅샷을 잠깐 뜨는 폴더의 부모. 한 번 돌 때마다 그 안에 새 폴더를 만들고 끝나면(실패해도) 지운다.
    pub temp_root: PathBuf,
}

/// 백업기.
pub struct Backup {
    settings: BackupSettings,
    store: Store,
    secrets: Arc<SecretStore>,
}

/// 성공한 올리기.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackupReport {
    /// 이번 백업 시각(`run`에 넘긴 `now`).
    pub at: UnixMillis,
    /// 원격 경로 `REMOTE_DIR/이름`.
    pub remote_path: String,
    /// 올린 암호문의 바이트 수(원격이 확인해 준 크기).
    pub size: u64,
    /// 묶음의 manifest.
    pub manifest: Manifest,
    /// 보존 규칙으로 지운 백업 시각들(로컬과 원격에서 같게).
    pub removed: Vec<UnixMillis>,
}

/// 실패한 올리기. 오류와 알림 요구를 함께 낸다(E15).
#[derive(Debug)]
pub struct BackupFailure {
    /// 무엇이 잘못됐나.
    pub error: BackupError,
    /// 사용자에게 보낼 알림 요구.
    pub notice: BackupNotice,
}

/// 백업 실패 알림 요구(E15). 비밀 값이 없다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BackupNotice {
    /// 실패 시각.
    pub at: UnixMillis,
    /// 실패 종류.
    pub kind: FailureKind,
}

/// 마지막 실패.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LastFailure {
    /// 시각.
    pub at: UnixMillis,
    /// 종류.
    pub kind: FailureKind,
}

/// 백업 상태. `local_dir`에 남아 다시 열어도 같다.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct BackupStatus {
    /// 마지막 성공 시각(하루 정리에 보인다, V50).
    pub last_success: Option<UnixMillis>,
    /// 마지막 실패.
    pub last_failure: Option<LastFailure>,
    /// 마지막 시도가 실패해 다음 장 마감에 다시 올려야 하는가(E15).
    pub retry_pending: bool,
    /// 마지막으로 올리기는 성공했지만 보존 규칙 적용(지우기)만 실패한 기록. 올리기 실패와 따로 둔다.
    pub last_retention_failure: Option<LastFailure>,
}

impl Backup {
    /// 백업기를 만든다. 저장소 이름이 `소유자/이름`(두 조각 모두 비지 않음)이 아니면
    /// [`BackupError::InvalidRepo`]. 네트워크는 쓰지 않는다.
    pub fn new(
        settings: BackupSettings,
        store: Store,
        secrets: Arc<SecretStore>,
    ) -> Result<Self, BackupError> {
        if !valid_repo(&settings.repo) {
            return Err(BackupError::InvalidRepo);
        }
        Ok(Self {
            settings,
            store,
            secrets,
        })
    }

    /// 한 번 올린다. `now`가 이번 백업 시각이다(파일 이름 · manifest · 상태 기록).
    ///
    /// 순서: 토큰([`pi_secrets::KeySlot::BackupGithubToken`]) 읽기 → 임시 폴더에 스냅샷 → 비밀 검사
    /// ([`SecretStore::contains_registered`]가 참이면 [`BackupError::SecretFound`], 아무것도 올리지 않는다)
    /// → 묶기 · 검증 → 암호화 → contents API로 `REMOTE_DIR/`[`crate::backup_file_name`]`(now)`에 올리기
    /// → 원격 크기 확인 → 로컬 사본 → 보존 규칙(로컬과 원격 같게, 이 이름 모양의 파일만) → 상태 기록.
    /// 임시 폴더는 성공해도 실패해도 지운다.
    ///
    /// 실패하면 상태에 실패와 "다시 올림"을 남기고 알림 요구와 함께 돌려준다. 오래된 백업은 지우지 않는다
    /// (온전한 백업 뒤에만 지운다).
    pub async fn run(&self, now: UnixMillis) -> Result<BackupReport, BackupFailure> {
        let (report, retention_failed) = match self.attempt(now).await {
            Ok(done) => done,
            Err(error) => return Err(self.fail(now, error)),
        };
        let mut recorded = self.status().unwrap_or_default();
        recorded.last_success = Some(now);
        recorded.retry_pending = false;
        // 올리기는 성공했다. 보존 지우기만 실패했으면 따로 남기고 다시 올리지 않는다.
        recorded.last_retention_failure = retention_failed.then_some(LastFailure {
            at: now,
            kind: FailureKind::Retention,
        });
        match status::write(&self.settings.local_dir, &recorded) {
            Ok(()) => Ok(report),
            Err(error) => Err(self.fail(now, error)),
        }
    }

    /// 지금 상태. 기록이 없으면 [`BackupStatus::default`].
    pub fn status(&self) -> Result<BackupStatus, BackupError> {
        status::read(&self.settings.local_dir)
    }

    /// 실패를 기록하고(다시 올림 표시) 알림 요구와 묶는다. 기록이 안 되더라도 실패는 그대로 돌려준다.
    fn fail(&self, now: UnixMillis, error: BackupError) -> BackupFailure {
        let kind = error.kind();
        let mut recorded = self.status().unwrap_or_default();
        recorded.last_failure = Some(LastFailure { at: now, kind });
        recorded.retry_pending = true;
        // 상태 파일을 못 써도 원래 오류와 알림 요구가 더 중요하다.
        let _ = status::write(&self.settings.local_dir, &recorded);
        BackupFailure {
            error,
            notice: BackupNotice { at: now, kind },
        }
    }

    /// 올리기까지 해낸 결과와, 보존 지우기가 실패했는지.
    async fn attempt(&self, now: UnixMillis) -> Result<(BackupReport, bool), BackupError> {
        let token = self.token()?;
        let (manifest, sealed) = self.prepare(now)?;
        let remote = Remote::new(&self.settings.api_base, &self.settings.repo, token)?;
        let name = backup_file_name(now);
        let remote_path = format!("{REMOTE_DIR}/{name}");
        let sent = sealed.len() as u64;
        let stored = remote.put(&remote_path, &sealed).await?;
        if stored != sent {
            return Err(BackupError::SizeMismatch { sent, stored });
        }
        self.write_local(&name, &sealed)?;
        let (removed, retention_failed) = match self.apply_retention(&remote, now).await {
            Ok(removed) => (removed, false),
            Err(_) => (Vec::new(), true),
        };
        let report = BackupReport {
            at: now,
            remote_path,
            size: stored,
            manifest,
            removed,
        };
        Ok((report, retention_failed))
    }

    fn token(&self) -> Result<SecretString, BackupError> {
        self.secrets
            .read_slot(KeySlot::BackupGithubToken)?
            .and_then(|values| values.get("token").cloned())
            .ok_or(BackupError::TokenMissing)
    }

    /// 임시 폴더에 스냅샷을 뜨고, 비밀을 검사하고, 묶어 검증하고, 암호화한다. 임시 폴더는 여기서 지운다.
    fn prepare(&self, now: UnixMillis) -> Result<(Manifest, Vec<u8>), BackupError> {
        let root = &self.settings.temp_root;
        std::fs::create_dir_all(root).map_err(|e| BackupError::io(root, e))?;
        let work = tempfile::Builder::new()
            .prefix("backup-")
            .tempdir_in(root)
            .map_err(|e| BackupError::io(root, e))?;
        let snapshot = work.path().join(SNAPSHOT_FILE);
        self.store.snapshot_to(&snapshot)?;
        let bytes = std::fs::read(&snapshot).map_err(|e| BackupError::io(&snapshot, e))?;
        let path = work.path().to_owned();
        work.close().map_err(|e| BackupError::io(path, e))?;

        let bundle = Bundle {
            manifest: Manifest {
                version: MANIFEST_VERSION,
                created_at: now,
                files: vec![ManifestFile {
                    path: SNAPSHOT_FILE.to_owned(),
                    size: bytes.len() as u64,
                    sha256: sha256_hex(&bytes),
                }],
            },
            files: vec![BundleFile {
                path: SNAPSHOT_FILE.to_owned(),
                bytes,
            }],
        };
        if bundle
            .files
            .iter()
            .any(|f| self.secrets.contains_registered(&f.bytes))
        {
            return Err(BackupError::SecretFound);
        }
        crate::bundle::verify_in(&bundle, root)?;
        let sealed = seal(&bundle, &self.settings.recipient)?;
        Ok((bundle.manifest, sealed))
    }

    fn write_local(&self, name: &str, sealed: &[u8]) -> Result<(), BackupError> {
        let dir = &self.settings.local_dir;
        std::fs::create_dir_all(dir).map_err(|e| BackupError::io(dir, e))?;
        let path = dir.join(name);
        std::fs::write(&path, sealed).map_err(|e| BackupError::io(path, e))
    }

    /// 로컬과 원격의 백업 시각을 합쳐 한 계획을 세우고 양쪽에서 같게 지운다.
    async fn apply_retention(
        &self,
        remote: &Remote,
        now: UnixMillis,
    ) -> Result<Vec<UnixMillis>, BackupError> {
        let remote_backups = remote.list(REMOTE_DIR).await?;
        let local_times = local_backups(&self.settings.local_dir)?;
        let mut times: Vec<UnixMillis> = remote_backups.iter().map(|b| b.at).collect();
        times.extend(local_times);
        times.push(now);
        let plan = plan_retention(&times);
        // 시계가 뒤로 가 더 새 이름의 파일이 있어도 이번에 올린 백업은 지우지 않는다.
        let removable: Vec<UnixMillis> = plan.remove.into_iter().filter(|&at| at != now).collect();
        for &at in &removable {
            for found in remote_backups.iter().filter(|b| b.at == at) {
                remote.delete(&found.path, &found.sha).await?;
            }
            let local = self.settings.local_dir.join(backup_file_name(at));
            match std::fs::remove_file(&local) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(BackupError::io(local, e)),
            }
        }
        Ok(removable)
    }
}

/// 로컬 폴더 안 백업 파일 이름 모양인 파일들의 시각.
fn local_backups(dir: &Path) -> Result<Vec<UnixMillis>, BackupError> {
    let read = std::fs::read_dir(dir).map_err(|e| BackupError::io(dir, e))?;
    let mut times = Vec::new();
    for entry in read {
        let entry = entry.map_err(|e| BackupError::io(dir, e))?;
        let is_file = entry.file_type().is_ok_and(|t| t.is_file());
        if let (true, Some(name)) = (is_file, entry.file_name().to_str()) {
            times.extend(parse_backup_file_name(name));
        }
    }
    Ok(times)
}

/// `소유자/이름`이고 두 조각 모두 비지 않으며 GitHub 이름 글자(영숫자 · `-` · `_` · `.`)만 쓴다.
fn valid_repo(repo: &str) -> bool {
    let piece_ok = |p: &str| {
        !p.is_empty()
            && p != "."
            && p != ".."
            && p.chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    };
    match repo.split_once('/') {
        Some((owner, name)) => piece_ok(owner) && piece_ok(name),
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::valid_repo;

    #[test]
    fn 저장소_이름은_두_조각이다() {
        assert!(valid_repo("owner/repo"));
        assert!(valid_repo("my-org/backups.v1"));
        for bad in [
            "", "repo", "/repo", "owner/", "a/b/c", "a/..", "a/b?x", "a b/c",
        ] {
            assert!(!valid_repo(bad), "{bad:?}");
        }
    }
}
