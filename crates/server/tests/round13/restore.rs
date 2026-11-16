//! 되살리기 중 꺼져 남은 작업 폴더(`<이름>.restore-work`)는 다음 되살리기 · 켤 때 지운다.

use pi_store::UnixMillis;

use crate::common::{AgeKeys, Bench, dead_base_url};

/// `parent` 안에 `home` 옆의 남은 작업 폴더(안에 파일 하나)를 만든다.
fn leftover(parent: &std::path::Path) -> std::path::PathBuf {
    let work = parent.join("home.restore-work");
    std::fs::create_dir_all(work.join("state")).unwrap();
    std::fs::write(work.join("state").join("pi.db"), b"partial").unwrap();
    work
}

// 검증: V51
#[test]
fn 남은_작업_폴더는_다음_되살리기가_지운다() {
    let parent = tempfile::tempdir().unwrap();
    let target = parent.path().join("home");
    let work = leftover(parent.path());
    let age = AgeKeys::generate();
    let restored =
        price_inha::restore_backup(b"not-a-backup", &age.identity, &target, UnixMillis(0));
    assert!(restored.is_err(), "풀 수 없는 것은 되살리지 않는다");
    assert!(!work.exists(), "남은 작업 폴더를 지운다");
}

// 검증: V51
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 남은_작업_폴더는_켤_때_지운다() {
    let parent = tempfile::tempdir().unwrap();
    let home = parent.path().join("home");
    let work = leftover(parent.path());
    let bench = Bench::at(&home, &dead_base_url()).await;
    assert!(!work.exists(), "켤 때 남은 작업 폴더를 지운다");
    assert!(home.exists(), "보관 폴더는 그대로다");
    bench.close().await;
}
