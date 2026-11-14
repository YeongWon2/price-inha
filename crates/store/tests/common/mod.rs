//! 테스트가 함께 쓰는 도우미. 모든 테스트 파일이 여기 있는 것을 전부 쓴다.

use std::path::PathBuf;

use pi_store::{Store, UnixMillis};
use tempfile::TempDir;

/// 임시 폴더에 새 저장소를 연다. 폴더가 사라지지 않게 `TempDir`을 함께 돌려준다.
pub fn temp_store() -> (TempDir, PathBuf, Store) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("pi.sqlite");
    let store = Store::open(&path).unwrap();
    (dir, path, store)
}

/// 테스트 시계. 고정된 기준 시각에서 `n` 밀리초 뒤.
pub fn t(n: i64) -> UnixMillis {
    UnixMillis(1_800_000_000_000 + n)
}
