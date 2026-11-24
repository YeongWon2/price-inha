//! 31차 고침(결정 22): 알림음 켜기 · 끄기는 서버가 둔다(`GET` · `POST /api/setup/sound`). 기본은 켜짐이고, 바꾼
//! 값은 보관 폴더에 남아 서버를 다시 만들어도 같다. `enabled`가 참 · 거짓이 아니면 400이고 값은 그대로다. 원격은
//! 세션 없이 401이고, 응답에는 `enabled` 하나뿐이라 등록한 키 값이 없다.

use price_inha::SETUP_SOUND_PATH;
use serde_json::json;

use crate::common::{Bench, KEY_VALUES, Reply, get_anonymous, get_with, keys_of, login, post_with};

/// 세션으로 읽어 200 · `{"enabled": bool}` 모양인지 보고 그 값을 돌려준다.
async fn read(bench: &Bench, cookie: &str) -> bool {
    let reply = get_with(bench.addr(), SETUP_SOUND_PATH, cookie).await;
    assert_eq!(reply.status, 200, "알림음 설정을 읽는다: {}", reply.text);
    assert_eq!(
        keys_of(&reply.body),
        ["enabled"],
        "칸은 enabled 하나뿐이다: {}",
        reply.body
    );
    reply.body["enabled"]
        .as_bool()
        .unwrap_or_else(|| panic!("enabled는 참 · 거짓이다: {}", reply.body))
}

/// 세션으로 `{"enabled": on}`을 보낸다.
async fn write(bench: &Bench, cookie: &str, on: bool) -> Reply {
    let body = json!({ "enabled": on }).to_string();
    post_with(bench.addr(), SETUP_SOUND_PATH, Some(cookie), &body).await
}

/// 응답 글자에 등록한 키 값이 하나도 없다.
fn no_key_values(reply: &Reply) {
    for value in KEY_VALUES {
        assert!(
            !reply.text.contains(value),
            "응답에 키 값이 없다: {}",
            reply.text
        );
    }
}

// 검증: V78 R18-4
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 알림음은_저장한_값이_없으면_켜짐이다() {
    let bench = Bench::with_keys().await;
    let cookie = login(bench.addr()).await;
    assert!(read(&bench, &cookie).await, "기본은 켜짐");
    bench.close().await;
}

// 검증: V78 R18-4
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 알림음을_끄면_답과_다시_읽은_값이_꺼짐이고_켜면_켜짐이다() {
    let bench = Bench::with_keys().await;
    let cookie = login(bench.addr()).await;
    let reply = write(&bench, &cookie, false).await;
    assert_eq!(reply.status, 200, "끄기를 받는다: {}", reply.text);
    assert_eq!(
        reply.body,
        json!({ "enabled": false }),
        "답은 저장한 값이다"
    );
    assert!(!read(&bench, &cookie).await, "다시 읽어도 꺼짐");
    let reply = write(&bench, &cookie, true).await;
    assert_eq!(reply.status, 200, "켜기를 받는다: {}", reply.text);
    assert_eq!(reply.body, json!({ "enabled": true }), "답은 저장한 값이다");
    assert!(read(&bench, &cookie).await, "다시 읽으면 켜짐");
    bench.close().await;
}

// 검증: V78 R18-4
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 끈_알림음은_같은_보관_폴더로_서버를_다시_만들어도_꺼짐이다() {
    let bench = Bench::with_keys().await;
    let cookie = login(bench.addr()).await;
    let reply = write(&bench, &cookie, false).await;
    assert_eq!(reply.status, 200, "끄기를 받는다: {}", reply.text);
    let bench = bench.restart().await;
    let cookie = login(bench.addr()).await;
    assert!(!read(&bench, &cookie).await, "다시 만든 서버도 꺼짐");
    bench.close().await;
}

// 검증: V78 E57
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 참_거짓이_아닌_enabled는_400이고_값은_그대로다() {
    let bench = Bench::with_keys().await;
    let cookie = login(bench.addr()).await;
    let reply = write(&bench, &cookie, false).await;
    assert_eq!(reply.status, 200, "끄기를 받는다: {}", reply.text);
    let bad = [
        json!({ "enabled": "true" }).to_string(),
        json!({ "enabled": 1 }).to_string(),
        json!({ "enabled": null }).to_string(),
        json!({}).to_string(),
        json!({ "on": true }).to_string(),
        json!(true).to_string(),
        "{\"enabled\":".to_string(),
    ];
    for body in &bad {
        let reply = post_with(bench.addr(), SETUP_SOUND_PATH, Some(&cookie), body).await;
        assert_eq!(reply.status, 400, "{body}은 거절한다: {}", reply.text);
        assert!(!read(&bench, &cookie).await, "{body} 뒤에도 꺼짐 그대로");
    }
    bench.close().await;
}

// 검증: V78 C15 E38
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 원격에서_세션_없이_알림음을_읽거나_바꾸면_401이고_값은_그대로다() {
    let bench = Bench::with_keys().await;
    bench.go_remote();
    let reply = get_anonymous(bench.addr(), SETUP_SOUND_PATH).await;
    assert_eq!(
        reply.status, 401,
        "세션 없는 원격은 읽지 못한다: {}",
        reply.text
    );
    let body = json!({ "enabled": false }).to_string();
    let reply = post_with(bench.addr(), SETUP_SOUND_PATH, None, &body).await;
    assert_eq!(
        reply.status, 401,
        "세션 없는 원격은 바꾸지 못한다: {}",
        reply.text
    );
    let cookie = bench.remote_login().await;
    assert!(read(&bench, &cookie).await, "바꾸지 못했으니 켜짐 그대로");
    bench.close().await;
}

// 검증: V78 V46 D11 N8
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 알림음_응답에는_등록한_키_값이_없다() {
    let bench = Bench::with_keys().await;
    let cookie = login(bench.addr()).await;
    let reply = get_with(bench.addr(), SETUP_SOUND_PATH, &cookie).await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    no_key_values(&reply);
    let reply = write(&bench, &cookie, false).await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    assert_eq!(
        keys_of(&reply.body),
        ["enabled"],
        "칸은 enabled 하나뿐이다: {}",
        reply.body
    );
    no_key_values(&reply);
    bench.close().await;
}
