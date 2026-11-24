//! 29차 1번: 날짜별 하루 정리(C30 · V84 · R4-3 · F7-9).

use price_inha::DAILY_SUMMARY_PATH;
use serde_json::{Value, json};

use crate::common::{Bench, HYNIX, get, kst, monday};

/// 하루 정리를 읽는다(200이어야 한다).
async fn summary(bench: &Bench, query: &str) -> Value {
    let reply = get(bench.addr(), &format!("{DAILY_SUMMARY_PATH}{query}")).await;
    assert_eq!(reply.status, 200, "{query}: {}", reply.body);
    reply.body
}

// 검증: V84 C30 R4-3 F7
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 날짜를_고르면_그날의_하루_정리이고_오늘은_날짜_없이와_같다() {
    let bench = Bench::start(kst(monday(), 9, 5)).await;
    bench.ready().await;
    bench.sold(HYNIX).await;

    let plain = summary(&bench, "").await;
    assert_eq!(plain["date"], json!("2026-11-16"), "{plain}");
    assert!(
        plain["orders"].as_u64().is_some_and(|n| n >= 2),
        "오늘 사고 팔았다: {plain}"
    );
    let today = summary(&bench, "?date=2026-11-16").await;
    assert_eq!(today, plain, "오늘 날짜는 날짜 없이와 같다");

    let friday = summary(&bench, "?date=2026-11-13").await;
    assert_eq!(friday["date"], json!("2026-11-13"), "고른 날짜: {friday}");
    assert_eq!(friday["orders"], json!(0), "그날 체결은 없다: {friday}");
    assert_eq!(friday["reviews"], json!(0), "그날 복기는 없다: {friday}");
    assert_eq!(
        friday["pnl_krw"],
        Value::Null,
        "그날 시작 평가액은 없다: {friday}"
    );
    for part in ["done", "pnl", "review"] {
        assert!(
            friday[part].as_str().is_some_and(|s| !s.is_empty()),
            "{part}가 있다: {friday}"
        );
    }
    assert_eq!(
        friday["backup_at"], plain["backup_at"],
        "마지막 백업은 날짜와 상관없다"
    );
    assert_ne!(friday["done"], plain["done"], "그날의 한 일: {friday}");
    bench.close().await;
}

// 검증: C30 E57
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 틀린_날짜_글자는_400이다() {
    let bench = Bench::start(kst(monday(), 9, 5)).await;
    for date in [
        "2026-02-30",
        "2026/11/16",
        "20261116",
        "yesterday",
        "2026-13-01",
    ] {
        let reply = get(bench.addr(), &format!("{DAILY_SUMMARY_PATH}?date={date}")).await;
        assert_eq!(reply.status, 400, "{date}: {}", reply.body);
        assert_eq!(reply.body["error"], json!("invalid_input"), "{date}");
    }
    let plain = get(bench.addr(), DAILY_SUMMARY_PATH).await;
    assert_eq!(plain.status, 200, "날짜 없이는 그대로: {}", plain.body);
    bench.close().await;
}
