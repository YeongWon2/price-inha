//! (결정 31) 보관 파일을 백업 묶음에 담고 되살릴 때 `archive/`로 돌린다. 원본에 없음(원본 백업은 스냅샷 하나).
//!
//! 묶음 안 이름은 한 조각이어야 하므로 [`crate::ARCHIVE_BUNDLE_PREFIX`]를 앞에 붙인다(`archive--room-2026-W07.jsonl`).
//! 담는 것은 보관 폴더 바로 아래의 `*.jsonl.gz`(6차, 결정 33 — gzip) · 옛 무압축 `*.jsonl` 파일뿐이다(쓰는 중인 임시
//! 파일 `.…tmp` · 다른 파일 · 하위 폴더는 뺀다). gzip 파일은 등록된 비밀 값 검사를 풀어서 본 원문에 한다
//! ([`plain_view`]).

use std::io::Read;
use std::path::{Path, PathBuf};

use crate::{ARCHIVE_BUNDLE_PREFIX, BackupError, BundleFile};

/// 보관 파일(gzip) 끝 이름.
const GZ_SUFFIX: &str = ".jsonl.gz";
/// 옛 무압축 보관 파일 끝 이름.
const PLAIN_SUFFIX: &str = ".jsonl";

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
        let archive = name.ends_with(GZ_SUFFIX) || name.ends_with(PLAIN_SUFFIX);
        if kind.is_file() && archive && !name.starts_with('.') {
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

/// 비밀 값 검사에 쓸 원문: 묶음의 gzip 보관 파일이면 풀어 준다(풀리지 않으면 오류 — 검사할 수 없는 것은 올리지
/// 않는다). 그 밖의 파일은 `None`(바이트 그대로 검사한다).
pub(crate) fn plain_view(file: &BundleFile) -> Result<Option<Vec<u8>>, BackupError> {
    let gz = file.path.starts_with(ARCHIVE_BUNDLE_PREFIX) && file.path.ends_with(GZ_SUFFIX);
    if !gz {
        return Ok(None);
    }
    let mut out = Vec::new();
    flate2::read::GzDecoder::new(file.bytes.as_slice())
        .read_to_end(&mut out)
        .map_err(|e| BackupError::io(Path::new(&file.path), e))?;
    Ok(Some(out))
}
