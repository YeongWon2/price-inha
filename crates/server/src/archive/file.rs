//! (61차) 보관 파일 쓰기: 0700 폴더 · 0600 파일 · 원자적 쓰기(임시 파일 → fsync → 이름 바꾸기) · 다시 읽어 확인.

use std::io::Write;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::error::ArchiveError;

fn fail(path: &Path, reason: impl ToString) -> ArchiveError {
    ArchiveError::File {
        path: path.display().to_string(),
        reason: reason.to_string(),
    }
}

/// 보관 폴더를 만들고(있으면 그대로) 유닉스에서는 0700으로 맞춘다.
pub(super) fn ensure_dir(dir: &Path) -> Result<(), ArchiveError> {
    std::fs::create_dir_all(dir).map_err(|e| fail(dir, e))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))
            .map_err(|e| fail(dir, e))?;
    }
    Ok(())
}

/// SHA-256(소문자 16진수).
pub(super) fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// 아직 없는 파일 이름: `<앞>-<기간>.jsonl`, 있으면 `<앞>-<기간>-2.jsonl` …(옛 파일을 덮지 않는다).
pub(super) fn free_name(dir: &Path, stem: &str, taken: &[String]) -> String {
    let mut n = 1;
    loop {
        let name = if n == 1 {
            format!("{stem}.jsonl")
        } else {
            format!("{stem}-{n}.jsonl")
        };
        if !dir.join(&name).exists() && !taken.contains(&name) {
            return name;
        }
        n += 1;
    }
}

/// 줄들을 `dir/name`에 원자적으로 쓰고(0600) 다시 읽어 줄 수 · 바이트가 같은지 본 뒤 SHA-256을 준다.
pub(super) fn write_verified(
    dir: &Path,
    name: &str,
    lines: &[String],
) -> Result<(PathBuf, String), ArchiveError> {
    let mut body = lines.join("\n");
    body.push('\n');
    let target = dir.join(name);
    let temp = dir.join(format!(".{name}.tmp"));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let mut file = options.open(&temp).map_err(|e| fail(&temp, e))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(std::fs::Permissions::from_mode(0o600))
            .map_err(|e| fail(&temp, e))?;
    }
    file.write_all(body.as_bytes())
        .and_then(|()| file.sync_all())
        .map_err(|e| fail(&temp, e))?;
    drop(file);
    std::fs::rename(&temp, &target).map_err(|e| fail(&target, e))?;
    if let Ok(handle) = std::fs::File::open(dir) {
        // 폴더의 이름 바꾸기를 디스크에 남긴다(못 해도 파일은 이미 다 쓰였다).
        let _ = handle.sync_all();
    }
    let read = std::fs::read(&target).map_err(|e| fail(&target, e))?;
    let written = sha256_hex(body.as_bytes());
    let found = sha256_hex(&read);
    let count = read.iter().filter(|b| **b == b'\n').count();
    if found != written || count != lines.len() {
        return Err(fail(&target, "다시 읽은 줄 수나 SHA-256이 쓴 것과 다르다"));
    }
    Ok((target, found))
}

/// (62차) 보관 파일을 읽어 원문을 준다. 뼈대.
pub fn read_archive_file(path: &Path) -> Result<String, ArchiveError> {
    Err(fail(path, "아직 없다"))
}
