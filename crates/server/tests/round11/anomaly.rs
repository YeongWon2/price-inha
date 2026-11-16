//! 이상 알림(V11 · N5 · N6): 시세가 끊기면 10초 안에 폰에 이상 알림이 간다.
//!
//! 시연 시세는 끊을 수 없으므로 로컬 가짜 토스증권(토큰 · 현재가 · 국내 달력)으로 실제 시세 길을 띄우고,
//! 현재가를 장애로 바꾼 뒤 시계를 30초 넘게 옮긴다.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use chrono::{DateTime, NaiveDate, Utc};
use price_inha::{App, Launch, Reach, TradingWiring};
use serde_json::{Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

use crate::common::{
    PushServer, STEP, TestClock, WATCHED, events, kst, register_required, settings, subscribe,
    wiring,
};

fn day() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 3, 25).unwrap()
}

fn at(hour: u32, minute: u32, second: i64) -> DateTime<Utc> {
    kst(day(), hour, minute) + chrono::Duration::seconds(second)
}

fn kr_open_day(date: &str) -> Value {
    json!({
        "date": date,
        "integrated": {
            "preMarket": {
                "startTime": format!("{date}T08:00:00+09:00"),
                "singlePriceAuctionStartTime": format!("{date}T08:50:00+09:00"),
                "endTime": format!("{date}T09:00:00+09:00"),
            },
            "regularMarket": {
                "startTime": format!("{date}T09:00:00+09:00"),
                "singlePriceAuctionStartTime": format!("{date}T15:20:00+09:00"),
                "endTime": format!("{date}T15:30:00+09:00"),
            },
            "afterMarket": {
                "startTime": format!("{date}T15:30:00+09:00"),
                "singlePriceAuctionEndTime": format!("{date}T15:40:00+09:00"),
                "endTime": format!("{date}T20:00:00+09:00"),
            },
        }
    })
}

/// 가짜 토스증권: 토큰 · 현재가(`failing`이면 503) · 국내 달력.
async fn fake_toss(failing: Arc<AtomicBool>) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/oauth2/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "access_token": "test-toss-token-not-real", "token_type": "Bearer", "expires_in": 86_400
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v1/prices"))
        .respond_with(move |_: &Request| {
            if failing.load(Ordering::SeqCst) {
                return ResponseTemplate::new(503).set_body_json(json!({
                    "error": {"requestId": "r-1", "code": "maintenance", "message": "점검"}
                }));
            }
            ResponseTemplate::new(200).set_body_json(json!({"result": [{
                "symbol": WATCHED, "timestamp": "2026-03-25T10:00:00.000+09:00",
                "lastPrice": "72000", "currency": "KRW"
            }]}))
        })
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v1/market-calendar/KR"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"result": {
            "today": kr_open_day("2026-03-25"),
            "previousBusinessDay": kr_open_day("2026-03-24"),
            "nextBusinessDay": kr_open_day("2026-03-26"),
        }})))
        .mount(&server)
        .await;
    server
}

async fn price_requests(toss: &MockServer) -> usize {
    let received = toss.received_requests().await.unwrap_or_default();
    received
        .iter()
        .filter(|r| r.url.path() == "/api/v1/prices")
        .count()
}

// 검증: V11 N5 N6 T22
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 시세가_끊기면_10초_안에_폰에_이상_알림이_간다() {
    let dir = tempfile::tempdir().unwrap();
    register_required(dir.path()).await;
    let failing = Arc::new(AtomicBool::new(false));
    let toss = fake_toss(Arc::clone(&failing)).await;
    let push = PushServer::start().await;
    let clock = TestClock::at(at(10, 0, 0));
    let wiring = wiring(
        Arc::clone(&clock),
        Some(toss.uri()),
        Arc::new(AtomicBool::new(false)),
    );
    let launch = Launch {
        trading: TradingWiring::default(),
        team: None,
        secrets: None,
    };
    let reach = Reach {
        push: Some(push.transport()),
        backup: None,
    };
    let start = App::launch_with(settings(dir.path(), WATCHED, false), wiring, launch, reach);
    let app = tokio::time::timeout(STEP, start).await.unwrap().unwrap();
    subscribe(app.local_addr(), 1).await;
    let deadline = std::time::Instant::now() + STEP;
    while price_requests(&toss).await == 0 {
        assert!(
            std::time::Instant::now() < deadline,
            "시세를 받기 시작해야 한다"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(
        events(&app.store(), "notice.anomaly").is_empty(),
        "아직 이상이 없다"
    );

    failing.store(true, Ordering::SeqCst);
    tokio::time::sleep(Duration::from_millis(100)).await;
    clock.set(at(10, 0, 31));
    let waited = push.wait_count(1, Duration::from_secs(10)).await;
    assert!(
        waited < Duration::from_secs(10),
        "끊긴 뒤 10초 안에 폰으로 간다"
    );
    let notices = events(&app.store(), "notice.anomaly");
    assert_eq!(notices.len(), 1, "{notices:?}");
    let text = format!("{} {}", notices[0]["title"], notices[0]["body"]);
    assert!(text.contains(WATCHED), "늦은 종목이 보인다: {text}");

    // 늦음이 이어지는 동안 같은 알림을 되풀이하지 않는다.
    clock.set(at(10, 0, 45));
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!(events(&app.store(), "notice.anomaly").len(), 1);
    tokio::time::timeout(STEP, app.shutdown())
        .await
        .unwrap()
        .unwrap();
}
