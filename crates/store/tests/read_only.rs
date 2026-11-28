//! store 23차 시험(백로그 U 2번 "doctor 읽기 전용 손잡이"): 있는 저장소 파일을 **읽기만** 하는 손잡이
//! [`Store::open_read_only`]. 판이 낮아도 올리지 않고(그 사실은 [`Store::info`]의 판으로 알린다), 파일 · 곁 파일 ·
//! 권한을 바꾸지 않으며, 없는 파일은 만들지 않는다. 도는 서버(쓰는 손잡이)와 함께 열어도 서로 막지 않는다.
//! (23차) 2026-11-28 백로그 U를 합쳐 잠금.

use std::fs;
use std::path::{Path, PathBuf};

use pi_store::{SCHEMA_VERSION, Store, StoreError};
use serde_json::json;

fn copy_of_v11() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("pi.sqlite");
    fs::copy(
        concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/v11.db"),
        &path,
    )
    .unwrap();
    (dir, path)
}

/// 폴더 안 파일들의 (이름, 내용) — 바뀌었는지 본다.
fn snapshot(dir: &Path) -> Vec<(String, Vec<u8>)> {
    let mut files: Vec<(String, Vec<u8>)> = fs::read_dir(dir)
        .unwrap()
        .map(|entry| {
            let entry = entry.unwrap();
            let name = entry.file_name().to_string_lossy().into_owned();
            (name, fs::read(entry.path()).unwrap())
        })
        .collect();
    files.sort();
    files
}

#[cfg(unix)]
fn mode(path: &Path) -> u32 {
    use std::os::unix::fs::PermissionsExt;
    fs::metadata(path).unwrap().permissions().mode() & 0o777
}

// 검증: D6
#[test]
fn 판_11_파일을_읽기_전용으로_열면_판을_올리지_않고_파일도_바꾸지_않는다() {
    let (dir, path) = copy_of_v11();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
    }
    let before = snapshot(dir.path());
    {
        let store = Store::open_read_only(&path).expect("판 11 파일을 연다");
        let info = store.info().unwrap();
        assert_eq!(info.schema_version, 11, "판을 올리지 않고 지금 판을 알린다");
        assert!(info.schema_version < SCHEMA_VERSION);
        assert!(
            store.integrity_ok().unwrap(),
            "무결성 검사를 읽기만으로 한다"
        );
        let _ = store.team_state().unwrap();
        let _ = store.access_key().unwrap();
        let written = store.append("office.agent", &json!({"x": 1}));
        assert!(written.is_err(), "읽기 전용 손잡이로는 쓰지 못한다");
    }
    assert_eq!(snapshot(dir.path()), before, "파일 · 곁 파일이 그대로다");
    #[cfg(unix)]
    assert_eq!(mode(&path), 0o644, "권한도 좁히지 않는다");
}

// 검증: D6
#[test]
fn 없는_파일은_만들지_않고_이_저장소의_파일이_아니면_거부한다() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("없음.sqlite");
    let result = Store::open_read_only(&missing);
    assert!(
        matches!(result, Err(StoreError::FileAccess { .. })),
        "없는 파일은 파일 오류"
    );
    assert!(!missing.exists(), "없는 파일을 만들지 않는다");
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 0);

    let other = dir.path().join("other.sqlite");
    let conn = rusqlite::Connection::open(&other).unwrap();
    conn.execute_batch("CREATE TABLE t(x INTEGER);").unwrap();
    drop(conn);
    assert!(matches!(
        Store::open_read_only(&other),
        Err(StoreError::Incompatible(_))
    ));
}

// 검증: D6
#[test]
fn 쓰는_손잡이가_쓰는_중에도_읽기_전용_손잡이는_막지도_막히지도_않는다() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("pi.sqlite");
    let writer = Store::open(&path).unwrap();
    writer.append("office.agent", &json!({"n": 0})).unwrap();
    let busy = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
    let worker = {
        let writer = writer.clone();
        let busy = busy.clone();
        std::thread::spawn(move || {
            let mut n = 0;
            while busy.load(std::sync::atomic::Ordering::SeqCst) {
                n += 1;
                writer
                    .append("office.agent", &json!({ "n": n }))
                    .expect("쓰는 손잡이는 막히지 않는다");
            }
            n
        })
    };
    for _ in 0..20 {
        let reader = Store::open_read_only(&path).expect("쓰는 중에도 연다");
        assert_eq!(reader.info().unwrap().schema_version, SCHEMA_VERSION);
        assert!(reader.integrity_ok().unwrap());
        assert!(reader.read_after(0, 1).is_ok());
    }
    busy.store(false, std::sync::atomic::Ordering::SeqCst);
    assert!(worker.join().unwrap() > 0);
}
