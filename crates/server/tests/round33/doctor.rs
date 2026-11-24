//! 1번 (라): 진단 읽기(`GET /api/setup/doctor`). 실행 중인 서버가 이미 아는 것만 모은 읽기 전용 요약이다:
//! 항목은 정한 순서 · 정한 칸, 상태는 넷 가운데 하나, 할 일은 `warn` · `fail`일 때만 글자. 등록한 키 값은 응답
//! 어디에도 없고, 원격은 세션 없이 401이며, 읽어도 바깥 주소(가짜 서버)로 요청이 나가지 않는다.

use price_inha::{
    DIAG_FAIL, DIAG_INFO, DIAG_OK, DIAG_WARN, DIAGNOSIS_ITEMS, SETUP_DOCTOR_PATH, SETUP_PATH,
};
use serde_json::{Value, json};

use crate::common::{Bench, KEY_VALUES, get, get_anonymous, get_with, keys_of};

/// 진단을 세션으로 읽어 200인지 보고 본문을 돌려준다.
async fn diagnosis(bench: &Bench) -> Value {
    let reply = get(bench.addr(), SETUP_DOCTOR_PATH).await;
    assert_eq!(reply.status, 200, "진단을 읽는다: {}", reply.text);
    reply.body
}

/// 그 `key`의 항목.
fn item<'a>(body: &'a Value, key: &str) -> &'a Value {
    body["items"]
        .as_array()
        .and_then(|items| items.iter().find(|item| item["key"] == key))
        .unwrap_or_else(|| panic!("{key} 항목이 있다: {body}"))
}

/// 항목의 `detail` 글자.
fn detail(item: &Value) -> &str {
    item["detail"]
        .as_str()
        .unwrap_or_else(|| panic!("detail은 글자다: {item}"))
}

// 검증: V78 C32 R18-4
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 진단은_정한_항목을_정한_순서로_내고_항목마다_정한_칸만_있다() {
    assert_eq!(
        DIAGNOSIS_ITEMS,
        [
            "store",
            "keys",
            "ai",
            "ai_daily_limit",
            "backup",
            "demo",
            "market"
        ],
        "실행 중인 서버가 아는 것만 본다"
    );
    let bench = Bench::with_keys().await;
    let body = diagnosis(&bench).await;
    assert_eq!(keys_of(&body), ["items"], "맨 위 칸은 items 하나다: {body}");
    let items = body["items"].as_array().expect("items는 목록이다");
    let order: Vec<&Value> = items.iter().map(|item| &item["key"]).collect();
    assert_eq!(order, DIAGNOSIS_ITEMS, "정한 순서다");
    for item in items {
        assert_eq!(
            keys_of(item),
            ["detail", "key", "label", "status", "todo"],
            "항목 칸은 다섯뿐이다: {item}"
        );
        assert!(
            item["label"]
                .as_str()
                .is_some_and(|label| !label.trim().is_empty()),
            "이름이 있다: {item}"
        );
        assert!(item["detail"].is_string(), "detail은 글자다: {item}");
        let status = item["status"].as_str().expect("상태는 글자다");
        assert!(
            [DIAG_OK, DIAG_WARN, DIAG_FAIL, DIAG_INFO].contains(&status),
            "상태는 넷 가운데 하나다: {item}"
        );
        if status == DIAG_OK || status == DIAG_INFO {
            assert_eq!(item["todo"], Value::Null, "할 일이 없으면 null이다: {item}");
        } else {
            assert!(
                item["todo"]
                    .as_str()
                    .is_some_and(|todo| !todo.trim().is_empty()),
                "살펴볼 것 · 문제에는 할 일이 있다: {item}"
            );
        }
    }
    assert!(
        !has_number(&body),
        "수는 싣지 않는다(판 번호 · 상한도 글자 안에 둔다): {body}"
    );
    bench.close().await;
}

/// JSON 어디에 수가 있나.
fn has_number(value: &Value) -> bool {
    match value {
        Value::Number(_) => true,
        Value::Array(list) => list.iter().any(has_number),
        Value::Object(map) => map.values().any(has_number),
        _ => false,
    }
}

// 검증: V78 C32 R18-4 R16-4
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 시연_모드에서_진단은_저장소_판_키_ai_연결_상한_백업_시연_시장을_보인다() {
    let bench = Bench::with_keys().await;
    let body = diagnosis(&bench).await;

    let store = item(&body, "store");
    assert_eq!(store["status"], json!(DIAG_OK), "{store}");
    assert!(
        detail(store).contains(&pi_store::SCHEMA_VERSION.to_string()),
        "열린 저장소의 판 번호가 보인다: {store}"
    );

    let keys = item(&body, "keys");
    assert_eq!(keys["status"], json!(DIAG_OK), "필수 키가 다 있다: {keys}");
    assert_eq!(keys["todo"], Value::Null);

    let ai = item(&body, "ai");
    assert_eq!(ai["status"], json!(DIAG_INFO), "{ai}");
    assert!(
        detail(ai).contains("API 키"),
        "고른 것이 없으면 API 키 연결이다: {ai}"
    );

    let limit = item(&body, "ai_daily_limit");
    assert_eq!(limit["status"], json!(DIAG_INFO), "{limit}");
    assert!(
        detail(limit).contains("없음"),
        "상한을 주지 않았다: {limit}"
    );

    let backup = item(&body, "backup");
    assert_eq!(
        backup["status"],
        json!(DIAG_INFO),
        "백업을 켜지 않았다: {backup}"
    );
    assert!(detail(backup).contains("꺼져"), "{backup}");
    assert_eq!(backup["todo"], Value::Null);

    let demo = item(&body, "demo");
    assert_eq!(demo["status"], json!(DIAG_INFO), "{demo}");
    assert!(detail(demo).contains("시연"), "시연 모드로 켰다: {demo}");

    let market = item(&body, "market");
    assert_eq!(market["status"], json!(DIAG_INFO), "{market}");
    assert!(
        detail(market).contains("국내") && detail(market).contains("kr"),
        "인자를 주지 않으면 국내 시장이다: {market}"
    );
    bench.close().await;
}

// 검증: V78 C32 E4
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 필수_키가_없으면_키_항목은_할_일과_함께_살펴볼_것이다() {
    let bench = Bench::without_keys().await;
    let body = diagnosis(&bench).await;
    let keys = item(&body, "keys");
    assert_eq!(
        keys["status"],
        json!(DIAG_WARN),
        "시연 모드는 키 없이 돌지만 실제로 쓰려면 키가 필요하다: {keys}"
    );
    assert!(
        keys["todo"]
            .as_str()
            .is_some_and(|todo| !todo.trim().is_empty()),
        "키 등록 안내가 있다: {keys}"
    );
    bench.close().await;
}

// 검증: V78 V46 N8 C32
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 진단과_설정_상태_응답_어디에도_등록한_키_값이_없다() {
    let bench = Bench::with_keys().await;
    for path in [SETUP_DOCTOR_PATH, SETUP_PATH] {
        let reply = get(bench.addr(), path).await;
        assert_eq!(reply.status, 200, "{path}: {}", reply.text);
        for value in KEY_VALUES {
            assert!(
                !reply.text.contains(value),
                "{path} 응답에 등록한 키 값이 나오면 안 된다"
            );
        }
    }
    let keys = item(&diagnosis(&bench).await, "keys").clone();
    assert!(
        detail(&keys).contains("있음"),
        "키 항목은 등록 여부만 말한다: {keys}"
    );
    bench.close().await;
}

// 검증: V78 C15 E38 C32
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 원격에서_세션_없이_진단을_읽으면_401이고_원격_세션이면_읽힌다() {
    let bench = Bench::with_keys().await;
    bench.go_remote();
    let reply = get_anonymous(bench.addr(), SETUP_DOCTOR_PATH).await;
    assert_eq!(
        reply.status, 401,
        "세션 없는 원격은 읽지 못한다: {}",
        reply.text
    );
    let cookie = bench.remote_login().await;
    let reply = get_with(bench.addr(), SETUP_DOCTOR_PATH, &cookie).await;
    assert_eq!(reply.status, 200, "원격 세션이면 읽는다: {}", reply.text);
    let order: Vec<&Value> = reply.body["items"]
        .as_array()
        .map(|items| items.iter().map(|item| &item["key"]).collect())
        .unwrap_or_default();
    assert_eq!(order, DIAGNOSIS_ITEMS, "같은 항목이다: {}", reply.body);
    bench.close().await;
}

// 검증: V78 D2 V31 C32
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 진단을_읽어도_바깥_주소로_요청이_나가지_않는다() {
    let bench = Bench::with_keys().await;
    for _ in 0..3 {
        diagnosis(&bench).await;
    }
    let reply = get(bench.addr(), SETUP_PATH).await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    for (name, count) in bench.outside.counts().await {
        assert_eq!(count, 0, "진단은 {name} 주소로 요청하지 않는다");
    }
    bench.close().await;
}
