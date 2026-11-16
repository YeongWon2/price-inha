//! 폰 잇기(T22): 푸시 공개키 · 구독(내부망 막기) · 알림 목록 · 승인 카드(V10) · 하루 정리(V12 · V50).

use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use pi_store::TradeState;
use price_inha::{
    DAILY_SUMMARY_PATH, DemoTurn, NOTICES_PATH, PUSH_KEY_PATH, PUSH_SUBSCRIBE_PATH,
    SUBSCRIBE_REFUSED,
};
use rust_decimal::Decimal;
use serde_json::json;

use crate::common::{
    AUTH, Bench, HYNIX, P256DH, events, get, kst, monday, post, subscribe, wait_for,
};

// 검증: T22 N5
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 푸시_공개키는_비밀_폴더에_남아_다시_켜도_같다() {
    let bench = Bench::start(kst(monday(), 9, 5)).await;
    let reply = get(bench.addr(), PUSH_KEY_PATH).await;
    assert_eq!(reply.status, 200, "{}", reply.text());
    let key = reply.body["key"].as_str().expect("공개키").to_string();
    let raw = URL_SAFE_NO_PAD.decode(&key).expect("base64url");
    assert_eq!(raw.len(), 65, "비압축 P-256 점");
    assert_eq!(raw[0], 4);
    let file = bench.home().join(pi_notify::VAPID_KEY_FILE);
    assert!(file.exists(), "VAPID 키는 비밀 폴더에 있다");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&file).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }
    let (dir, age) = bench.close().await;
    let again = Bench::with(dir, kst(monday(), 9, 6), Some(age)).await;
    let reply = get(again.addr(), PUSH_KEY_PATH).await;
    assert_eq!(reply.body["key"], key.as_str(), "다시 켜도 같은 키");
    again.close().await;
}

// 검증: T22 N5 E38
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 구독은_올바른_키와_바깥_https_주소만_받는다() {
    let bench = Bench::start(kst(monday(), 9, 5)).await;
    let addr = bench.addr();
    subscribe(addr, 1).await;
    let body = |endpoint: &str, p256dh: &str| json!({"endpoint": endpoint, "p256dh": p256dh, "auth": AUTH, "device": "폰"});
    let bad_key = post(
        addr,
        PUSH_SUBSCRIBE_PATH,
        &body("https://push.invalid/push/9", "AAAA"),
    )
    .await;
    assert_eq!(bad_key.status, 400);
    assert_eq!(bad_key.body["error"], "invalid_input");
    let plain = post(
        addr,
        PUSH_SUBSCRIBE_PATH,
        &body("http://push.invalid/push/9", P256DH),
    )
    .await;
    assert_eq!(plain.status, 400);
    assert_eq!(plain.body["error"], "invalid_input");
    for inside in [
        "https://localhost/push/1",
        "https://app.localhost/push/1",
        "https://printer.local/push/1",
        "https://127.0.0.1/push/1",
        "https://10.1.2.3/push/1",
        "https://192.168.0.7/push/1",
        "https://172.16.5.5/push/1",
        "https://169.254.169.254/push/1",
        "https://0.0.0.0/push/1",
        "https://[::1]/push/1",
        "https://[fd00::1]/push/1",
        "https://[fe80::1]/push/1",
    ] {
        let reply = post(addr, PUSH_SUBSCRIBE_PATH, &body(inside, P256DH)).await;
        assert_eq!(reply.status, 400, "{inside}는 내부망이다");
        assert_eq!(reply.body["error"], SUBSCRIBE_REFUSED, "{inside}");
    }
    let saved = bench.store().push_subscriptions().unwrap();
    assert_eq!(saved.len(), 1, "올바른 구독 하나만 남는다");
    bench.close().await;
}

// 검증: V10 C7 C12 N5 T22
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 한도를_넘는_주문은_폰에_알림이_가고_폰에서_승인한다() {
    let bench = Bench::start(kst(monday(), 10, 0)).await;
    subscribe(bench.addr(), 1).await;
    bench
        .ready_with(json!({"symbol_weight_pct": "10", "auto_order_limit_pct": "2"}))
        .await;
    let (trade, number, fingerprint) = bench.awaiting().await;
    let waited = bench.push.wait_count(1, Duration::from_secs(10)).await;
    assert!(waited < Duration::from_secs(10), "10초 안에 폰으로 간다");
    let notices = events(&bench.store(), "notice.approval_request");
    assert_eq!(notices.len(), 1, "{notices:?}");
    let link = notices[0]["link"].as_str().expect("주소");
    assert!(
        link.contains(&format!("/approvals/{}", number.as_str())),
        "알림 주소가 그 승인 카드를 연다: {link}"
    );
    let list = get(bench.addr(), &format!("{NOTICES_PATH}?after=0&max=50")).await;
    assert_eq!(list.status, 200);
    let items = list.body.as_array().expect("알림 목록");
    assert!(
        items
            .iter()
            .any(|n| n["kind"] == "approval_request" && n["link"] == link && n["seq"].is_u64()),
        "{items:?}"
    );

    let card = get(bench.addr(), &format!("/api/approvals/{}", number.as_str())).await;
    assert_eq!(card.status, 200, "{}", card.text());
    assert_eq!(card.body["fingerprint"], fingerprint.as_str());
    assert_eq!(card.body["short_code"], fingerprint.short_code());
    assert!(card.body["summary"].as_str().is_some_and(|s| !s.is_empty()));
    let path = format!("/api/approvals/{}/approve", number.as_str());
    let body = json!({"fingerprint": fingerprint.as_str(), "code": "123456"});
    let reply = post(bench.addr(), &path, &body).await;
    assert_eq!(reply.status, 200, "{}", reply.text());
    assert_eq!(reply.body["result"], "approved");
    assert_eq!(bench.trade_state(trade), TradeState::Ordered);
    bench.close().await;
}

// 검증: V10 C7 E7 E8
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 폰에서_거절하면_막히고_지문이_다르면_승인하지_않는다() {
    let bench = Bench::start(kst(monday(), 10, 0)).await;
    bench
        .ready_with(json!({"symbol_weight_pct": "10", "auto_order_limit_pct": "2"}))
        .await;
    let (first, number, fingerprint) = bench.awaiting().await;
    let reply = post(
        bench.addr(),
        &format!("/api/approvals/{}/reject", number.as_str()),
        &json!({"fingerprint": fingerprint.as_str()}),
    )
    .await;
    assert_eq!(reply.status, 200, "{}", reply.text());
    assert_eq!(reply.body["result"], "rejected");
    assert_eq!(reply.body["reason"], "user_rejected");
    assert_eq!(bench.trade_state(first), TradeState::Blocked);

    bench.set(bench.now() + chrono::Duration::seconds(5));
    let (second, number, _) = bench.awaiting().await;
    let wrong = "0".repeat(64);
    let reply = post(
        bench.addr(),
        &format!("/api/approvals/{}/approve", number.as_str()),
        &json!({"fingerprint": wrong}),
    )
    .await;
    assert_eq!(reply.status, 409, "승인을 눌렀는데 거절이면 409");
    assert_eq!(reply.body["result"], "rejected");
    assert_eq!(reply.body["reason"], "fingerprint_mismatch");
    assert_ne!(bench.trade_state(second), TradeState::Ordered);

    let missing = get(bench.addr(), "/api/approvals/없는번호").await;
    assert_eq!(missing.status, 404);
    let verb = post(
        bench.addr(),
        &format!("/api/approvals/{}/maybe", number.as_str()),
        &json!({}),
    )
    .await;
    assert_eq!(verb.status, 404);
    bench.close().await;
}

// 검증: V12 V50 T22 C7
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 장_마감_뒤_하루_정리에_한_일과_손익과_복기와_백업_시각이_보인다() {
    let bench = Bench::start(kst(monday(), 10, 0)).await;
    subscribe(bench.addr(), 1).await;
    bench.ready().await;
    bench.sold(HYNIX).await;
    let note = "공시 직후 따라 샀고 손절이 늦었다";
    let review = json!({"mistakes": ["late_stop"], "note": note}).to_string();
    bench.script(DemoTurn::Review, &review);
    let reviewed = bench.crew().review_day().await.expect("복기");
    assert_eq!(reviewed.reviews.len(), 1);

    bench.set(kst(monday(), 15, 40));
    wait_for("하루 정리 알림", || {
        !events(&bench.store(), "notice.daily_summary").is_empty()
    })
    .await;
    let report = bench.app.back_up_now().await.expect("백업");

    let summary = get(bench.addr(), DAILY_SUMMARY_PATH).await;
    assert_eq!(summary.status, 200, "{}", summary.text());
    let body = &summary.body;
    assert_eq!(body["date"], "2026-11-16");
    assert!(body["orders"].as_u64().is_some_and(|n| n >= 2), "{body}");
    let pnl: Decimal = body["pnl_krw"].as_str().expect("손익").parse().unwrap();
    assert!(pnl < Decimal::ZERO, "손절로 잃었다: {pnl}");
    assert_eq!(body["reviews"], 1);
    for part in ["done", "pnl", "review"] {
        assert!(
            body[part].as_str().is_some_and(|s| !s.is_empty()),
            "{part}가 한 화면에 있다: {body}"
        );
    }
    assert!(body["review"].as_str().unwrap().contains(note));
    assert_eq!(body["backup_at"], report.at.0, "마지막 백업 시각");
    assert!(body["done"].as_str().unwrap().contains("백업 완료"));
    bench.close().await;
}
