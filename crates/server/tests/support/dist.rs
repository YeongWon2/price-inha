//! 화면 빌드 결과 폴더(server 11차).

use std::path::PathBuf;

/// 빌드 결과 폴더(`crates/server/web/dist`).
pub fn dist_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("web/dist")
}
