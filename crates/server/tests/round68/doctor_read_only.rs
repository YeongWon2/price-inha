//! doctor 읽기 전용 손잡이(server 9차 보강 4 "다음 차수 후보" · 백로그 U 2번): 진단은 저장소를 store의 읽기 전용
//! 손잡이로, 백업 상태를 backup의 읽기 전용 상태 읽기로 본다. 판 11 파일에 진단을 돌려도 보관 폴더의 어떤 파일도
//! 바뀌지 않고(내용 해시가 같고 곁 파일도 생기지 않는다), 쓰는 손잡이(도는 서버)가 쓰는 중에도 잠금 다툼이 없다.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use pi_store::Store;
use price_inha::{BACKUP_SETTINGS_FILE, CheckItem, DB_FILE, DoctorReport, Os, Service, doctor};
use serde_json::json;

use crate::common::{ScriptedRunner, plain_settings, setup};

const FIXTURE_V11: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../store/tests/fixtures/v11.db"
);

/// 폴더 아래 모든 파일의 (경로, 내용 해시).
fn hashes(dir: &Path) -> Vec<(String, u64)> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(next) = stack.pop() {
        for entry in std::fs::read_dir(&next).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            let mut hasher = DefaultHasher::new();
            std::fs::read(&path).unwrap().hash(&mut hasher);
            let name = path.strip_prefix(dir).unwrap().display().to_string();
            out.push((name, hasher.finish()));
        }
    }
    out.sort();
    out
}

fn run_doctor(home: &Path) -> DoctorReport {
    let defs = tempfile::tempdir().unwrap();
    let runner = ScriptedRunner::ok();
    let service = Service::new(setup(Os::Linux, home, defs.path(), &[]), &runner);
    doctor(&plain_settings(home), &service)
}

/// 백업을 켜 둔 설정과 백업기가 남긴 모양의 상태 파일.
fn backup_configured(home: &Path) {
    std::fs::write(
        home.join(BACKUP_SETTINGS_FILE),
        json!({ "repo": "owner/name", "recipient": "age1example" }).to_string(),
    )
    .unwrap();
    std::fs::create_dir_all(home.join("backups")).unwrap();
    std::fs::write(
        home.join("backups").join("backup-status.json"),
        json!({
            "last_success": 1_794_900_000_000_i64,
            "last_failure": { "at": 1_794_990_000_000_i64, "kind": "upload" },
            "retry_pending": true,
        })
        .to_string(),
    )
    .unwrap();
}

// 검증: D6 E15
#[test]
fn 판_11_저장소에_진단을_돌려도_보관_폴더의_파일이_바뀌지_않는다() {
    let home = tempfile::tempdir().unwrap();
    std::fs::copy(FIXTURE_V11, home.path().join(DB_FILE)).unwrap();
    backup_configured(home.path());
    let before = hashes(home.path());

    let report = run_doctor(home.path());
    let database = report.check(CheckItem::Database).unwrap();
    assert!(database.ok, "{database:?}");
    assert!(
        database.what.contains("스키마 11"),
        "판을 올리지 않고 지금 판을 알린다: {database:?}"
    );
    let backup = report.check(CheckItem::Backup).unwrap();
    assert!(!backup.ok, "다시 올리기를 기다리면 ✗: {backup:?}");
    assert!(backup.what.contains("마지막 백업"), "{backup:?}");

    assert_eq!(
        hashes(home.path()),
        before,
        "어떤 파일도 바뀌거나 생기지 않는다"
    );
    let version: i64 = rusqlite::Connection::open(home.path().join(DB_FILE))
        .unwrap()
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap();
    assert_eq!(version, 11);
}

// 검증: D6
#[test]
fn 도는_서버가_쓰는_중에도_진단은_잠금_다툼_없이_저장소를_본다() {
    let home = tempfile::tempdir().unwrap();
    let server = Store::open(home.path().join(DB_FILE)).unwrap();
    server.append("office.agent", &json!({ "n": 0 })).unwrap();
    let busy = Arc::new(AtomicBool::new(true));
    let worker = {
        let server = server.clone();
        let busy = busy.clone();
        std::thread::spawn(move || {
            let mut n = 0_u64;
            while busy.load(Ordering::SeqCst) {
                n += 1;
                server
                    .append("office.agent", &json!({ "n": n }))
                    .expect("서버의 쓰기는 진단에 막히지 않는다");
            }
            n
        })
    };
    for _ in 0..10 {
        let report = run_doctor(home.path());
        let database = report.check(CheckItem::Database).unwrap();
        assert!(database.ok, "{database:?}");
        assert!(
            database.what.starts_with("저장소 정상"),
            "쓰는 중이라 확인하지 못했다가 아니라 본다: {database:?}"
        );
    }
    busy.store(false, Ordering::SeqCst);
    assert!(worker.join().unwrap() > 0);
}
