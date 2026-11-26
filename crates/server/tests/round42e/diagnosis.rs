//! 5번: 설정 화면 진단(`GET /api/setup/doctor`)의 "AI 연결" 줄이 시연이면 시연용 가짜 AI라고 말한다("키 등록" 줄의
//! "AI 연결 없음"과 어긋나지 않게). 고른 연결 방식("API 키")은 뒤에 그대로 적는다. 항목 수 · 순서 · key는 그대로다.

use price_inha::{DIAGNOSIS_ITEMS, SETUP_DOCTOR_PATH};
use serde_json::{Value, json};

use crate::common::{Running, get};

fn item<'a>(body: &'a Value, key: &str) -> &'a Value {
    body["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["key"] == json!(key))
        .unwrap_or_else(|| panic!("{key} 항목이 있다: {body}"))
}

fn keys_in_order(body: &Value) -> Vec<String> {
    body["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["key"].as_str().unwrap().to_string())
        .collect()
}

// 검증: V78 C32 D14
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 시연_진단의_ai_연결_줄은_시연용_가짜_ai라고_말한다() {
    let home = tempfile::tempdir().unwrap();
    let running = Running::start(home.path(), true).await;
    let reply = get(running.addr(), SETUP_DOCTOR_PATH).await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    assert_eq!(
        keys_in_order(&reply.body),
        DIAGNOSIS_ITEMS,
        "항목 수 · 순서 그대로"
    );
    let ai = item(&reply.body, "ai");
    let detail = ai["detail"].as_str().unwrap();
    assert!(detail.contains("시연용 가짜 AI"), "{ai}");
    assert!(
        detail.contains("API 키"),
        "고른 연결 방식은 그대로 적는다: {ai}"
    );
    assert_eq!(ai["status"], json!("info"), "{ai}");
    running.close().await;
}

// 검증: V78 C32
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 실제_모드_진단의_ai_연결_줄에는_시연이_없다() {
    let home = tempfile::tempdir().unwrap();
    let running = Running::start(home.path(), false).await;
    let reply = get(running.addr(), SETUP_DOCTOR_PATH).await;
    assert_eq!(keys_in_order(&reply.body), DIAGNOSIS_ITEMS);
    let detail = item(&reply.body, "ai")["detail"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(detail, "API 키", "실제 모드는 고른 방식만: {detail}");
    running.close().await;
}
