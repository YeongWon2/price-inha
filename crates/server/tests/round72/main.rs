//! server 48차(보안 검토 5번 · 결정 36 · E56): 시연에서 확인 없이 받아 둔 키는, 다음 시연이 아닌 실행이 확인하기
//! 전에는 쓰지 않는다. 받아 둔 칸 이름만(값 없이) 보관 폴더의 표시 파일에 적고, 시연이 아닌 실행이 켜질 때 지금의
//! 키 확인으로 확인한다: 통과 → 표시 지움 / 거절 → 지우고 안내 · 알림 한 번 / 확인할 수 없음 → 남기고 쓰지 않음.

pub mod common;

use std::path::Path;

use price_inha::{
    App, NOTICES_PATH, Os, SETUP_KEYS_PATH, SETUP_PATH, SETUP_PROVIDERS_PATH, Service,
    ServiceSetup, UNVERIFIED_HELD, UNVERIFIED_KEYS_FILE, UNVERIFIED_PENDING, UNVERIFIED_REMOVED,
    doctor,
};
use serde_json::{Value, json};

use crate::common::{
    AI_KEY, Answer, BROKER_ID, BROKER_SECRET, NAVER_ID, NAVER_SECRET, Outside, QUIET, ai_body,
    assembly, broker_body, call, demo_settings, eventually, live_settings, naver_body,
};

/// `run --demo`로 띄워 `bodies`를 차례로 등록하고 끈다. 시연은 바깥에 닿지 않는다.
async fn register_in_demo(home: &Path, bodies: &[Value]) {
    let outside = Outside::silent().await;
    let settings = demo_settings(home);
    let built = assembly(&settings, &outside);
    let app = App::launch_run(settings, built).await.unwrap();
    for body in bodies {
        let (status, reply) = call(app.local_addr(), SETUP_KEYS_PATH, Some(body)).await;
        assert_eq!(status, 200, "{reply}");
        assert_eq!(reply["trial"], json!(true), "{reply}");
    }
    app.shutdown().await.unwrap();
    assert_eq!(outside.total().await, 0, "시연은 바깥에 닿지 않는다");
}

/// 표시 파일의 글자(없으면 빈 글자).
fn marks_text(home: &Path) -> String {
    std::fs::read_to_string(home.join(UNVERIFIED_KEYS_FILE)).unwrap_or_default()
}

/// 설정 상태의 `unverified` 칸에서 그 항목(없으면 `Null`).
fn unverified_of(setup: &Value, item: &str) -> Value {
    setup["unverified"]
        .as_array()
        .and_then(|list| list.iter().find(|entry| entry["item"] == json!(item)))
        .cloned()
        .unwrap_or(Value::Null)
}

fn item_registered(setup: &Value, name: &str) -> bool {
    setup["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|item| item["name"] == json!(name) && item["registered"] == json!(true))
}

// 검증: E56, N8
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 시연에서_받은_키는_값_없이_칸_이름만_표시_파일에_적힌다() {
    let dir = tempfile::tempdir().unwrap();
    register_in_demo(dir.path(), &[broker_body(), ai_body(), naver_body()]).await;
    let path = dir.path().join(UNVERIFIED_KEYS_FILE);
    let text = marks_text(dir.path());
    for item in ["broker", "ai_api_key", "naver_news"] {
        assert!(text.contains(item), "{item}이 표시돼야 한다: {text}");
    }
    for value in [BROKER_ID, BROKER_SECRET, AI_KEY, NAVER_ID, NAVER_SECRET] {
        assert!(!text.contains(value), "표시 파일에 키 값이 있으면 안 된다");
        assert!(!text.contains(&value[value.len() - 6..]), "키 일부도 없다");
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "표시 파일은 0600");
    }
}

// 검증: E56, V31
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 바깥이_받아들이면_표시가_지워지고_그_뒤에야_시세를_묻는다() {
    let dir = tempfile::tempdir().unwrap();
    register_in_demo(dir.path(), &[broker_body(), ai_body()]).await;
    let outside = Outside::start(Answer::Accept, Answer::Accept, Answer::Down).await;
    let settings = live_settings(dir.path());
    let built = assembly(&settings, &outside);
    let app = App::launch_run(settings, built).await.unwrap();
    let text = marks_text(dir.path());
    assert!(
        !text.contains("broker") && !text.contains("ai_api_key"),
        "확인을 지난 키는 표시에서 지운다: {text}"
    );
    let asked = eventually(|| async {
        outside
            .broker_calls()
            .await
            .iter()
            .any(|(method, _)| method == "GET")
    })
    .await;
    assert!(asked, "확인을 지난 뒤에는 시세를 묻는다");
    let calls = outside.broker_calls().await;
    assert_eq!(calls[0].0, "POST", "첫 요청은 키 확인(토큰)이다: {calls:?}");
    let ai = outside.requests(1).await;
    assert_eq!(ai[0].url.path(), "/v1/models", "AI도 먼저 확인한다");
    let (_, setup) = call(app.local_addr(), SETUP_PATH, None).await;
    assert_eq!(unverified_of(&setup, "broker"), Value::Null, "{setup}");
    app.shutdown().await.unwrap();
}

// 검증: E56, V31
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 바깥이_거절하면_키를_지우거나_쓰지_않고_안내하며_바깥_요청은_확인뿐이다() {
    let dir = tempfile::tempdir().unwrap();
    register_in_demo(dir.path(), &[broker_body(), naver_body()]).await;
    let outside = Outside::start(Answer::Reject, Answer::Down, Answer::Reject).await;
    let settings = live_settings(dir.path());
    let built = assembly(&settings, &outside);
    let app = App::launch_run(settings, built).await.unwrap();
    tokio::time::sleep(QUIET).await;
    let calls = outside.broker_calls().await;
    assert_eq!(calls.len(), 1, "증권사에는 확인 한 번뿐이다: {calls:?}");
    assert_eq!(calls[0].0, "POST", "{calls:?}");
    assert_eq!(
        outside.requests(3).await.len(),
        1,
        "뉴스도 확인 한 번뿐이다"
    );
    let (_, setup) = call(app.local_addr(), SETUP_PATH, None).await;
    let naver = unverified_of(&setup, "naver_news");
    assert_eq!(naver["state"], json!(UNVERIFIED_REMOVED), "{setup}");
    let message = naver["message"].as_str().unwrap_or_default();
    assert!(
        message.contains("시연에서 받아 둔")
            && message.contains("지웠습니다")
            && message.contains("다시 등록하세요"),
        "{message}"
    );
    let broker = unverified_of(&setup, "broker");
    assert_eq!(broker["state"], json!(UNVERIFIED_HELD), "{setup}");
    assert!(
        broker["message"]
            .as_str()
            .unwrap_or_default()
            .contains("다시 등록하세요"),
        "{broker}"
    );
    let (_, providers) = call(app.local_addr(), SETUP_PROVIDERS_PATH, None).await;
    assert_eq!(
        providers["providers"][0]["registered"],
        json!(false),
        "거절된 네이버 키는 지웠다: {providers}"
    );
    let noticed =
        eventually(|| async { count_notices(app.local_addr(), "네이버 뉴스").await == 1 }).await;
    assert!(noticed, "거절은 알림 한 번");
    app.shutdown().await.unwrap();
    // 다시 켜도 같은 알림을 또 내지 않고, 거절된 키로 바깥에 묻지 않는다.
    let again = Outside::start(Answer::Reject, Answer::Down, Answer::Reject).await;
    let settings = live_settings(dir.path());
    let built = assembly(&settings, &again);
    let app = App::launch_run(settings, built).await.unwrap();
    tokio::time::sleep(QUIET).await;
    assert_eq!(
        count_notices(app.local_addr(), "네이버 뉴스").await,
        1,
        "알림은 한 번"
    );
    assert_eq!(
        count_notices(app.local_addr(), "증권사").await,
        1,
        "알림은 한 번"
    );
    assert!(
        again.broker_calls().await.is_empty(),
        "쓰지 않는 키로 묻지 않는다"
    );
    app.shutdown().await.unwrap();
}

/// 알림 목록에서 "시연에서 받아 둔"과 `label`이 함께 든 알림 수.
async fn count_notices(addr: std::net::SocketAddr, label: &str) -> usize {
    let (_, list) = call(addr, &format!("{NOTICES_PATH}?max=200"), None).await;
    list.as_array().map_or(0, |list| {
        list.iter()
            .filter(|notice| {
                let body = notice["body"].as_str().unwrap_or_default();
                body.contains("시연에서 받아 둔") && body.contains(label)
            })
            .count()
    })
}

// 검증: E56, V31
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 바깥이_장애면_키를_남기되_쓰지_않고_확인_대기로_둔다() {
    let dir = tempfile::tempdir().unwrap();
    register_in_demo(dir.path(), &[broker_body()]).await;
    let outside = Outside::start(Answer::Down, Answer::Down, Answer::Down).await;
    let settings = live_settings(dir.path());
    let built = assembly(&settings, &outside);
    let app = App::launch_run(settings, built).await.unwrap();
    tokio::time::sleep(QUIET).await;
    let calls = outside.broker_calls().await;
    assert!(!calls.is_empty(), "확인은 해 본다");
    assert!(
        calls.iter().all(|(method, _)| method == "POST"),
        "확인 전에는 시세를 묻지 않는다: {calls:?}"
    );
    assert!(marks_text(dir.path()).contains("broker"), "표시는 남는다");
    let (_, setup) = call(app.local_addr(), SETUP_PATH, None).await;
    let broker = unverified_of(&setup, "broker");
    assert_eq!(broker["state"], json!(UNVERIFIED_PENDING), "{setup}");
    assert!(
        broker["message"]
            .as_str()
            .unwrap_or_default()
            .contains("확인 대기"),
        "{broker}"
    );
    assert!(
        item_registered(&setup, "broker"),
        "키는 지우지 않는다: {setup}"
    );
    app.shutdown().await.unwrap();
}

// 검증: E56
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn doctor는_시연에서_받아_둔_키_수를_한_줄로_알린다() {
    let dir = tempfile::tempdir().unwrap();
    register_in_demo(dir.path(), &[broker_body(), naver_body()]).await;
    let defs = tempfile::tempdir().unwrap();
    let runner = crate::common::NoCommands;
    let setup = ServiceSetup {
        os: Os::MacOs,
        exe: "/usr/local/bin/price-inha".into(),
        home: dir.path().to_path_buf(),
        definition_dir: defs.path().to_path_buf(),
        uid: 501,
        path_env: "/usr/bin:/bin".to_string(),
        run_args: Vec::new(),
    };
    let service = Service::new(setup, &runner);
    let text = doctor(&live_settings(dir.path()), &service).render();
    assert!(
        text.contains("시연에서 받아 둔 키 2개 — 다음 실행에서 확인합니다"),
        "{text}"
    );
}

// 검증: E56, E4
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 실제판에서_직접_등록한_키는_표시하지_않는다() {
    let dir = tempfile::tempdir().unwrap();
    let outside = Outside::start(Answer::Accept, Answer::Down, Answer::Down).await;
    let settings = live_settings(dir.path());
    let built = assembly(&settings, &outside);
    let app = App::launch_run(settings, built).await.unwrap();
    let (status, reply) = call(app.local_addr(), SETUP_KEYS_PATH, Some(&broker_body())).await;
    assert_eq!(status, 200, "{reply}");
    assert_eq!(reply["registered"], json!(true), "{reply}");
    assert!(
        !marks_text(dir.path()).contains("broker"),
        "실제판 등록은 표시하지 않는다"
    );
    let (_, setup) = call(app.local_addr(), SETUP_PATH, None).await;
    assert_eq!(unverified_of(&setup, "broker"), Value::Null, "{setup}");
    app.shutdown().await.unwrap();
}
