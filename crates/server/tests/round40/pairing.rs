//! 39차 4번: 페어링 승인은 이 컴퓨터에서만 — 맞는 코드로 연결 · 틀리면 400 · 5번째에 423 · 원격 403 · 빼기 · 허용 1명
//! (결정 24 · V88).

use std::time::Duration;

use price_inha::{
    KIND_MESSENGER, MESSENGER_ACTION_PAIRED, MESSENGER_ACTION_REVOKED, MESSENGER_HELP,
    MESSENGER_LOCKED, MESSENGER_OWNER_EXISTS, MESSENGER_PAIRING_APPROVE_PATH,
    MESSENGER_PAIRING_REVOKE_PATH, MESSENGER_WRONG_CODE,
};
use serde_json::{Value, json};

use crate::common::{
    Bench, FakeTelegram, QUIET, code_in, events, is_pairing_reply, post_empty, send_with,
};

/// 틀린 코드(코드 글자 모양이지만 준 적 없는 것).
const WRONG: &str = "ZZZZZZZZ";

/// 그 동작의 메신저 이벤트들.
fn messenger_events(bench: &Bench, action: &str) -> Vec<Value> {
    events(&bench.store(), KIND_MESSENGER)
        .into_iter()
        .filter(|e| e["action"] == json!(action))
        .collect()
}

/// 그 사람이 처음 말을 걸어 받은 코드.
async fn code_for(tg: &FakeTelegram, sender: i64) -> String {
    tg.push_dm(sender, "안녕하세요");
    let reply = tg.wait_sent(sender, "코드 답", is_pairing_reply).await;
    code_in(&reply).unwrap()
}

// 검증: V88 N8
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 맞는_코드로_승인하면_그_사람만_명령할_수_있다() {
    let tg = FakeTelegram::start().await;
    let bench = Bench::with_token(&tg).await;
    let owner: i64 = 82_345_678;
    bench.pair(&tg, owner).await;

    let status = bench.status().await;
    assert_eq!(status["telegram"]["paired"], json!(true), "{status}");
    let paired = messenger_events(&bench, MESSENGER_ACTION_PAIRED);
    assert_eq!(paired.len(), 1, "승인 이벤트 하나: {paired:?}");
    assert_eq!(paired[0]["channel"], json!("telegram"), "{paired:?}");
    assert_eq!(
        paired[0]["sender_tail"],
        json!("5678"),
        "ID는 뒤 4자리만: {paired:?}"
    );
    assert!(
        !paired[0].to_string().contains(&owner.to_string()),
        "이벤트에 ID 전체가 없다"
    );

    tg.push_dm(owner, "/help");
    tg.wait_sent(owner, "허용된 사람의 도움말", |t| {
        t.contains(MESSENGER_HELP)
    })
    .await;
    let stranger: i64 = 82_000_099;
    tg.push_dm(stranger, "/help");
    tg.wait_sent(stranger, "모르는 사람에게는 코드", is_pairing_reply)
        .await;
    tokio::time::sleep(QUIET).await;
    assert!(
        tg.sent_to(stranger)
            .await
            .iter()
            .all(|t| !t.contains(MESSENGER_HELP)),
        "허용되지 않은 사람의 명령은 읽지 않는다"
    );
    bench.close().await;
}

// 검증: V88
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 틀린_코드는_400이고_남은_횟수를_말하지_않는다() {
    let tg = FakeTelegram::start().await;
    let bench = Bench::with_token(&tg).await;
    let code = code_for(&tg, 82_000_001).await;
    assert_ne!(code, WRONG);

    let reply = bench.approve(WRONG).await;
    assert_eq!(reply.status, 400, "{}", reply.text);
    assert_eq!(reply.body["error"], json!("wrong_code"), "{}", reply.text);
    assert_eq!(
        reply.body["message"],
        json!(MESSENGER_WRONG_CODE),
        "{}",
        reply.text
    );
    assert!(
        !reply.text.contains("남은") && !reply.text.contains("remaining"),
        "남은 횟수를 싣지 않는다: {}",
        reply.text
    );
    let status = bench.status().await;
    assert_eq!(status["telegram"]["paired"], json!(false), "{status}");
    assert_eq!(
        status["telegram"]["pending"],
        json!(1),
        "요청은 그대로: {status}"
    );
    bench.close().await;
}

// 검증: V88
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 다섯_번째_틀리면_423으로_한_시간_잠기고_맞는_코드도_받지_않는다() {
    let tg = FakeTelegram::start().await;
    let bench = Bench::with_token(&tg).await;
    let code = code_for(&tg, 82_000_002).await;

    for round in 1..=4 {
        let reply = bench.approve(WRONG).await;
        assert_eq!(reply.status, 400, "{round}번째: {}", reply.text);
    }
    let locked = bench.approve(WRONG).await;
    assert_eq!(locked.status, 423, "5번째에 잠긴다: {}", locked.text);
    assert_eq!(locked.body["error"], json!("locked"), "{}", locked.text);
    assert_eq!(
        locked.body["message"],
        json!(MESSENGER_LOCKED),
        "{}",
        locked.text
    );
    let until = bench.clock().now_ms() + 3_600_000;
    assert_eq!(
        locked.body["locked_until"],
        json!(until),
        "풀리는 시각(한 시간 뒤, 유닉스 ms): {}",
        locked.text
    );

    let right = bench.approve(&code).await;
    assert_eq!(right.status, 423, "잠긴 동안은 맞는 코드도: {}", right.text);
    let status = bench.status().await;
    assert_eq!(status["telegram"]["paired"], json!(false), "{status}");
    assert_eq!(status["telegram"]["locked_until"], json!(until), "{status}");
    bench.close().await;
}

// 검증: D2 V46 V88
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 다른_기기에서는_승인하거나_뺄_수_없다() {
    let tg = FakeTelegram::start().await;
    let bench = Bench::with_token(&tg).await;
    let code = code_for(&tg, 82_000_003).await;
    let cookie = bench.remote_login().await;

    let body = json!({ "code": code }).to_string();
    let approve = send_with(
        bench.addr(),
        reqwest::Method::POST,
        MESSENGER_PAIRING_APPROVE_PATH,
        &cookie,
        Some(&body),
    )
    .await;
    assert_eq!(approve.status, 403, "{}", approve.text);
    let revoke = send_with(
        bench.addr(),
        reqwest::Method::POST,
        MESSENGER_PAIRING_REVOKE_PATH,
        &cookie,
        None,
    )
    .await;
    assert_eq!(revoke.status, 403, "{}", revoke.text);
    assert!(
        messenger_events(&bench, MESSENGER_ACTION_PAIRED).is_empty(),
        "승인되지 않았다"
    );
    bench.close().await;
}

// 검증: V88
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 빼면_그_사람의_명령을_더는_읽지_않고_이벤트가_남는다() {
    let tg = FakeTelegram::start().await;
    let bench = Bench::with_token(&tg).await;
    let owner: i64 = 82_004_321;
    bench.pair(&tg, owner).await;

    let reply = post_empty(bench.addr(), MESSENGER_PAIRING_REVOKE_PATH).await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    assert_eq!(reply.body["revoked"], json!(true), "{}", reply.text);
    let status = bench.status().await;
    assert_eq!(status["telegram"]["paired"], json!(false), "{status}");
    let revoked = messenger_events(&bench, MESSENGER_ACTION_REVOKED);
    assert_eq!(revoked.len(), 1, "{revoked:?}");
    assert_eq!(revoked[0]["sender_tail"], json!("4321"), "{revoked:?}");

    let id = tg.push_dm(owner, "/help");
    let deadline = std::time::Instant::now() + crate::common::STEP;
    while !tg.offsets().await.iter().flatten().any(|o| *o > id) {
        assert!(
            std::time::Instant::now() < deadline,
            "그 글을 지나가야 한다"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    tokio::time::sleep(QUIET).await;
    assert!(
        tg.sent_to(owner)
            .await
            .iter()
            .all(|t| !t.contains(MESSENGER_HELP)),
        "뺀 사람의 명령은 읽지 않는다"
    );
    bench.close().await;
}

// 검증: V88
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 이미_연결된_사람이_있으면_다른_코드는_409다() {
    let tg = FakeTelegram::start().await;
    let bench = Bench::with_token(&tg).await;
    bench.pair(&tg, 82_000_004).await;
    let other = code_for(&tg, 82_000_005).await;

    let reply = bench.approve(&other).await;
    assert_eq!(reply.status, 409, "허용은 1명: {}", reply.text);
    assert_eq!(reply.body["error"], json!("owner_exists"), "{}", reply.text);
    assert_eq!(
        reply.body["message"],
        json!(MESSENGER_OWNER_EXISTS),
        "{}",
        reply.text
    );
    assert_eq!(
        messenger_events(&bench, MESSENGER_ACTION_PAIRED).len(),
        1,
        "두 번째 사람은 승인되지 않는다"
    );
    bench.close().await;
}
