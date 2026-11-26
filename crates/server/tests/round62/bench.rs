//! 발판 시험: 이 폴더의 도우미가 지금 구현에서 서는가.

use price_inha::DB_FILE;

use crate::common::{Bench, count, in_session, kst, today};

// 검증: V6
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn 발판은_시계를_옮기며_시연_서버를_띄우고_읽고_끈다() {
    let bench = Bench::start(in_session(), |_| {}).await;
    bench.set_clock(kst(today(), 10, 5));
    let path = bench.home().join(DB_FILE);
    assert!(count(&path, "SELECT COUNT(*) FROM paper_snapshots") >= 0);
    let reply = bench.get("/api/setup/doctor").await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    bench.close().await;
}
