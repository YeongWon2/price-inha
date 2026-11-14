#![cfg(all(test, unix))]
//! 3차 규칙 1 (대조표 C4): 저장 파일은 본인만 읽는다.
//!
//! 저장소를 열 때 DB 파일과 딸린 파일(`-wal` · `-shm`)의 권한을 0600으로 만든다.
//! 이미 있는 파일의 권한이 더 넓으면 좁힌다. Unix에서만 본다.
//!
//! 원본에서 확인한 것: Hermes는 연결을 만들기 전에 본 파일을 0600으로 만들거나 좁히고,
//! WAL을 켠 뒤에 딸린 파일을 한 번 더 좁힌다(`hermes_state.py:247-300, 812-828`).
//! OpenClaw도 열 때 DB 파일들을 0600으로 맞춘다(`openclaw-agent-db-permissions.ts:7-8, 33-44`).

mod common;

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use common::{t, temp_store};
use pi_store::{NewAccess, Store};
use serde_json::json;

const FIXTURE_V1: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/v1.db");
const FIXTURE_V2: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/v2.db");

/// 권한 비트만(파일 종류 비트는 뺀다).
fn mode(path: &Path) -> u32 {
    fs::metadata(path).unwrap().permissions().mode() & 0o7777
}

fn set_mode(path: &Path, mode: u32) {
    fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
}

/// 딸린 파일의 경로(`pi.sqlite` → `pi.sqlite-wal`).
fn sidecar(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(suffix);
    PathBuf::from(name)
}

fn access(n: i64) -> NewAccess {
    NewAccess {
        at: t(n),
        origin: "127.0.0.1".to_string(),
        action: format!("{n}번째 접속"),
    }
}

/// 본 파일과 딸린 파일 둘이 모두 있고 모두 0600인지 본다.
fn assert_all_private(path: &Path) {
    for file in [
        path.to_path_buf(),
        sidecar(path, "-wal"),
        sidecar(path, "-shm"),
    ] {
        assert!(file.exists(), "파일이 있어야 한다: {file:?}");
        assert_eq!(mode(&file), 0o600, "{file:?}");
    }
}

// 검증: D6
#[test]
fn 새로_만든_저장소_파일과_딸린_파일은_본인만_읽고_쓴다() {
    let (_dir, path, store) = temp_store();
    assert_eq!(mode(&path), 0o600);

    // 쓰고 나면 WAL의 딸린 파일이 있다. 그것들도 본인만 읽는다.
    store.append("chat", &json!({ "n": 1 })).unwrap();
    store.record_access(access(1)).unwrap();
    assert_all_private(&path);
}

// 검증: D6
#[test]
fn 이미_있는_파일의_권한이_더_넓으면_열_때_좁힌다() {
    for wide in [0o644, 0o640, 0o604, 0o660, 0o666, 0o755] {
        let (_dir, path, store) = temp_store();
        store.record_access(access(1)).unwrap();
        // 닫으면 딸린 파일은 사라지고 본 파일만 남는다.
        drop(store);
        set_mode(&path, wide);
        assert_eq!(mode(&path), wide);

        let store = Store::open(&path).unwrap();
        assert_eq!(mode(&path), 0o600, "{wide:o}에서 좁혀져야 한다");
        store.record_access(access(2)).unwrap();
        assert_all_private(&path);
        // 자료는 그대로다.
        assert_eq!(store.access_log().unwrap().len(), 2);
    }
}

// 검증: D6
#[test]
fn 남아_있던_딸린_파일의_권한도_열_때_좁힌다() {
    let (_dir, path, store) = temp_store();
    store.append("chat", &json!({ "n": 1 })).unwrap();
    store.record_access(access(1)).unwrap();
    // 닫지 못하고 꺼진 것처럼 둔다: 딸린 파일이 남는다.
    std::mem::forget(store);
    let files = [path.clone(), sidecar(&path, "-wal"), sidecar(&path, "-shm")];
    for file in &files {
        assert!(file.exists(), "{file:?}");
        set_mode(file, 0o644);
    }

    let store = Store::open(&path).unwrap();
    // 열자마자(다른 일을 하기 전에) 셋 다 좁혀져 있다.
    for file in &files {
        assert_eq!(mode(file), 0o600, "{file:?}");
    }
    assert_eq!(store.read_after(0, 10).unwrap().len(), 1);
    assert_eq!(store.access_log().unwrap().len(), 1);
}

// 검증: D6, V6
#[test]
fn 옛_버전_파일을_올릴_때도_권한을_좁힌다() {
    for fixture in [FIXTURE_V1, FIXTURE_V2] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("pi.sqlite");
        fs::copy(fixture, &path).unwrap();
        set_mode(&path, 0o644);

        let store = Store::open(&path).unwrap();
        assert_eq!(mode(&path), 0o600, "{fixture}");
        store.record_access(access(700)).unwrap();
        assert_all_private(&path);
        // 올린 뒤에도 앞선 자료가 읽힌다.
        assert_eq!(store.access_log().unwrap().len(), 3);
    }
}

// 검증: D6
#[test]
fn 다시_열어도_권한은_본인만_읽는_채로다() {
    let (_dir, path, store) = temp_store();
    store.record_access(access(1)).unwrap();
    drop(store);
    for n in 2..=4 {
        let store = Store::open(&path).unwrap();
        store.record_access(access(n)).unwrap();
        assert_all_private(&path);
        drop(store);
        assert_eq!(mode(&path), 0o600);
    }
}
