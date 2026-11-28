//! (결정 36 · E56) 체험 폴더(`price-inha demo`가 만든 폴더)의 키 등록은 바깥에 닿지 않는다: 빈 값이 아니면 받아 두고
//! 답에 "체험판 — 실제로 확인하지 않음"을 싣는다. 설정 상태에는 `"trial": true`가 붙어 화면이 안내 줄을 그린다.
//! `run --demo`(체험 폴더 표시가 없는 폴더)는 지금 동작 그대로다.

use std::net::SocketAddr;

use price_inha::{App, DEMO_KEYS_NOTICE, DemoRequest, SETUP_KEYS_PATH, SETUP_PATH, start_demo};
use serde_json::{Value, json};

use crate::common::{Outside, QUIET, Recorder, STEP, assembly, demo_run_settings};

fn origin_of(addr: SocketAddr) -> String {
    format!("http://127.0.0.1:{}", addr.port())
}

async fn login(addr: SocketAddr) -> String {
    let origin = origin_of(addr);
    let reply = reqwest::Client::new()
        .post(format!("{origin}{}", pi_gate::SESSION_PATH))
        .header("Origin", &origin)
        .send()
        .await
        .expect("세션 요청이 닿아야 한다");
    let raw = reply
        .headers()
        .get("set-cookie")
        .expect("세션 쿠키가 있어야 한다")
        .to_str()
        .unwrap()
        .to_string();
    raw.split(';').next().unwrap().trim().to_string()
}

/// 세션을 받아 같은 출처에서 부른다(`body`가 있으면 POST). (상태, JSON 본문).
async fn call(addr: SocketAddr, path: &str, body: Option<&Value>) -> (u16, Value) {
    let cookie = login(addr).await;
    let client = reqwest::Client::new();
    let url = format!("{}{path}", origin_of(addr));
    let request = match body {
        Some(body) => client
            .post(url)
            .header("Content-Type", "application/json")
            .body(body.to_string()),
        None => client.get(url),
    };
    let reply = request
        .header("Cookie", cookie)
        .header("Origin", origin_of(addr))
        .send()
        .await
        .expect("요청이 닿아야 한다");
    let status = reply.status().as_u16();
    let bytes = reply.bytes().await.unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

async fn start_trial(home: &std::path::Path, outside: &Outside) -> App {
    let settings = demo_run_settings(home);
    let built = assembly(&settings, outside);
    let mut printed = Vec::new();
    let request = DemoRequest { no_open: true };
    tokio::time::timeout(
        STEP,
        start_demo(
            settings,
            built,
            &request,
            &mut printed,
            &Recorder::default(),
        ),
    )
    .await
    .expect("시간 안에 떠야 한다")
    .expect("체험 서버가 떠야 한다")
}

fn broker_body() -> Value {
    json!({"item": "broker", "fields": {"client_id": "test-client-id-0067", "client_secret": "test-client-secret-not-real-67"}})
}

// 검증: E56, C3, V54
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 체험_폴더의_키_등록은_바깥에_닿지_않고_받아_둔다() {
    let dir = tempfile::tempdir().unwrap();
    let outside = Outside::start().await;
    let app = start_trial(dir.path(), &outside).await;
    let before = outside.total().await;
    let (status, body) = call(app.local_addr(), SETUP_KEYS_PATH, Some(&broker_body())).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["registered"], json!(true));
    assert_eq!(body["trial"], json!(true), "{body}");
    assert_eq!(body["notice"], json!(DEMO_KEYS_NOTICE));
    assert!(DEMO_KEYS_NOTICE.contains("체험판"));
    tokio::time::sleep(QUIET).await;
    assert_eq!(
        outside.total().await,
        before,
        "키 확인이 바깥에 닿으면 안 된다"
    );
    let (_, setup) = call(app.local_addr(), SETUP_PATH, None).await;
    let broker = setup["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["name"] == json!("broker"))
        .cloned()
        .unwrap();
    assert_eq!(broker["registered"], json!(true), "받아 두었다: {setup}");
    app.shutdown().await.unwrap();
}

// 검증: E56, V54
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 체험_폴더면_설정_상태에_trial이_붙는다() {
    let dir = tempfile::tempdir().unwrap();
    let outside = Outside::start().await;
    let app = start_trial(dir.path(), &outside).await;
    let (status, setup) = call(app.local_addr(), SETUP_PATH, None).await;
    assert_eq!(status, 200);
    assert_eq!(setup["trial"], json!(true), "{setup}");
    assert_eq!(setup["demo"], json!(true));
    app.shutdown().await.unwrap();
}

// 검증: E56
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn run_demo의_폴더에는_trial이_없다() {
    let dir = tempfile::tempdir().unwrap();
    let outside = Outside::start().await;
    let settings = demo_run_settings(dir.path());
    let built = assembly(&settings, &outside);
    let app = App::launch_run(settings, built).await.unwrap();
    let (_, setup) = call(app.local_addr(), SETUP_PATH, None).await;
    assert!(setup.get("trial").is_none(), "{setup}");
    app.shutdown().await.unwrap();
}
