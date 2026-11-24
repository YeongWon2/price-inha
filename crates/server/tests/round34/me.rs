//! 32차 1번: 내 캐릭터 읽기 · 이름 · 외형 바꾸기([`OFFICE_ME_PATH`]). 모양은 `src/reach.rs`의 경로 문서 주석에 있다.

use pi_store::{PLAYER_DEFAULT_NAME, PLAYER_NAME_MAX_CHARS};
use price_inha::{OFFICE_ENTRANCE, OFFICE_ENTRANCE_DIR, OFFICE_ME_PATH, PLAYER_DEFAULT_LOOK};
use serde_json::{Value, json};

use crate::common::{Bench, available_looks, keys_of, me, post, session};

/// 기본 외형이 아닌 원본 외형 하나.
async fn other_look(bench: &Bench, cookie: &str, not: &[&str]) -> String {
    available_looks(bench.addr(), cookie)
        .await
        .into_iter()
        .find(|id| !not.contains(&id.as_str()))
        .expect("다른 외형이 있다")
}

// 검증: V56 C17 M8
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 처음_읽으면_기본_이름_외형과_입구_자리다() {
    let bench = Bench::start().await;
    let cookie = session(bench.addr()).await;
    let body = me(bench.addr(), &cookie).await;
    assert_eq!(keys_of(&body), ["dir", "look", "name", "x", "y"], "{body}");
    assert_eq!(body["name"], PLAYER_DEFAULT_NAME);
    assert_eq!(body["name"], "나");
    assert_eq!(body["look"], PLAYER_DEFAULT_LOOK);
    assert_eq!(body["x"], OFFICE_ENTRANCE.0);
    assert_eq!(body["y"], OFFICE_ENTRANCE.1);
    assert_eq!(body["dir"], OFFICE_ENTRANCE_DIR);
    let looks = available_looks(bench.addr(), &cookie).await;
    assert!(
        looks.iter().any(|id| id == PLAYER_DEFAULT_LOOK),
        "기본 외형은 원본 외형 목록 안이다"
    );
    bench.close().await;
}

// 검증: V56 C17 M8
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 이름과_외형을_바꾸면_돌려주고_다시_읽어도_같으며_자리는_그대로다() {
    let bench = Bench::start().await;
    let cookie = session(bench.addr()).await;
    let look = other_look(&bench, &cookie, &[PLAYER_DEFAULT_LOOK]).await;
    let reply = post(
        bench.addr(),
        OFFICE_ME_PATH,
        &cookie,
        &json!({"name": "영원", "look": look}),
    )
    .await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    assert_eq!(keys_of(&reply.body), ["dir", "look", "name", "x", "y"]);
    assert_eq!(reply.body["name"], "영원");
    assert_eq!(reply.body["look"], look.as_str());
    assert_eq!(reply.body["x"], OFFICE_ENTRANCE.0, "자리는 그대로다");
    assert_eq!(reply.body["y"], OFFICE_ENTRANCE.1);
    assert_eq!(
        me(bench.addr(), &cookie).await,
        reply.body,
        "다시 읽어도 같다"
    );
    bench.close().await;
}

// 검증: V56 C17 M8
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 이름만_또는_외형만_보내면_다른_칸은_그대로다() {
    let bench = Bench::start().await;
    let cookie = session(bench.addr()).await;
    let first = other_look(&bench, &cookie, &[PLAYER_DEFAULT_LOOK]).await;
    let reply = post(
        bench.addr(),
        OFFICE_ME_PATH,
        &cookie,
        &json!({"look": first}),
    )
    .await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    assert_eq!(reply.body["name"], PLAYER_DEFAULT_NAME, "이름은 그대로다");
    assert_eq!(reply.body["look"], first.as_str());

    let reply = post(
        bench.addr(),
        OFFICE_ME_PATH,
        &cookie,
        &json!({"name": "가격인하"}),
    )
    .await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    assert_eq!(reply.body["name"], "가격인하");
    assert_eq!(reply.body["look"], first.as_str(), "외형은 그대로다");
    assert_eq!(me(bench.addr(), &cookie).await, reply.body);
    bench.close().await;
}

// 검증: V56 C17 M8 E3
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 이름은_저장소_한도까지_받고_넘거나_비면_400이고_저장하지_않는다() {
    let bench = Bench::start().await;
    let cookie = session(bench.addr()).await;
    let longest = "가".repeat(PLAYER_NAME_MAX_CHARS);
    let reply = post(
        bench.addr(),
        OFFICE_ME_PATH,
        &cookie,
        &json!({"name": longest}),
    )
    .await;
    assert_eq!(reply.status, 200, "한도 길이는 받는다: {}", reply.text);
    let before = me(bench.addr(), &cookie).await;
    assert_eq!(before["name"], longest.as_str());

    let too_long = "가".repeat(PLAYER_NAME_MAX_CHARS + 1);
    let bad_names: [Value; 4] = [json!(too_long), json!(""), json!(7), json!(null)];
    for name in bad_names {
        let reply = post(
            bench.addr(),
            OFFICE_ME_PATH,
            &cookie,
            &json!({"name": name}),
        )
        .await;
        assert_eq!(reply.status, 400, "이름 {name}은 거부한다: {}", reply.text);
    }
    assert_eq!(
        me(bench.addr(), &cookie).await,
        before,
        "거부한 요청은 아무것도 바꾸지 않는다"
    );
    bench.close().await;
}

// 검증: V56 C17 M8 E3
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 목록에_없는_외형은_400이고_이름도_함께_저장하지_않는다() {
    let bench = Bench::start().await;
    let cookie = session(bench.addr()).await;
    let before = me(bench.addr(), &cookie).await;
    let bad_looks: [Value; 5] = [
        json!("no-such-look"),
        json!(""),
        json!("../../etc/passwd"),
        json!("office-jun.glb"),
        json!(3),
    ];
    for look in bad_looks {
        let reply = post(
            bench.addr(),
            OFFICE_ME_PATH,
            &cookie,
            &json!({"name": "바뀌면 안 됨", "look": look}),
        )
        .await;
        assert_eq!(reply.status, 400, "외형 {look}은 거부한다: {}", reply.text);
    }
    let reply = post(bench.addr(), OFFICE_ME_PATH, &cookie, &json!("나")).await;
    assert_eq!(reply.status, 400, "객체가 아니면 거부한다");
    assert_eq!(
        me(bench.addr(), &cookie).await,
        before,
        "거부한 요청은 아무것도 바꾸지 않는다"
    );
    bench.close().await;
}
