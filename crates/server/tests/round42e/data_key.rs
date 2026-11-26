//! 2번: 옛 "자료 키"(`item: "data"`, `KeyKind::Data`)는 확인할 곳이 없어 등록이 늘 실패한다. 서버는 지금처럼 거절하되
//! 까닭을 분명히 한다(`retired_item`, 확인기를 부르지 않는다). 설정 화면의 진단 · `price-inha doctor`의 키 줄에서는
//! 그 항목이 보이지 않는다. 설정 상태(`GET /api/setup`)의 항목 넷(`SETUP_ITEMS`)은 그대로다(API 모양 유지).

use price_inha::{
    ITEM_DATA, KEY_DATA, REMOTE_KEYS_PATH, SETUP_DOCTOR_PATH, SETUP_ITEMS, SETUP_KEYS_PATH,
    SetupRefusal,
};
use serde_json::json;

use crate::common::{Running, get, keys_of, post};
use crate::doctor_cli::{home_with_keys, keys_line};

/// 시험용 자료 키 값(진짜 키가 아니다).
const DATA_VALUE: &str = "round42e-data-key-not-real-zz";

// 검증: E4 C3
#[test]
fn 옛_자료_키_거절의_까닭은_retired_item이고_새_칸을_안내한다() {
    let refusal = SetupRefusal::RetiredItem;
    assert_eq!(refusal.reason(), "retired_item");
    assert_eq!(refusal.status(), 400);
    let text = refusal.to_string();
    for slot in ["DART", "ECOS", "FRED", "네이버"] {
        assert!(text.contains(slot), "새 칸을 안내한다: {text}");
    }
    assert!(
        SETUP_ITEMS.iter().any(|item| item.name == ITEM_DATA),
        "설정 상태의 항목 넷(API 모양)은 그대로다"
    );
}

// 검증: E4 C3 V46
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 옛_자료_키_등록은_확인하지_않고_retired_item으로_거절한다() {
    let home = tempfile::tempdir().unwrap();
    let running = Running::start(home.path(), true).await;
    let body = json!({"item": KEY_DATA, "fields": {"api_key": DATA_VALUE}});
    for path in [SETUP_KEYS_PATH, REMOTE_KEYS_PATH] {
        let reply = post(running.addr(), path, &body).await;
        assert_eq!(reply.status, 400, "{path}: {}", reply.text);
        assert_eq!(
            reply.body["error"],
            json!("retired_item"),
            "{path}: {}",
            reply.text
        );
        assert_eq!(keys_of(&reply.body), ["error"], "그 밖의 칸은 없다");
        assert!(!reply.text.contains(DATA_VALUE), "받은 값을 싣지 않는다");
    }
    running.assert_stayed_home().await;
    running.close().await;
}

// 검증: V78 C32 V46
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 설정_화면의_진단_키_줄에_옛_자료_키가_없다() {
    let home = tempfile::tempdir().unwrap();
    let running = Running::start(home.path(), true).await;
    let reply = get(running.addr(), SETUP_DOCTOR_PATH).await;
    let keys = reply.body["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["key"] == json!("keys"))
        .cloned()
        .unwrap();
    let detail = keys["detail"].as_str().unwrap();
    assert!(
        !detail.contains("자료 키"),
        "옛 자료 키 항목을 보이지 않는다: {detail}"
    );
    assert!(
        detail.contains("DART"),
        "자료 키 칸(DART …)은 그대로 보인다: {detail}"
    );
    running.close().await;
}

// 검증: D1 V46
#[tokio::test]
async fn doctor_키_줄에_옛_자료_키가_없다() {
    let home = tempfile::tempdir().unwrap();
    home_with_keys(home.path()).await;
    let line = keys_line(home.path());
    assert!(
        !line.contains("자료 키"),
        "옛 자료 키 항목을 보이지 않는다: {line}"
    );
    assert!(
        line.contains("DART"),
        "자료 키 칸(DART …)은 그대로 보인다: {line}"
    );
}
