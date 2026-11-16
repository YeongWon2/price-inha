//! 접속 키 발급 · 잠금 풀기 · 접속 기록 보기(C1 · N3 · E2 · V40 · V41).

use price_inha::{ACCESS_KEY_PATH, ACCESS_LOG_PATH, ACCESS_UNLOCK_PATH, SETUP_PATH};
use serde_json::{Value, json};

use crate::common::{
    Bench, REMOTE_DEVICE, ask_session, get_with, home_contains, issue_key, post_with, session,
};

/// 틀린 키(글자 수 · 글자 모음은 맞다).
const WRONG_KEY: &str = "00000000000000000000";

fn entries(log: &Value) -> Vec<Value> {
    log["entries"]
        .as_array()
        .expect("기록 목록이 있어야 한다")
        .clone()
}

/// C1 · V40: 그 컴퓨터에서 발급한 키로 다른 기기의 세션이 생기고, 접속 기록에 그 기기와 시각이 보인다.
#[tokio::test]
async fn issued_key_opens_remote_session_and_shows_in_log() {
    let bench = Bench::start().await;
    let (key, reply) = issue_key(&bench).await;
    assert_eq!(key.chars().count(), pi_gate::ACCESS_KEY_LEN);
    assert!(
        key.chars()
            .all(|c| pi_gate::ACCESS_KEY_ALPHABET.contains(c))
    );
    assert_eq!(reply.body["generation"], json!(1));

    bench.go_remote();
    let cookie = session(bench.addr(), Some(&key)).await;
    let log = get_with(bench.addr(), ACCESS_LOG_PATH, Some(&cookie)).await;
    assert_eq!(log.status, 200, "{}", log.text);
    let entered = entries(&log.body).into_iter().any(|e| {
        e["action"] == json!(pi_gate::ACCESS_ENTER)
            && e["origin"] == json!(REMOTE_DEVICE)
            && e["at"].as_i64().is_some_and(|at| at > 0)
    });
    assert!(
        entered,
        "방금 들어온 기기와 시각이 보여야 한다: {}",
        log.text
    );
    bench.close().await;
}

/// 원격 세션으로는 키를 발급하지도 잠금을 풀지도 못한다(403). 앞 키는 그대로 맞는다.
#[tokio::test]
async fn remote_session_cannot_issue_or_unlock() {
    let bench = Bench::start().await;
    let (key, _) = issue_key(&bench).await;
    bench.go_remote();
    let cookie = session(bench.addr(), Some(&key)).await;

    let issue = post_with(bench.addr(), ACCESS_KEY_PATH, &cookie, &json!({})).await;
    assert_eq!(issue.status, pi_gate::LOCAL_ONLY_STATUS);
    assert!(issue.body.get(pi_gate::ACCESS_KEY_FIELD).is_none());
    let unlock = post_with(
        bench.addr(),
        ACCESS_UNLOCK_PATH,
        &cookie,
        &json!({ "device": REMOTE_DEVICE }),
    )
    .await;
    assert_eq!(unlock.status, pi_gate::LOCAL_ONLY_STATUS);

    let again = ask_session(bench.addr(), Some(&key)).await;
    assert!(
        (200..300).contains(&again.status),
        "키가 바뀌지 않았어야 한다"
    );
    bench.close().await;
}

/// 키 글자는 발급 답에만 있다: 접속 기록 · 설정 상태 · 보관 폴더 어디에도 남지 않는다.
#[tokio::test]
async fn issued_key_appears_only_in_the_reply() {
    let bench = Bench::start().await;
    let (key, reply) = issue_key(&bench).await;
    assert!(reply.text.contains(&key));

    bench.go_remote();
    let remote = session(bench.addr(), Some(&key)).await;
    bench.back_home();
    let home = session(bench.addr(), None).await;
    for path in [ACCESS_LOG_PATH, SETUP_PATH] {
        for cookie in [&home, &remote] {
            let seen = get_with(bench.addr(), path, Some(cookie)).await;
            assert_eq!(seen.status, 200, "{path}");
            assert!(!seen.text.contains(&key), "{path}에 키가 보이면 안 된다");
        }
    }
    let log = get_with(bench.addr(), ACCESS_LOG_PATH, Some(&home)).await;
    let changed = entries(&log.body)
        .iter()
        .any(|e| e["action"] == json!(pi_gate::ACCESS_KEY_CHANGED));
    assert!(changed, "키를 바꾼 것이 기록에 보여야 한다");
    assert!(
        !home_contains(&bench.home(), &key),
        "보관 폴더에 키가 남으면 안 된다"
    );
    bench.close().await;
}

/// V41: 다시 발급하면 앞 세대의 원격 세션이 끊기고 앞 키는 더 맞지 않는다. 그 컴퓨터의 세션은 그대로다.
#[tokio::test]
async fn reissue_cuts_old_remote_sessions() {
    let bench = Bench::start().await;
    let (first, _) = issue_key(&bench).await;
    bench.go_remote();
    let old = session(bench.addr(), Some(&first)).await;
    bench.back_home();
    let home = session(bench.addr(), None).await;

    let reply = post_with(bench.addr(), ACCESS_KEY_PATH, &home, &json!({})).await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    assert_eq!(reply.body["generation"], json!(2));
    let second = reply.body[pi_gate::ACCESS_KEY_FIELD]
        .as_str()
        .unwrap()
        .to_string();
    assert_ne!(first, second);
    assert_eq!(
        get_with(bench.addr(), ACCESS_LOG_PATH, Some(&home))
            .await
            .status,
        200
    );

    bench.go_remote();
    let cut = get_with(bench.addr(), ACCESS_LOG_PATH, Some(&old)).await;
    assert_eq!(cut.status, 401, "앞 세대 세션은 끊겨야 한다");
    assert_eq!(ask_session(bench.addr(), Some(&first)).await.status, 401);
    session(bench.addr(), Some(&second)).await;
    bench.close().await;
}

/// N3 · E2: 잠긴 기기가 접속 기록 화면에 보이고, 그 컴퓨터에서 풀면 다시 들어올 수 있다.
#[tokio::test]
async fn unlock_from_home_frees_locked_device() {
    let bench = Bench::start().await;
    let (key, _) = issue_key(&bench).await;
    bench.go_remote();
    for _ in 0..pi_gate::MAX_KEY_FAILURES {
        assert_eq!(ask_session(bench.addr(), Some(WRONG_KEY)).await.status, 401);
    }
    let locked = ask_session(bench.addr(), Some(&key)).await;
    assert_eq!(locked.status, pi_gate::LOCKED_STATUS);

    bench.back_home();
    let home = session(bench.addr(), None).await;
    let log = get_with(bench.addr(), ACCESS_LOG_PATH, Some(&home)).await;
    let shown = log.body["locked"].as_array().is_some_and(|all| {
        all.iter()
            .any(|d| d["device"] == json!(REMOTE_DEVICE) && d["until"].as_i64().is_some())
    });
    assert!(shown, "잠긴 기기가 보여야 한다: {}", log.text);

    let bad = json!({ "device": "not-an-address" });
    let refused = post_with(bench.addr(), ACCESS_UNLOCK_PATH, &home, &bad).await;
    assert_eq!(refused.status, 400);
    let device = json!({ "device": REMOTE_DEVICE });
    let freed = post_with(bench.addr(), ACCESS_UNLOCK_PATH, &home, &device).await;
    assert_eq!(freed.status, 200, "{}", freed.text);
    assert_eq!(freed.body, json!({ "unlocked": true }));
    let twice = post_with(bench.addr(), ACCESS_UNLOCK_PATH, &home, &device).await;
    assert_eq!(twice.body, json!({ "unlocked": false }));

    bench.go_remote();
    session(bench.addr(), Some(&key)).await;
    bench.close().await;
}

/// 세션이 없으면 접속 기록을 볼 수 없다.
#[tokio::test]
async fn access_log_needs_a_session() {
    let bench = Bench::start().await;
    bench.go_remote();
    let seen = get_with(bench.addr(), ACCESS_LOG_PATH, None).await;
    assert_eq!(seen.status, 401);
    bench.close().await;
}
