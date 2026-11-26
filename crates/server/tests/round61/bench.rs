//! 발판 시험: 이 폴더의 도우미가 지금 구현에서 서는가.

use price_inha::DB_FILE;

use crate::common::{Bench, count, in_session, seed};

// 검증: V78
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn 발판은_옛_기록을_심은_보관_폴더로_서버를_띄우고_읽고_끈다() {
    let now = in_session();
    let bench = Bench::start(now, |store| {
        let seeded = seed(store, now);
        assert_eq!(seeded.old_member, 30);
    })
    .await;
    let path = bench.home().join(DB_FILE);
    assert!(
        count(
            &path,
            "SELECT COUNT(*) FROM room_events WHERE kind = 'message.member'"
        ) >= 34
    );
    let reply = bench.get("/api/setup/doctor").await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    assert!(bench.store().info().is_ok());
    bench.close().await;
}
