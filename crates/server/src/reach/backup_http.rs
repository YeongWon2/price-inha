//! 백업 설정 화면(저장소 이름 · 수신자 공개키)과 되살리기.

use std::path::Path;

use pi_backup::RestoreReport;
use pi_secrets::SecretString;
use pi_store::UnixMillis;

use crate::AppError;

/// 백업 설정. `GET` → `{"repo", "recipient", "configured"}`, `POST` 몸통 `{"repo", "recipient"}`.
pub const SETUP_BACKUP_PATH: &str = "/api/setup/backup";

/// 암호문을 빈 폴더 `target`에 되살린다.
pub fn restore_backup(
    _encrypted: &[u8],
    _identity: &SecretString,
    _target: &Path,
    _at: UnixMillis,
) -> Result<RestoreReport, AppError> {
    Err(AppError::Backup("아직 없다".to_string()))
}
