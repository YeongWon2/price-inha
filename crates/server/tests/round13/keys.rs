//! 다른 기기의 키 입력(`/keys` 화면 · `POST /api/remote/keys`)의 HTTP 경로.

use price_inha::{KEYS_PAGE_PATH, REMOTE_KEYS_PATH};
use serde_json::json;

use crate::common::{Bench, fake_ai, get_with, login, origin_of, post_with};

/// 시험용 AI API 키(실제 키가 아니다).
const AI_KEY: &str = "test-ai-key-round13-remote-zz";

fn ai_key_body() -> serde_json::Value {
    json!({"item": "ai_api_key", "fields": {"api_key": AI_KEY}})
}

// 검증: C16
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 키_화면과_다른_기기_키_등록은_세션이_없으면_401이다() {
    let ai = fake_ai().await;
    let bench = Bench::start(&ai.uri()).await;
    let addr = bench.addr();
    let page = get_with(addr, KEYS_PAGE_PATH, None).await;
    assert_eq!(page.status, 401, "{}", page.text());
    let refused = post_with(
        addr,
        REMOTE_KEYS_PATH,
        None,
        &origin_of(addr),
        &ai_key_body(),
    )
    .await;
    assert_eq!(refused.status, 401, "{}", refused.text());
    assert!(!refused.text().contains(AI_KEY));
    assert!(
        ai.received_requests().await.unwrap_or_default().is_empty(),
        "확인기를 부르지 않는다"
    );
    bench.close().await;
}

// 검증: C3 E4
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 다른_출처에서_보낸_다른_기기_키_등록은_거부한다() {
    let ai = fake_ai().await;
    let bench = Bench::start(&ai.uri()).await;
    let addr = bench.addr();
    let cookie = login(addr).await;
    let elsewhere = format!("http://127.0.0.1:{}", addr.port().wrapping_add(1));
    let refused = post_with(
        addr,
        REMOTE_KEYS_PATH,
        Some(&cookie),
        &elsewhere,
        &ai_key_body(),
    )
    .await;
    assert!(
        (400..500).contains(&refused.status),
        "다른 출처는 거부한다: {} {}",
        refused.status,
        refused.text()
    );
    assert!(!refused.text().contains(AI_KEY));
    assert!(
        ai.received_requests().await.unwrap_or_default().is_empty(),
        "확인기를 부르지 않는다"
    );
    bench.close().await;
}

// 검증: C16
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 세션이면_키_화면이_열리고_다른_기기_키가_등록되며_답에_값이_없다() {
    let ai = fake_ai().await;
    let bench = Bench::start(&ai.uri()).await;
    let addr = bench.addr();
    let cookie = login(addr).await;
    let page = get_with(addr, KEYS_PAGE_PATH, Some(&cookie)).await;
    assert_eq!(page.status, 200, "{}", page.text());
    // 11차(Vite 이전): 스크립트가 묶음으로 나가므로 받은 화면 뒤에 그 쪽이 부르는 묶음을 이어 본다.
    let text = crate::screen::served_page(addr, &page.text()).await;
    assert!(text.contains(REMOTE_KEYS_PATH), "화면이 등록 경로를 부른다");
    let saved = post_with(
        addr,
        REMOTE_KEYS_PATH,
        Some(&cookie),
        &origin_of(addr),
        &ai_key_body(),
    )
    .await;
    assert_eq!(saved.status, 200, "{}", saved.text());
    assert_eq!(saved.body, json!({"registered": true}), "답은 등록 여부뿐");
    assert!(
        !saved.text().contains(AI_KEY),
        "키 값을 다시 보여 주지 않는다"
    );
    assert_eq!(
        ai.received_requests().await.unwrap_or_default().len(),
        1,
        "확인을 한 번 한다"
    );
    bench.close().await;
}
