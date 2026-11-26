//! 키 화면(`/keys`)의 관문: 그 컴퓨터(루프백 · 전달 머리글 없음)는 세션 없이 열리고 화면이 세션을 얻는다.
//! 다른 기기 · 전달 머리글이 있는 요청은 지금처럼 원격 세션이 있어야 한다. 값을 다루는 길(키 등록 ·
//! 등록 여부 읽기)의 관문은 그대로다.

use price_inha::{KEYS_PAGE_PATH, REMOTE_KEYS_PATH, SETUP_KEYS_PATH, SETUP_SLOTS_PATH};
use serde_json::json;

use crate::common::{Bench, FORWARDED_FOR, fake_ai, get, origin_of, post, send, session_cookie};

/// 시험용 AI API 키(실제 키가 아니다).
const AI_KEY: &str = "test-ai-key-round56-keys-zz";

fn ai_key_body() -> serde_json::Value {
    json!({"item": "ai_api_key", "fields": {"api_key": AI_KEY}})
}

// 검증: C15 C16
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 그_컴퓨터에서는_세션_없이_키_화면이_열리고_화면이_세션을_얻는다() {
    let ai = fake_ai().await;
    let bench = Bench::start(&ai.uri()).await;
    let addr = bench.addr();
    let page = get(addr, KEYS_PAGE_PATH, None, false).await;
    assert_eq!(page.status, 200, "{}", page.text());
    assert!(
        page.content_type.starts_with("text/html"),
        "{}",
        page.content_type
    );
    assert!(!page.sets_cookie, "화면(HTML)을 내줄 때 세션을 주지 않는다");
    // 화면이 부르는 묶음: 등록 경로와 세션 경로를 부른다.
    let text = crate::screen_served::served_page(addr, &page.text()).await;
    assert!(text.contains(REMOTE_KEYS_PATH), "화면이 등록 경로를 부른다");
    assert!(text.contains(pi_gate::SESSION_PATH), "화면이 세션을 얻는다");
    // 화면만 열렸을 뿐 값을 다루는 길은 세션이 있어야 한다.
    let slots = get(addr, SETUP_SLOTS_PATH, None, false).await;
    assert_eq!(slots.status, 401, "{}", slots.text());
    // 화면이 하듯 본문 없이 세션을 청하면 그 컴퓨터 세션을 받고, 그 세션으로 등록 여부를 읽는다.
    let cookie = session_cookie(addr, None)
        .await
        .expect("그 컴퓨터는 키 없이 세션을 받는다");
    let slots = get(addr, SETUP_SLOTS_PATH, Some(&cookie), false).await;
    assert_eq!(slots.status, 200, "{}", slots.text());
    bench.close().await;
}

// 검증: C16
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 전달_머리글이_있으면_세션_없이_키_화면은_401이다() {
    let ai = fake_ai().await;
    let bench = Bench::start(&ai.uri()).await;
    let addr = bench.addr();
    let page = get(addr, KEYS_PAGE_PATH, None, true).await;
    assert_eq!(page.status, 401, "{}", page.text());
    assert!(!page.text().contains("키 등록"), "화면을 내주지 않는다");
    bench.close().await;
}

// 검증: C1 C16
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 다른_기기는_세션_없이_키_화면이_401이고_원격_세션이면_열린다() {
    let ai = fake_ai().await;
    let bench = Bench::start(&ai.uri()).await;
    let addr = bench.addr();
    bench.go_remote();
    let page = get(addr, KEYS_PAGE_PATH, None, false).await;
    assert_eq!(page.status, 401, "{}", page.text());
    assert!(
        session_cookie(addr, None).await.is_none(),
        "다른 기기는 접속 키 없이 세션을 받지 못한다"
    );
    let cookie = bench.remote_login().await;
    let page = get(addr, KEYS_PAGE_PATH, Some(&cookie), false).await;
    assert_eq!(page.status, 200, "{}", page.text());
    bench.close().await;
}

// 검증: C3 C16 E4
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 키_화면을_세션_없이_열어도_키_등록_길의_관문은_그대로다() {
    let ai = fake_ai().await;
    let bench = Bench::start(&ai.uri()).await;
    let addr = bench.addr();
    let body = ai_key_body();
    // 그 컴퓨터: 화면을 연 뒤에도 세션 없이는 두 등록 길 모두 401.
    let page = get(addr, KEYS_PAGE_PATH, None, false).await;
    assert_eq!(page.status, 200, "{}", page.text());
    for path in [SETUP_KEYS_PATH, REMOTE_KEYS_PATH] {
        let refused = post(addr, path, None, &body).await;
        assert_eq!(refused.status, 401, "{path}: {}", refused.text());
        assert!(!refused.text().contains(AI_KEY));
    }
    // 전달 머리글이 있는 요청: 세션 없이 401.
    let forwarded = send(
        reqwest::Client::new()
            .post(format!("{}{REMOTE_KEYS_PATH}", origin_of(addr)))
            .header("Origin", origin_of(addr))
            .header("Content-Type", "application/json")
            .header("X-Forwarded-For", FORWARDED_FOR)
            .body(body.to_string()),
    )
    .await;
    assert_eq!(forwarded.status, 401, "{}", forwarded.text());
    // 다른 기기: 원격 세션이 있어도 그 컴퓨터 전용 등록 길은 닫혀 있다.
    let cookie = bench.remote_login().await;
    let local_only = post(addr, SETUP_KEYS_PATH, Some(&cookie), &body).await;
    assert_eq!(
        local_only.status,
        pi_gate::LOCAL_ONLY_STATUS,
        "{}",
        local_only.text()
    );
    assert!(!local_only.text().contains(AI_KEY));
    assert!(
        ai.received_requests().await.unwrap_or_default().is_empty(),
        "확인기를 부르지 않는다"
    );
    bench.close().await;
}
