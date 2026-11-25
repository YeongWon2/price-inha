//! 39차 1번: 봇 토큰 등록 · 지우기(결정 24 · V87 · N8 · E38).

use pi_secrets::SecretStore;
use price_inha::{
    KIND_MESSENGER, MESSENGER_ACTION_TOKEN_REMOVED, MESSENGER_ACTION_TOKEN_SET,
    MESSENGER_BOOK_FILE, MESSENGER_PATH, MESSENGER_TELEGRAM_PATH, TELEGRAM_KEYS,
};
use serde_json::json;

use crate::common::{
    BOT, Bench, FakeTelegram, QUIET, TOKEN, WRONG_TOKEN, delete, events, get_with,
    plain_files_with, send_with,
};

// 검증: V87 N8 E38 C3
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 토큰은_확인한_뒤에_저장되고_받기를_시작한다() {
    let tg = FakeTelegram::start().await;
    let bench = Bench::start(&tg).await;

    let reply = bench.register(TOKEN).await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    assert_eq!(reply.body["registered"], json!(true), "{}", reply.text);
    assert_eq!(
        reply.body["bot"],
        json!(BOT),
        "봇 이름을 돌려준다: {}",
        reply.text
    );
    assert!(!reply.text.contains(TOKEN), "응답에 토큰이 없다");
    assert!(
        tg.count("getMe").await >= 1,
        "저장하기 전에 getMe로 확인한다"
    );

    bench.wait_state("polling").await;
    tg.wait_count("getUpdates", 1).await;
    let status = bench.status().await;
    assert_eq!(status["telegram"]["configured"], json!(true), "{status}");
    assert_eq!(status["telegram"]["bot"], json!(BOT), "{status}");

    let saved = SecretStore::open(bench.home())
        .unwrap()
        .read_provider(TELEGRAM_KEYS)
        .unwrap()
        .expect("비밀 보관소에 저장된다");
    assert_eq!(saved.get("bot_token").unwrap().expose(), TOKEN);

    let set = events(&bench.store(), KIND_MESSENGER);
    assert!(
        set.iter()
            .any(|e| e["action"] == json!(MESSENGER_ACTION_TOKEN_SET)),
        "토큰 저장 이벤트: {set:?}"
    );
    assert!(
        set.iter().all(|e| !e.to_string().contains(TOKEN)),
        "이벤트에 토큰이 없다"
    );
    assert!(
        plain_files_with(bench.home(), TOKEN).is_empty(),
        "홈 폴더의 비밀 아닌 파일에 토큰 글자가 없다"
    );
    bench.close().await;
}

// 검증: V87 E4 C3
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 틀린_토큰은_저장되지_않고_받기를_시작하지_않는다() {
    let tg = FakeTelegram::start().await;
    let bench = Bench::start(&tg).await;

    let reply = bench.register(WRONG_TOKEN).await;
    assert_eq!(reply.status, 422, "{}", reply.text);
    assert_eq!(reply.body["error"], json!("key_rejected"), "{}", reply.text);
    assert!(!reply.text.contains(WRONG_TOKEN), "응답에 토큰이 없다");
    assert!(tg.count("getMe").await >= 1, "확인은 했다");

    let status = bench.status().await;
    assert_eq!(status["telegram"]["configured"], json!(false), "{status}");
    assert_eq!(status["telegram"]["state"], json!("off"), "{status}");
    let saved = SecretStore::open(bench.home())
        .unwrap()
        .read_provider(TELEGRAM_KEYS)
        .unwrap();
    assert!(saved.is_none(), "저장하지 않는다");
    tokio::time::sleep(QUIET).await;
    assert_eq!(tg.count("getUpdates").await, 0, "받기를 시작하지 않는다");
    assert!(
        events(&bench.store(), KIND_MESSENGER)
            .iter()
            .all(|e| e["action"] != json!(MESSENGER_ACTION_TOKEN_SET)),
        "저장 이벤트가 없다"
    );
    assert!(plain_files_with(bench.home(), WRONG_TOKEN).is_empty());
    bench.close().await;
}

// 검증: D2 V46 C16
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 다른_기기에서는_읽기만_하고_토큰을_등록하거나_지울_수_없다() {
    let tg = FakeTelegram::start().await;
    let bench = Bench::start(&tg).await;
    let cookie = bench.remote_login().await;

    let status = get_with(bench.addr(), MESSENGER_PATH, &cookie).await;
    assert_eq!(status.status, 200, "원격도 읽을 수는 있다: {}", status.text);
    assert_eq!(status.body["editable"], json!(false), "{}", status.text);

    let body = json!({ "token": TOKEN }).to_string();
    let post = send_with(
        bench.addr(),
        reqwest::Method::POST,
        MESSENGER_TELEGRAM_PATH,
        &cookie,
        Some(&body),
    )
    .await;
    assert_eq!(post.status, 403, "같은 컴퓨터에서만 등록: {}", post.text);
    let removed = send_with(
        bench.addr(),
        reqwest::Method::DELETE,
        MESSENGER_TELEGRAM_PATH,
        &cookie,
        None,
    )
    .await;
    assert_eq!(
        removed.status, 403,
        "같은 컴퓨터에서만 지움: {}",
        removed.text
    );

    assert_eq!(tg.count("getMe").await, 0, "확인하러 나가지도 않는다");
    let saved = SecretStore::open(bench.home())
        .unwrap()
        .read_provider(TELEGRAM_KEYS)
        .unwrap();
    assert!(saved.is_none(), "저장하지 않는다");
    bench.close().await;
}

// 검증: V88 C3
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 지우면_토큰과_페어링_장부가_없어지고_받기가_멈춘다() {
    let tg = FakeTelegram::start().await;
    let bench = Bench::with_token(&tg).await;
    bench.pair(&tg, 70_001_234).await;
    assert!(
        bench.home().join(MESSENGER_BOOK_FILE).exists(),
        "페어링 장부는 홈 폴더의 파일이다"
    );

    let reply = delete(bench.addr(), MESSENGER_TELEGRAM_PATH).await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    assert_eq!(reply.body["removed"], json!(true), "{}", reply.text);

    assert!(
        !bench.home().join(MESSENGER_BOOK_FILE).exists(),
        "장부도 함께 지운다"
    );
    let saved = SecretStore::open(bench.home())
        .unwrap()
        .read_provider(TELEGRAM_KEYS)
        .unwrap();
    assert!(saved.is_none(), "토큰을 지운다");
    let status = bench.status().await;
    assert_eq!(status["telegram"]["configured"], json!(false), "{status}");
    assert_eq!(status["telegram"]["paired"], json!(false), "{status}");
    assert_eq!(status["telegram"]["state"], json!("off"), "{status}");
    assert!(
        events(&bench.store(), KIND_MESSENGER)
            .iter()
            .any(|e| e["action"] == json!(MESSENGER_ACTION_TOKEN_REMOVED)),
        "지움 이벤트"
    );

    tokio::time::sleep(QUIET).await;
    let polled = tg.count("getUpdates").await;
    tokio::time::sleep(QUIET).await;
    assert_eq!(tg.count("getUpdates").await, polled, "받기가 멈췄다");
    bench.close().await;
}
