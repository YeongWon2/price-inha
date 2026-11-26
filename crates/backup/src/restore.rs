//! 되살리기 (V51 · F9, 원본 동작: OpenClaw `backup restore` — 빈 폴더에만, 쓰기 전에 검증).

use std::path::Path;

use pi_secrets::SecretString;
use pi_store::{RestoreOutcome, Store, UnixMillis};

use crate::{BackupError, Manifest, SNAPSHOT_FILE, open, verify};

/// 되살리기 결과. identity는 담지 않는다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestoreReport {
    /// 되살린 묶음의 manifest.
    pub manifest: Manifest,
    /// [`pi_store::Store::mark_restored`]가 한 일(팀 전체 정지 · 만료한 승인).
    pub outcome: RestoreOutcome,
}

/// 암호문을 `target` 폴더에 되살린다.
///
/// 1. `target`은 없거나 빈 폴더여야 한다. 아니면 [`BackupError::TargetNotEmpty`]이고 아무것도 쓰지 않는다
///    (제자리 되살리기 · 덮어쓰기는 없다).
/// 2. `identity`로 풀고([`crate::open`]) 검증한다([`crate::verify`]). 실패하면 아무것도 쓰지 않는다
///    (`target`이 없었으면 만들지도 않는다).
/// 3. 파일을 쓰고, `target/`[`crate::SNAPSHOT_FILE`] 저장소에 `mark_restored(at)`을 부른다:
///    팀은 전체 정지가 되고 기다리던 승인은 만료된다.
///
/// `identity`는 인자로만 쓰고 파일 · 기록 · 오류 어디에도 남기지 않는다.
pub fn restore(
    encrypted: &[u8],
    identity: &SecretString,
    target: &Path,
    at: UnixMillis,
) -> Result<RestoreReport, BackupError> {
    restore_with(encrypted, identity, target, None, at)
}

/// [`restore`]와 같되 검증 사본을 `temp_root` 안(0700 폴더)에만 두고 끝나면 지운다
/// (시스템 임시 폴더에 평문을 두지 않는다). `temp_root`가 없으면 만든다.
pub fn restore_in(
    encrypted: &[u8],
    identity: &SecretString,
    target: &Path,
    temp_root: &Path,
    at: UnixMillis,
) -> Result<RestoreReport, BackupError> {
    restore_with(encrypted, identity, target, Some(temp_root), at)
}

fn restore_with(
    encrypted: &[u8],
    identity: &SecretString,
    target: &Path,
    temp_root: Option<&Path>,
    at: UnixMillis,
) -> Result<RestoreReport, BackupError> {
    ensure_empty(target)?;
    let bundle = open(encrypted, identity)?;
    match temp_root {
        Some(root) => {
            std::fs::create_dir_all(root).map_err(|e| BackupError::io(root, e))?;
            crate::bundle::verify_in(&bundle, root)?;
        }
        None => verify(&bundle)?,
    }

    std::fs::create_dir_all(target).map_err(|e| BackupError::io(target, e))?;
    for file in &bundle.files {
        // 검증에서 경로가 한 조각임을 확인했다. (결정 31) 보관 파일은 `archive/` 아래로 돌린다.
        let path = crate::archive::restored_path(target, &file.path)?;
        write_new(&path, &file.bytes)?;
    }
    let store = Store::open(target.join(SNAPSHOT_FILE))?;
    let outcome = store.mark_restored(at)?;
    Ok(RestoreReport {
        manifest: bundle.manifest,
        outcome,
    })
}

/// 대상이 없거나 빈 폴더인가. 파일이거나 무언가 들어 있으면 [`BackupError::TargetNotEmpty`].
fn ensure_empty(target: &Path) -> Result<(), BackupError> {
    let meta = match std::fs::symlink_metadata(target) {
        Ok(meta) => meta,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(BackupError::io(target, e)),
    };
    if !meta.is_dir() {
        return Err(BackupError::TargetNotEmpty(target.to_owned()));
    }
    let mut entries = std::fs::read_dir(target).map_err(|e| BackupError::io(target, e))?;
    if entries.next().is_some() {
        return Err(BackupError::TargetNotEmpty(target.to_owned()));
    }
    Ok(())
}

/// 새 파일로만 쓴다(있으면 덮어쓰지 않는다). Unix에서는 처음부터 0600이다.
fn write_new(path: &Path, bytes: &[u8]) -> Result<(), BackupError> {
    use std::io::Write;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let mut file = options.open(path).map_err(|e| BackupError::io(path, e))?;
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|e| BackupError::io(path, e))
}
