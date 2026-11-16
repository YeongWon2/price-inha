//! 백업 상태 파일(`local_dir` 안). 비밀 값 · 자유 글자를 담지 않는다(E15 · P22).

use std::path::Path;

use pi_store::UnixMillis;
use serde::{Deserialize, Serialize};

use crate::{BackupError, BackupStatus, FailureKind, LastFailure};

/// 상태 파일 이름. 백업 파일 이름 모양이 아니라 보존 규칙이 건드리지 않는다.
const STATUS_FILE: &str = "backup-status.json";

#[derive(Serialize, Deserialize, Default)]
struct WireStatus {
    last_success: Option<i64>,
    last_failure: Option<WireFailure>,
    retry_pending: bool,
}

#[derive(Serialize, Deserialize)]
struct WireFailure {
    at: i64,
    kind: String,
}

fn kind_code(kind: FailureKind) -> &'static str {
    match kind {
        FailureKind::TokenMissing => "token_missing",
        FailureKind::SecretFound => "secret_found",
        FailureKind::Snapshot => "snapshot",
        FailureKind::Encrypt => "encrypt",
        FailureKind::Upload => "upload",
        FailureKind::SizeMismatch => "size_mismatch",
        FailureKind::Local => "local",
        FailureKind::Retention => "retention",
    }
}

fn kind_from(code: &str) -> FailureKind {
    match code {
        "token_missing" => FailureKind::TokenMissing,
        "secret_found" => FailureKind::SecretFound,
        "snapshot" => FailureKind::Snapshot,
        "encrypt" => FailureKind::Encrypt,
        "upload" => FailureKind::Upload,
        "size_mismatch" => FailureKind::SizeMismatch,
        "retention" => FailureKind::Retention,
        _ => FailureKind::Local,
    }
}

/// 상태를 읽는다. 파일이 없으면 기본값.
pub(crate) fn read(dir: &Path) -> Result<BackupStatus, BackupError> {
    let path = dir.join(STATUS_FILE);
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(BackupStatus::default()),
        Err(e) => return Err(BackupError::io(path, e)),
    };
    let wire: WireStatus = serde_json::from_slice(&bytes).map_err(|e| {
        BackupError::io(
            &path,
            std::io::Error::new(std::io::ErrorKind::InvalidData, e),
        )
    })?;
    Ok(BackupStatus {
        last_success: wire.last_success.map(UnixMillis),
        last_failure: wire.last_failure.map(|f| LastFailure {
            at: UnixMillis(f.at),
            kind: kind_from(&f.kind),
        }),
        retry_pending: wire.retry_pending,
        last_retention_failure: None,
    })
}

/// 상태를 쓴다(임시 파일에 쓰고 바꿔 넣는다).
pub(crate) fn write(dir: &Path, status: &BackupStatus) -> Result<(), BackupError> {
    std::fs::create_dir_all(dir).map_err(|e| BackupError::io(dir, e))?;
    let wire = WireStatus {
        last_success: status.last_success.map(|t| t.0),
        last_failure: status.last_failure.map(|f| WireFailure {
            at: f.at.0,
            kind: kind_code(f.kind).to_owned(),
        }),
        retry_pending: status.retry_pending,
    };
    let bytes = serde_json::to_vec_pretty(&wire)
        .map_err(|e| BackupError::io(dir, std::io::Error::other(e)))?;
    let path = dir.join(STATUS_FILE);
    let temp = dir.join(format!("{STATUS_FILE}.tmp"));
    std::fs::write(&temp, bytes).map_err(|e| BackupError::io(&temp, e))?;
    std::fs::rename(&temp, &path).map_err(|e| BackupError::io(&path, e))
}

#[cfg(test)]
mod tests {
    use super::{kind_code, kind_from};
    use crate::FailureKind;

    #[test]
    fn 실패_종류는_글자로_갔다가_되돌아온다() {
        for kind in [
            FailureKind::TokenMissing,
            FailureKind::SecretFound,
            FailureKind::Snapshot,
            FailureKind::Encrypt,
            FailureKind::Upload,
            FailureKind::SizeMismatch,
            FailureKind::Local,
        ] {
            assert_eq!(kind_from(kind_code(kind)), kind);
        }
    }
}
