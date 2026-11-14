//! 저장 파일은 본인만 읽는다(3차, 대조표 C4).
//!
//! 원본(Hermes `hermes_state.py`의 저장 파일 권한, OpenClaw의 에이전트 DB 권한)과 같게 한다:
//!
//! - 연결을 만들기 전에 본 파일이 없으면 0600으로 새로 만든다(`O_CREAT | O_EXCL`이라 이미 있는
//!   파일 · 심볼릭 링크 · 폴더는 열지 않는다). 프로세스의 umask가 넓어도 새 파일이 드러나지 않는다.
//! - (4차) 이미 있는 파일은 이 저장소의 파일임을 확인한 뒤에야 맞춘다. 거부한 파일(다른 프로그램의
//!   파일 · 더 새 버전)은 권한도 그대로다(원본과 의도해서 다름, `docs/dev/conformance.md`).
//! - (4차) 파일 시스템 오류는 [`StoreError::FileAccess`]로 낸다.
//! - 본 파일과 딸린 파일(`-wal` · `-shm`)을 0600으로 맞춘다. 없는 파일 · 심볼릭 링크 · 보통 파일이
//!   아닌 것은 건너뛴다. 이미 있는 파일은 열지 않고 경로로 `chmod`한다(파일을 열었다 닫으면 같은
//!   프로세스가 그 파일에 건 SQLite 잠금이 풀린다).
//! - WAL을 켠 뒤 한 번 더 맞춘다(그때 생긴 딸린 파일 몫).
//! - Unix가 아닌 곳에서는 아무것도 하지 않는다.

use std::path::Path;

use crate::error::StoreError;

/// 연결 전에: 본 파일이 없으면 0600으로 만든다. 이미 있는 파일은 건드리지 않는다.
pub(super) fn create_if_missing(path: &Path) -> Result<(), StoreError> {
    imp::create_private(path)
}

/// 이 저장소의 파일임을 확인한 뒤(그리고 WAL을 켠 뒤 한 번 더): 본 파일과 딸린 파일을 0600으로 맞춘다.
pub(super) fn tighten(path: &Path) -> Result<(), StoreError> {
    imp::tighten_all(path)
}

#[cfg(unix)]
fn io_error(path: &Path, err: std::io::Error) -> StoreError {
    StoreError::FileAccess {
        path: path.to_path_buf(),
        source: err,
    }
}

#[cfg(unix)]
mod imp {
    use std::ffi::OsString;
    use std::fs::{self, OpenOptions, Permissions};
    use std::io::ErrorKind;
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    use std::path::{Path, PathBuf};

    use super::io_error;
    use crate::error::StoreError;

    const PRIVATE: u32 = 0o600;

    /// 새 파일만 0600으로 만든다. 이미 있으면(폴더 · 링크 포함) 건드리지 않는다.
    pub(super) fn create_private(path: &Path) -> Result<(), StoreError> {
        let made = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(PRIVATE)
            .open(path);
        match made {
            Ok(_) => Ok(()),
            // 이미 있는 것은 아래에서 좁히거나, 저장 파일이 아니면 SQLite가 오류를 준다.
            Err(err) if err.kind() == ErrorKind::AlreadyExists => Ok(()),
            Err(err) if err.kind() == ErrorKind::IsADirectory => Ok(()),
            Err(err) => Err(io_error(path, err)),
        }
    }

    /// 본 파일과 딸린 파일을 0600으로 맞춘다.
    pub(super) fn tighten_all(path: &Path) -> Result<(), StoreError> {
        for file in [
            path.to_path_buf(),
            sidecar(path, "-wal"),
            sidecar(path, "-shm"),
        ] {
            tighten(&file)?;
        }
        Ok(())
    }

    fn sidecar(path: &Path, suffix: &str) -> PathBuf {
        let mut name = OsString::from(path.as_os_str());
        name.push(suffix);
        PathBuf::from(name)
    }

    fn tighten(path: &Path) -> Result<(), StoreError> {
        let meta = match fs::symlink_metadata(path) {
            Ok(meta) => meta,
            Err(err) if err.kind() == ErrorKind::NotFound => return Ok(()),
            Err(err) => return Err(io_error(path, err)),
        };
        // 심어 둔 링크를 따라가지 않는다. 보통 파일이 아니면 SQLite가 다룰 일이다.
        if !meta.file_type().is_file() {
            return Ok(());
        }
        if meta.permissions().mode() & 0o7777 == PRIVATE {
            return Ok(());
        }
        fs::set_permissions(path, Permissions::from_mode(PRIVATE))
            .map_err(|err| io_error(path, err))
    }
}

#[cfg(not(unix))]
mod imp {
    use std::path::Path;

    use crate::error::StoreError;

    pub(super) fn create_private(_path: &Path) -> Result<(), StoreError> {
        Ok(())
    }

    pub(super) fn tighten_all(_path: &Path) -> Result<(), StoreError> {
        Ok(())
    }
}
