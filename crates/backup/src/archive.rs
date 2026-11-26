//! (결정 31) 보관 파일을 백업 묶음에 담고 되살릴 때 `archive/`로 돌린다. 원본에 없음(원본 백업은 스냅샷 하나).
//!
//! 묶음 안 이름은 한 조각이어야 하므로 [`crate::ARCHIVE_BUNDLE_PREFIX`]를 앞에 붙인다(`archive--room-2026-W07.jsonl`).
//! 담는 것은 보관 폴더 바로 아래의 `*.jsonl` 파일뿐이다(쓰는 중인 임시 파일 `.…tmp` · 다른 파일 · 하위 폴더는 뺀다).

use std::path::{Path, PathBuf};

use crate::{ARCHIVE_BUNDLE_PREFIX, BackupError, BundleFile};

/// 보관 폴더의 보관 파일들(이름 순서). 폴더가 없으면 빈 목록.
pub(crate) fn bundle_files(dir: &Path) -> Result<Vec<BundleFile>, BackupError> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(BackupError::io(dir, e)),
    };
    let mut names = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| BackupError::io(dir, e))?;
        let kind = entry.file_type().map_err(|e| BackupError::io(dir, e))?;
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        if kind.is_file() && name.ends_with(".jsonl") && !name.starts_with('.') {
            names.push(name);
        }
    }
    names.sort();
    let mut files = Vec::with_capacity(names.len());
    for name in names {
        let path = dir.join(&name);
        let bytes = std::fs::read(&path).map_err(|e| BackupError::io(&path, e))?;
        files.push(BundleFile {
            path: format!("{ARCHIVE_BUNDLE_PREFIX}{name}"),
            bytes,
        });
    }
    Ok(files)
}

/// 되살릴 자리: 보관 파일이면 `target/archive/<이름>`(폴더는 만든다, 유닉스 0700), 아니면 `target/<이름>`.
pub(crate) fn restored_path(target: &Path, bundled: &str) -> Result<PathBuf, BackupError> {
    let Some(name) = bundled
        .strip_prefix(ARCHIVE_BUNDLE_PREFIX)
        .filter(|n| !n.is_empty())
    else {
        return Ok(target.join(bundled));
    };
    let dir = target.join("archive");
    std::fs::create_dir_all(&dir).map_err(|e| BackupError::io(&dir, e))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700))
            .map_err(|e| BackupError::io(&dir, e))?;
    }
    Ok(dir.join(name))
}
