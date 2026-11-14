#![cfg(all(test, unix))]
//! 4차 2번 · 4번: 저장 파일을 다루는 파일 시스템 오류와 권한의 빈칸.
//!
//! - 2번: 저장 파일을 만들거나 권한을 맞추는 IO 오류는 `Incompatible`이 아니라
//!   [`StoreError::FileAccess`]로 따로 구분된다.
//! - 4번: 심어 둔 심볼릭 링크는 따라가 권한을 바꾸지 않는다. 이 저장소의 파일이 아니어서(또는 더 새
//!   버전이어서) 거부한 파일은 권한도 그대로다("거부된 호출은 아무것도 바꾸지 않는다").

mod common;

use std::fs;
use std::io::ErrorKind;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::{Path, PathBuf};

use common::{t, temp_store};
use pi_store::{Store, StoreError};
use serde_json::json;

/// 권한 비트만(파일 종류 비트는 뺀다). 링크를 따라가지 않는다.
fn mode(path: &Path) -> u32 {
    fs::symlink_metadata(path).unwrap().permissions().mode() & 0o7777
}

fn set_mode(path: &Path, mode: u32) {
    fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
}

fn sidecar(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(suffix);
    PathBuf::from(name)
}

fn temp_path() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("pi.sqlite");
    (dir, path)
}

// ───────────── 2번: 권한 오류는 따로 구분된다 ─────────────

// 검증: D6
#[test]
fn 쓸_수_없는_폴더에_새_저장소를_열면_파일_오류이고_incompatible이_아니다() {
    let dir = tempfile::tempdir().unwrap();
    let locked = dir.path().join("locked");
    fs::create_dir(&locked).unwrap();
    set_mode(&locked, 0o500);
    // 관리자 권한으로 돌면 폴더 권한이 막지 못하므로 볼 것이 없다.
    let probe = locked.join("probe");
    if fs::write(&probe, b"").is_ok() {
        set_mode(&locked, 0o700);
        return;
    }
    let path = locked.join("pi.sqlite");
    let result = Store::open(&path);
    set_mode(&locked, 0o700);
    match result {
        Err(StoreError::FileAccess { path: got, source }) => {
            assert_eq!(got, path);
            assert_eq!(source.kind(), ErrorKind::PermissionDenied);
        }
        Err(StoreError::Incompatible(message)) => {
            panic!("권한 오류가 Incompatible로 나왔다: {message}")
        }
        Err(other) => panic!("권한 오류가 FileAccess로 나와야 한다: {other:?}"),
        Ok(_) => panic!("쓸 수 없는 폴더에서는 열리지 않아야 한다"),
    }
    assert!(!path.exists());
}

// 검증: D6
#[test]
fn 파일_오류는_오류_글에_파일_경로를_담고_원인을_잇는다() {
    let dir = tempfile::tempdir().unwrap();
    let locked = dir.path().join("locked");
    fs::create_dir(&locked).unwrap();
    set_mode(&locked, 0o500);
    if fs::write(locked.join("probe"), b"").is_ok() {
        set_mode(&locked, 0o700);
        return;
    }
    let path = locked.join("pi.sqlite");
    let result = Store::open(&path);
    set_mode(&locked, 0o700);
    let Err(err) = result else {
        panic!("쓸 수 없는 폴더에서는 열리지 않아야 한다");
    };
    assert!(matches!(err, StoreError::FileAccess { .. }), "{err:?}");
    assert!(
        err.to_string().contains(&path.display().to_string()),
        "{err}"
    );
    let source = std::error::Error::source(&err).expect("원인 IO 오류가 이어져 있어야 한다");
    assert!(source.downcast_ref::<std::io::Error>().is_some());
}

// 검증: D6
#[test]
fn 이_저장소의_파일이_아니면_여전히_incompatible이다() {
    let (_dir, path) = temp_path();
    let conn = rusqlite::Connection::open(&path).unwrap();
    conn.execute_batch("CREATE TABLE notes (body TEXT); INSERT INTO notes VALUES ('x');")
        .unwrap();
    drop(conn);
    assert!(matches!(
        Store::open(&path),
        Err(StoreError::Incompatible(_))
    ));
}

// ───────────── 4번: 심볼릭 링크 ─────────────

// 검증: D6
#[test]
fn 저장소_경로에_둔_심볼릭_링크를_따라가_대상_파일의_권한을_바꾸지_않는다() {
    let (dir, path) = temp_path();
    let target = dir.path().join("elsewhere.db");
    fs::write(&target, b"").unwrap();
    set_mode(&target, 0o644);
    symlink(&target, &path).unwrap();

    // 열리든 거부되든 링크 너머의 파일 권한은 그대로다.
    drop(Store::open(&path));
    assert_eq!(mode(&target), 0o644);
    assert!(
        fs::symlink_metadata(&path)
            .unwrap()
            .file_type()
            .is_symlink()
    );
}

// 검증: D6
#[test]
fn 딸린_파일_자리에_둔_심볼릭_링크를_따라가_대상_파일의_권한을_바꾸지_않는다() {
    let (dir, path, store) = temp_store();
    store.append_at("chat", &json!({ "n": 1 }), t(1)).unwrap();
    drop(store);
    for suffix in ["-wal", "-shm"] {
        let side = sidecar(&path, suffix);
        if side.exists() {
            fs::remove_file(&side).unwrap();
        }
    }
    let other_wal = dir.path().join("other-wal.txt");
    let other_shm = dir.path().join("other-shm.txt");
    for other in [&other_wal, &other_shm] {
        fs::write(other, b"").unwrap();
        set_mode(other, 0o644);
    }
    symlink(&other_wal, sidecar(&path, "-wal")).unwrap();
    symlink(&other_shm, sidecar(&path, "-shm")).unwrap();

    drop(Store::open(&path));
    assert_eq!(mode(&other_wal), 0o644);
    assert_eq!(mode(&other_shm), 0o644);
}

// ───────────── 4번: 저장소 파일이 아닌 파일 ─────────────

// 검증: D6
#[test]
fn 다른_프로그램의_sqlite_파일은_거부하고_권한도_바꾸지_않는다() {
    let (_dir, path) = temp_path();
    let conn = rusqlite::Connection::open(&path).unwrap();
    conn.execute_batch("CREATE TABLE notes (body TEXT); INSERT INTO notes VALUES ('x');")
        .unwrap();
    drop(conn);
    set_mode(&path, 0o644);

    assert!(matches!(
        Store::open(&path),
        Err(StoreError::Incompatible(_))
    ));
    assert_eq!(mode(&path), 0o644);
    assert!(!sidecar(&path, "-wal").exists());
}

// 검증: D6
#[test]
fn sqlite_파일이_아닌_파일은_거부하고_권한도_바꾸지_않는다() {
    let (_dir, path) = temp_path();
    fs::write(&path, "이것은 데이터베이스가 아니다. ".repeat(200)).unwrap();
    set_mode(&path, 0o644);

    assert!(Store::open(&path).is_err());
    assert_eq!(mode(&path), 0o644);
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        "이것은 데이터베이스가 아니다. ".repeat(200)
    );
}

// 검증: D6
#[test]
fn 더_새_버전의_저장소_파일은_거부하고_권한도_바꾸지_않는다() {
    let (_dir, path, store) = temp_store();
    drop(store);
    let conn = rusqlite::Connection::open(&path).unwrap();
    conn.pragma_update(None, "user_version", 999).unwrap();
    drop(conn);
    for suffix in ["-wal", "-shm"] {
        let side = sidecar(&path, suffix);
        if side.exists() {
            fs::remove_file(&side).unwrap();
        }
    }
    set_mode(&path, 0o644);

    assert!(matches!(
        Store::open(&path),
        Err(StoreError::Incompatible(_))
    ));
    assert_eq!(mode(&path), 0o644);
}
