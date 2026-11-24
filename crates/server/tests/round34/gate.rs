//! 32차 1번: 내 캐릭터 API의 관문은 사무실의 다른 API와 같다 — 세션이 없으면(원격 포함) 401, 쓰기는 같은 출처만.

use price_inha::{OFFICE_ME_PATH, OFFICE_ME_POSITION_PATH};
use serde_json::json;

use crate::common::{Bench, get, me, origin_of, post_raw, session};

// 검증: V56 C17
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 세션이_없으면_읽기도_바꾸기도_401이다() {
    let bench = Bench::start().await;
    let origin = origin_of(bench.addr());
    let profile = json!({"name": "몰래"}).to_string();
    let spot = json!({"x": 2, "y": 2, "dir": "up"}).to_string();
    assert_eq!(get(bench.addr(), OFFICE_ME_PATH, None).await.status, 401);
    let reply = post_raw(bench.addr(), OFFICE_ME_PATH, None, Some(&origin), &profile).await;
    assert_eq!(reply.status, 401);
    let reply = post_raw(
        bench.addr(),
        OFFICE_ME_POSITION_PATH,
        None,
        Some(&origin),
        &spot,
    )
    .await;
    assert_eq!(reply.status, 401);
    let cookie = session(bench.addr()).await;
    let body = me(bench.addr(), &cookie).await;
    assert_eq!(body["name"], "나", "거부한 요청은 저장하지 않는다");
    assert_ne!((body["x"].clone(), body["y"].clone()), (json!(2), json!(2)));
    bench.close().await;
}

// 검증: V56 C17
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 원격에서_세션이_없으면_401이다() {
    let bench = Bench::start().await;
    let origin = origin_of(bench.addr());
    bench.go_remote();
    assert_eq!(get(bench.addr(), OFFICE_ME_PATH, None).await.status, 401);
    let spot = json!({"x": 2, "y": 2, "dir": "up"}).to_string();
    let reply = post_raw(
        bench.addr(),
        OFFICE_ME_POSITION_PATH,
        None,
        Some(&origin),
        &spot,
    )
    .await;
    assert_eq!(reply.status, 401);
    let profile = json!({"name": "원격"}).to_string();
    let reply = post_raw(bench.addr(), OFFICE_ME_PATH, None, Some(&origin), &profile).await;
    assert_eq!(reply.status, 401);
    bench.close().await;
}

// 검증: V56 C17
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 다른_출처나_출처_없는_쓰기는_거부하고_저장하지_않는다() {
    let bench = Bench::start().await;
    let cookie = session(bench.addr()).await;
    let before = me(bench.addr(), &cookie).await;
    // 포트가 다르면 다른 출처다(로컬 주소만 쓴다).
    let other = "http://127.0.0.1:9";
    let spot = json!({"x": 2, "y": 2, "dir": "up"}).to_string();
    let profile = json!({"name": "남의 출처"}).to_string();
    for (path, body) in [(OFFICE_ME_POSITION_PATH, &spot), (OFFICE_ME_PATH, &profile)] {
        for origin in [Some(other), None] {
            let reply = post_raw(bench.addr(), path, Some(&cookie), origin, body).await;
            assert!(
                (400..500).contains(&reply.status),
                "{path} 출처 {origin:?}: {}",
                reply.status
            );
        }
    }
    assert_eq!(
        me(bench.addr(), &cookie).await,
        before,
        "거부한 요청은 아무것도 바꾸지 않는다"
    );
    bench.close().await;
}
