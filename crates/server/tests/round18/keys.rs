//! 10번 키 칸(secrets 8차, 결정 2026-11-21 2 · 3번): 키 칸 목록을 공급자 선언(`pi_secrets::ProviderKeys`)에서
//! 만든다. 네이버 뉴스(`pi_data::NAVER_NEWS_KEYS`)의 두 칸이 새 칸 목록 경로에 있고, 옛 `item` `data` 칸은 없다.
//! 네이버 키 등록은 `verify_news_keys`(검색 한 번)로 확인하고, 네이버가 거절하면 저장하지 않는다.
//!
//! 옛 `GET /api/setup` · `/api/setup/slots`의 모양은 앞 차수의 잠긴 시험이 그대로 지킨다(여기서 보지 않는다).

use pi_secrets::SecretStore;
use price_inha::{SETUP_KEYS_PATH, SETUP_PROVIDERS_PATH};
use serde_json::{Value, json};

use crate::common::{
    Bench, NAVER_ID, NAVER_SECRET, NEWS_PATH, Plan, get, kst, monday, naver_keys, post,
};

/// 네이버 뉴스 키 등록 본문.
fn naver_body() -> Value {
    json!({
        "item": naver_keys().provider(),
        "fields": {"client_id": NAVER_ID, "client_secret": NAVER_SECRET},
    })
}

/// 새 칸 목록에서 그 공급자의 항목.
async fn provider_entry(bench: &Bench, name: &str) -> Option<Value> {
    let reply = get(bench.addr(), SETUP_PROVIDERS_PATH).await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    let list = reply.body["providers"].as_array().cloned();
    let list = list.expect("공급자 목록");
    list.into_iter()
        .find(|entry| entry["provider"] == json!(name))
}

/// 보관소에 네이버 키가 있나.
fn stored(bench: &Bench) -> bool {
    let secrets = SecretStore::open(bench.home()).unwrap();
    secrets.read_provider(naver_keys()).unwrap().is_some()
}

// 검증: C3 E4 D9
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 키_칸_목록은_공급자_선언에서_나오고_네이버_두_칸이_있으며_옛_자료_칸은_없다() {
    let bench = Plan::demo().start(kst(monday(), 10, 0)).await;
    let naver = provider_entry(&bench, naver_keys().provider())
        .await
        .expect("네이버 뉴스 칸이 있다");
    let fields: Vec<&str> = naver["fields"]
        .as_array()
        .expect("칸 이름들")
        .iter()
        .filter_map(Value::as_str)
        .collect();
    assert_eq!(fields, naver_keys().fields(), "칸은 선언 그대로다");
    assert_eq!(naver["registered"], json!(false), "아직 등록하지 않았다");
    assert!(
        provider_entry(&bench, "data").await.is_none(),
        "옛 자료 칸(item data)은 새 칸 목록에 없다"
    );
    bench.close().await;
}

// 검증: C3 E4 N8 E38
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 네이버_키는_검색_한_번으로_확인되고_받아들여지면_저장된다() {
    let bench = Plan::live().start(kst(monday(), 10, 0)).await;
    let reply = post(bench.addr(), SETUP_KEYS_PATH, &naver_body()).await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    assert_eq!(reply.body, json!({"registered": true}));
    assert!(
        !reply.text.contains(NAVER_SECRET) && !reply.text.contains(NAVER_ID),
        "답에 값이 없다"
    );

    let searched = bench.data.requests(NEWS_PATH).await;
    assert!(!searched.is_empty(), "확인은 네이버 검색으로 한다");
    let id = searched[0]
        .headers
        .get("x-naver-client-id")
        .and_then(|value| value.to_str().ok());
    assert_eq!(id, Some(NAVER_ID), "넣은 키로 확인한다");
    assert!(
        !searched[0].url.as_str().contains(NAVER_SECRET),
        "키는 주소에 싣지 않는다"
    );
    assert!(stored(&bench), "보관소에 있다");
    let naver = provider_entry(&bench, naver_keys().provider()).await;
    assert_eq!(naver.expect("네이버 칸")["registered"], json!(true));
    let listed = get(bench.addr(), SETUP_PROVIDERS_PATH).await;
    assert!(!listed.text.contains(NAVER_SECRET), "칸 목록에 값이 없다");
    bench.close().await;
}

// 검증: C3 E4
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 네이버가_키를_거절하면_저장하지_않는다() {
    let bench = Plan::live().start(kst(monday(), 10, 0)).await;
    bench.data.reject_news_keys();
    let reply = post(bench.addr(), SETUP_KEYS_PATH, &naver_body()).await;
    assert_eq!(reply.status, 422, "{}", reply.text);
    assert_eq!(reply.body, json!({"error": "key_rejected"}));
    assert!(!reply.text.contains(NAVER_SECRET), "답에 값이 없다");
    assert!(
        !bench.data.requests(NEWS_PATH).await.is_empty(),
        "네이버에 물어 확인했다"
    );
    assert!(!stored(&bench), "저장하지 않는다");
    let naver = provider_entry(&bench, naver_keys().provider()).await;
    assert_eq!(naver.expect("네이버 칸")["registered"], json!(false));
    bench.close().await;
}
