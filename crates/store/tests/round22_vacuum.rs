#![cfg(test)]
//! 22차: 큰 판 11 파일을 판 12로 열고 처음 공간을 돌려받을 때(자동 정리 방식을 바꾸는 한 번의 전체 정리)
//! 걸리는 시간을 잰다. 10만 줄에 각각 30초 안이어야 한다(넉넉한 상한 — 재 본 값은 출력에 남긴다).

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use pi_store::Store;

const ROWS: i64 = 100_000;
const LIMIT: Duration = Duration::from_secs(30);

/// 시험 안에서 만든 큰 판 11 파일: 화면 이벤트 10만 줄(줄마다 약 300바이트).
fn big_v11() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("pi.sqlite");
    fs::copy(
        concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/v11.db"),
        &path,
    )
    .unwrap();
    let mut conn = rusqlite::Connection::open(&path).unwrap();
    let tx = conn.transaction().unwrap();
    {
        let mut insert = tx
            .prepare("INSERT INTO events (kind, payload, at) VALUES ('office.progress', ?1, ?2)")
            .unwrap();
        let pad = "가".repeat(90);
        for i in 0..ROWS {
            let payload = format!("{{\"agent\":\"scout\",\"step\":{i},\"pad\":\"{pad}\"}}");
            insert
                .execute(rusqlite::params![payload, 1_790_000_000_000_i64 + i * 1000])
                .unwrap();
        }
    }
    tx.commit().unwrap();
    (dir, path)
}

fn pragma(path: &Path, name: &str) -> i64 {
    let conn = rusqlite::Connection::open(path).unwrap();
    conn.query_row(&format!("PRAGMA {name}"), [], |r| r.get(0))
        .unwrap()
}

// 검증: D6
#[test]
fn 큰_판_11_파일을_열고_처음_돌려받기까지_각각_30초_안이다() {
    let (_dir, path) = big_v11();
    assert_eq!(pragma(&path, "user_version"), 11);
    let file_bytes = fs::metadata(&path).unwrap().len();

    let started = Instant::now();
    let store = Store::open(&path).unwrap();
    let opened = started.elapsed();
    assert_eq!(pragma(&path, "user_version"), 12);

    let started = Instant::now();
    let report = store.reclaim_space(8192).unwrap();
    let reclaimed = started.elapsed();
    assert!(report.converted, "판 12 전 파일은 처음 한 번 바꾼다");
    assert_eq!(pragma(&path, "auto_vacuum"), 2);

    eprintln!(
        "판 11 → 12: 줄 {ROWS} · 파일 {file_bytes} 바이트 · 열기 {opened:?} · 처음 돌려받기(VACUUM) {reclaimed:?}"
    );
    assert!(opened < LIMIT, "열기 {opened:?}");
    assert!(reclaimed < LIMIT, "처음 돌려받기 {reclaimed:?}");

    let started = Instant::now();
    let second = store.reclaim_space(8192).unwrap();
    assert!(!second.converted);
    assert!(started.elapsed() < LIMIT);
}
