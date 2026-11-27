//! (61차) 보관 파일 쓰기: 0700 폴더 · 0600 파일 · 원자적 쓰기(임시 파일 → fsync → 이름 바꾸기) · 다시 읽어 확인.
//! (62차, 결정 33) 파일은 gzip(`.jsonl.gz`)이다. 줄 수 · SHA-256은 풀어서 본 원문 기준이고, 옛 무압축 `.jsonl`도 읽는다.

use std::io::{Read, Write};
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

/// 보관 파일 끝 이름(gzip).
pub(super) const GZ_SUFFIX: &str = ".jsonl.gz";
/// 옛 무압축 보관 파일 끝 이름.
pub(super) const PLAIN_SUFFIX: &str = ".jsonl";

/// 아직 없는 파일 이름: `<앞>-<기간>.jsonl.gz`, 있으면 `<앞>-<기간>-2.jsonl.gz` …(옛 파일을 덮지 않는다). 같은 이름의
/// 옛 무압축 `.jsonl` 파일이 있거나 `taken`(이미 남긴 묶음의 앞 이름 `<앞>-<기간>`)에 있어도 피한다.
pub(super) fn free_name(dir: &Path, stem: &str, taken: &[String]) -> String {
    let mut n = 1;
    loop {
        let base = if n == 1 {
            stem.to_owned()
        } else {
            format!("{stem}-{n}")
        };
        let used = [GZ_SUFFIX, PLAIN_SUFFIX]
            .iter()
            .any(|suffix| dir.join(format!("{base}{suffix}")).exists());
        if !used && !taken.contains(&base) {
            return format!("{base}{GZ_SUFFIX}");
        }
        n += 1;
    }
}

/// 보관 파일 이름에서 끝 이름(`.jsonl.gz` 또는 `.jsonl`)을 뗀 앞 이름.
pub(super) fn stem_of(name: &str) -> &str {
    name.strip_suffix(GZ_SUFFIX)
        .or_else(|| name.strip_suffix(PLAIN_SUFFIX))
        .unwrap_or(name)
}

fn gzip(body: &[u8]) -> std::io::Result<Vec<u8>> {
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(body)?;
    encoder.finish()
}

/// 보관 파일을 읽어 원문을 준다: `.gz`로 끝나면 풀고, 아니면(옛 무압축 `.jsonl`) 그대로. 풀리지 않거나 UTF-8이
/// 아니면 오류다.
pub fn read_archive_file(path: &Path) -> Result<String, ArchiveError> {
    let raw = std::fs::read(path).map_err(|e| fail(path, e))?;
    let plain = if path.extension().is_some_and(|ext| ext == "gz") {
        let mut out = Vec::new();
        flate2::read::GzDecoder::new(raw.as_slice())
            .read_to_end(&mut out)
            .map_err(|e| fail(path, e))?;
        out
    } else {
        raw
    };
    String::from_utf8(plain).map_err(|e| fail(path, e))
}

/// 줄들을 gzip으로 `dir/name`에 원자적으로 쓰고(0600) 다시 읽어 풀어 본 원문의 줄 수 · SHA-256이 쓴 것과 같은지 본 뒤
/// 원문의 SHA-256을 준다.
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
    let packed = gzip(body.as_bytes()).map_err(|e| fail(&temp, e))?;
    file.write_all(&packed)
        .and_then(|()| file.sync_all())
        .map_err(|e| fail(&temp, e))?;
    drop(file);
    std::fs::rename(&temp, &target).map_err(|e| fail(&target, e))?;
    if let Ok(handle) = std::fs::File::open(dir) {
        // 폴더의 이름 바꾸기를 디스크에 남긴다(못 해도 파일은 이미 다 쓰였다).
        let _ = handle.sync_all();
    }
    let read = read_archive_file(&target)?;
    let written = sha256_hex(body.as_bytes());
    let found = sha256_hex(read.as_bytes());
    let count = read.bytes().filter(|b| *b == b'\n').count();
    if found != written || count != lines.len() {
        return Err(fail(&target, "다시 읽은 줄 수나 SHA-256이 쓴 것과 다르다"));
    }
    Ok((target, found))
}
