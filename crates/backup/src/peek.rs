//! (7차, 백로그 U 2번) 백업기 없이 상태 파일만 읽기. 진단(`price-inha doctor`)처럼 저장소를 열지 않는 명령이 쓴다.

use std::path::Path;

use crate::{BackupError, BackupStatus};

/// `local_dir`(백업 설정의 [`crate::BackupSettings::local_dir`])의 상태 파일을 읽기만 한다.
pub fn read_status(local_dir: &Path) -> Result<BackupStatus, BackupError> {
    let _ = local_dir;
    todo!()
}
