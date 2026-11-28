//! (7차, 백로그 U 2번) 백업기 없이 상태 파일만 읽기. 진단(`price-inha doctor`)처럼 저장소를 열지 않는 명령이 쓴다
//! ([`crate::Backup`]은 저장소 손잡이가 있어야 만든다 — 그 손잡이를 열면 판이 오를 수 있다).

use std::path::Path;

use crate::{BackupError, BackupStatus, status};

/// `local_dir`(백업 설정의 [`crate::BackupSettings::local_dir`])의 상태 파일을 읽기만 한다. [`crate::Backup::status`]와
/// 같은 값이다. 파일이 없으면 [`BackupStatus::default`]이고 폴더 · 파일을 만들지 않는다. 읽지 못하거나 모양이
/// 다르면 [`BackupError`].
pub fn read_status(local_dir: &Path) -> Result<BackupStatus, BackupError> {
    status::read(local_dir)
}
