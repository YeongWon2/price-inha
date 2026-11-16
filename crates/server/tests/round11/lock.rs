//! 접속 잠금 알림(E2 · V39): 다른 기기가 접속 키를 5번 틀려 잠기면 폰에 알림이 간다.

use std::sync::atomic::Ordering;
use std::time::Duration;

use serde_json::json;

use crate::common::{Bench, events, kst, monday, origin_of, send, subscribe};

// 검증: E2 V39 N5
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 접속_키를_5번_틀려_잠기면_잠금_알림이_간다() {
    let bench = Bench::start(kst(monday(), 9, 5)).await;
    subscribe(bench.addr(), 1).await;
    bench.remote.store(true, Ordering::SeqCst);
    let origin = origin_of(bench.addr());
    let mut statuses = Vec::new();
    for _ in 0..pi_gate::MAX_KEY_FAILURES + 1 {
        let request = reqwest::Client::new()
            .post(format!("{origin}{}", pi_gate::SESSION_PATH))
            .header("Origin", &origin)
            .header("Content-Type", "application/json")
            .body(json!({ pi_gate::ACCESS_KEY_FIELD: "0000000000000000000Z" }).to_string());
        statuses.push(send(request).await.status);
    }
    bench.remote.store(false, Ordering::SeqCst);
    assert_eq!(
        statuses.last(),
        Some(&pi_gate::LOCKED_STATUS),
        "5번 틀리면 잠긴다: {statuses:?}"
    );
    let waited = bench.push.wait_count(1, Duration::from_secs(10)).await;
    assert!(waited < Duration::from_secs(10));
    let notices = events(&bench.store(), "notice.access_lock");
    assert_eq!(notices.len(), 1, "잠금 한 번에 알림 하나: {notices:?}");
    let text = notices[0].to_string();
    assert!(text.contains("100.64.0.7"), "잠긴 기기가 보인다: {text}");
    bench.close().await;
}
