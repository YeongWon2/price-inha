//! server 47차(결정 36 · E56): 시연 모드의 키 등록은 어느 길로 띄워도 바깥에 닿지 않는다.
//! `run --demo`로 띄운 서버(체험 폴더 표시 `demo.json`이 없는 폴더)에서도 키를 실제로 확인하지 않고 받아 두며,
//! 답과 설정 상태에 `trial`이 붙는다. 발판은 round67의 도우미를 그대로 쓴다(그 폴더는 고치지 않는다).

#[path = "../round67/common.rs"]
pub mod common;

use std::net::SocketAddr;

use price_inha::{App, DEMO_KEYS_NOTICE, SETUP_KEYS_PATH, SETUP_PATH};
use serde_json::{Value, json};

use crate::common::{Outside, QUIET, assembly, demo_run_settings};

fn origin_of(addr: SocketAddr) -> String {
    format!("http://127.0.0.1:{}", addr.port())
}

/// 세션을 받아 같은 출처에서 부른다(`body`가 있으면 POST). (상태, JSON 본문).
async fn call(addr: SocketAddr, path: &str, body: Option<&Value>) -> (u16, Value) {
    let origin = origin_of(addr);
    let client = reqwest::Client::new();
    let reply = client
        .post(format!("{origin}{}", pi_gate::SESSION_PATH))
        .header("Origin", &origin)
        .send()
        .await
        .expect("세션 요청이 닿아야 한다");
    let cookie = reply
        .headers()
        .get("set-cookie")
        .expect("세션 쿠키가 있어야 한다")
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .trim()
        .to_string();
    let request = match body {
        Some(body) => client.post(format!("{origin}{path}")).json(body),
        None => client.get(format!("{origin}{path}")),
    };
    let reply = request
        .header("Origin", &origin)
        .header("Cookie", cookie)
        .send()
        .await
        .expect("요청이 닿아야 한다");
    let status = reply.status().as_u16();
    (status, reply.json().await.unwrap_or(Value::Null))
}

fn broker_body() -> Value {
    json!({"item": "broker", "fields": {"client_id": "test-client-id-0071", "client_secret": "test-client-secret-not-real-71"}})
}

// 검증: E56, C3, V54
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn run_demo로_띄운_서버의_키_등록도_바깥에_닿지_않고_받아_둔다() {
    let dir = tempfile::tempdir().unwrap();
    let outside = Outside::start().await;
    let settings = demo_run_settings(dir.path());
    let built = assembly(&settings, &outside);
    let app = App::launch_run(settings, built).await.unwrap();
    let before = outside.total().await;
    let (status, body) = call(app.local_addr(), SETUP_KEYS_PATH, Some(&broker_body())).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["registered"], json!(true), "{body}");
    assert_eq!(body["trial"], json!(true), "{body}");
    assert_eq!(body["notice"], json!(DEMO_KEYS_NOTICE));
    tokio::time::sleep(QUIET).await;
    assert_eq!(
        outside.total().await,
        before,
        "시연에서는 키 확인이 바깥에 닿으면 안 된다"
    );
    let (_, setup) = call(app.local_addr(), SETUP_PATH, None).await;
    assert_eq!(setup["trial"], json!(true), "{setup}");
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
