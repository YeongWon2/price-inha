//! 장 마감 뒤에 뜬 서버는 보관을 스스로 한 번 돈다(하루 한 번, 하루 정리 뒤).

use std::time::{Duration, Instant};

use price_inha::{ARCHIVE_DIR, DB_FILE, SETUP_STORAGE_PATH};

use crate::common::{Bench, STEP, after_close, count, in_session, seed};

// 검증: D6 M1
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn 장_마감_뒤에_뜬_서버는_보관을_스스로_돌아_행이_준다() {
    let now = after_close();
    let bench = Bench::start(now, |store| {
        seed(store, now);
    })
    .await;
    let store = bench.store();
    let started = Instant::now();
    while store.archive_batches().unwrap().is_empty() {
        assert!(started.elapsed() < STEP, "보관이 시간 안에 돌아야 한다");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let path = bench.home().join(DB_FILE);
    assert_eq!(
        count(
            &path,
            "SELECT COUNT(*) FROM events WHERE kind = 'rules_config'"
        ),
        3
    );
    assert!(bench.home().join(ARCHIVE_DIR).is_dir());
    let reply = bench.get(SETUP_STORAGE_PATH).await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    bench.close().await;
}

// 검증: D6
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn 장중에_뜬_서버는_보관을_돌지_않는다() {
    let now = in_session();
    let bench = Bench::start(now, |store| {
        seed(store, now);
    })
    .await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert!(bench.store().archive_batches().unwrap().is_empty());
    assert!(!bench.home().join(ARCHIVE_DIR).exists());
    bench.close().await;
}
