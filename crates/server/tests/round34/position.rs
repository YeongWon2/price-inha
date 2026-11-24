//! 32차 1번 · 8번: 내 캐릭터 자리 남기기([`OFFICE_ME_POSITION_PATH`]). 범위는 사무실 칸([`OFFICE_COLS`] × [`OFFICE_ROWS`]),
//! 방향은 넷. 다시 켜도 남고(저장소), 두 화면이 남기면 마지막에 온 것이 이긴다(E59).

use price_inha::{OFFICE_COLS, OFFICE_ME_PATH, OFFICE_ME_POSITION_PATH, OFFICE_ROWS};
use serde_json::{Value, json};

use crate::common::{Bench, me, post, post_raw, session};

async fn put(bench: &Bench, cookie: &str, body: &Value) -> u16 {
    let reply = post(bench.addr(), OFFICE_ME_POSITION_PATH, cookie, body).await;
    if reply.status == 200 {
        assert_eq!(
            reply.body,
            me(bench.addr(), cookie).await,
            "답은 다시 읽은 것과 같다"
        );
    }
    reply.status
}

// 검증: V56 C17 M8
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 자리를_남기면_돌려주고_다시_읽어도_같으며_이름_외형은_그대로다() {
    let bench = Bench::start().await;
    let cookie = session(bench.addr()).await;
    let before = me(bench.addr(), &cookie).await;
    let reply = post(
        bench.addr(),
        OFFICE_ME_POSITION_PATH,
        &cookie,
        &json!({"x": 5, "y": 6, "dir": "left"}),
    )
    .await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    assert_eq!(reply.body["x"], 5);
    assert_eq!(reply.body["y"], 6);
    assert_eq!(reply.body["dir"], "left");
    assert_eq!(reply.body["name"], before["name"], "이름은 그대로다");
    assert_eq!(reply.body["look"], before["look"], "외형은 그대로다");
    assert_eq!(
        me(bench.addr(), &cookie).await,
        reply.body,
        "다시 읽어도 같다"
    );
    for dir in ["up", "down", "right", "left"] {
        assert_eq!(
            put(&bench, &cookie, &json!({"x": 1, "y": 2, "dir": dir})).await,
            200,
            "방향 {dir}"
        );
        assert_eq!(me(bench.addr(), &cookie).await["dir"], dir);
    }
    bench.close().await;
}

// 검증: V56 C17 M8
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 사무실_끝_칸까지_받는다() {
    let bench = Bench::start().await;
    let cookie = session(bench.addr()).await;
    let corner = json!({"x": 0, "y": 0, "dir": "down"});
    assert_eq!(put(&bench, &cookie, &corner).await, 200);
    let far = json!({"x": OFFICE_COLS - 1, "y": OFFICE_ROWS - 1, "dir": "up"});
    assert_eq!(put(&bench, &cookie, &far).await, 200);
    let body = me(bench.addr(), &cookie).await;
    assert_eq!(body["x"], OFFICE_COLS - 1);
    assert_eq!(body["y"], OFFICE_ROWS - 1);
    bench.close().await;
}

// 검증: V56 C17 M8 E3
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 범위_밖이거나_모양이_틀린_자리는_400이고_저장하지_않는다() {
    let bench = Bench::start().await;
    let cookie = session(bench.addr()).await;
    assert_eq!(
        put(&bench, &cookie, &json!({"x": 4, "y": 4, "dir": "right"})).await,
        200
    );
    let before = me(bench.addr(), &cookie).await;
    let bad: Vec<Value> = vec![
        json!({"x": OFFICE_COLS, "y": 4, "dir": "up"}),
        json!({"x": 4, "y": OFFICE_ROWS, "dir": "up"}),
        json!({"x": -1, "y": 4, "dir": "up"}),
        json!({"x": 4, "y": -1, "dir": "up"}),
        json!({"x": 1.5, "y": 4, "dir": "up"}),
        json!({"x": "3", "y": 4, "dir": "up"}),
        json!({"x": 4_294_967_296_u64, "y": 4, "dir": "up"}),
        json!({"x": 4, "y": 4, "dir": "north"}),
        json!({"x": 4, "y": 4, "dir": "Up"}),
        json!({"x": 4, "y": 4}),
        json!({"y": 4, "dir": "up"}),
        json!({"x": 4, "dir": "up"}),
        json!([4, 4, "up"]),
    ];
    for body in &bad {
        assert_eq!(
            put(&bench, &cookie, body).await,
            400,
            "자리 {body}는 거부한다"
        );
    }
    let origin = crate::common::origin_of(bench.addr());
    let broken = post_raw(
        bench.addr(),
        OFFICE_ME_POSITION_PATH,
        Some(&cookie),
        Some(&origin),
        "{\"x\": 2,",
    )
    .await;
    assert_eq!(broken.status, 400, "JSON이 아니면 거부한다");
    assert_eq!(
        me(bench.addr(), &cookie).await,
        before,
        "거부한 요청은 아무것도 바꾸지 않는다"
    );
    bench.close().await;
}

// 검증: V56 C17 M8
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 서버를_다시_만들어도_이름_외형_자리가_남는다() {
    let bench = Bench::start().await;
    let cookie = session(bench.addr()).await;
    let looks = crate::common::available_looks(bench.addr(), &cookie).await;
    let look = looks.last().expect("외형이 있다").clone();
    let reply = post(
        bench.addr(),
        OFFICE_ME_PATH,
        &cookie,
        &json!({"name": "다시 켜도", "look": look}),
    )
    .await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    assert_eq!(
        put(&bench, &cookie, &json!({"x": 12, "y": 20, "dir": "right"})).await,
        200
    );
    let saved = me(bench.addr(), &cookie).await;

    let bench = bench.restart().await;
    let cookie = session(bench.addr()).await;
    let again = me(bench.addr(), &cookie).await;
    assert_eq!(again, saved, "다시 켜도 같다");
    assert_eq!(again["x"], 12);
    assert_eq!(again["y"], 20);
    assert_eq!(again["dir"], "right");
    assert_eq!(again["name"], "다시 켜도");
    bench.close().await;
}

// 검증: V56 E59 C17
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 두_화면이_따로_남기면_마지막에_온_자리가_이긴다() {
    let bench = Bench::start().await;
    let first = session(bench.addr()).await;
    let second = session(bench.addr()).await;
    assert_eq!(
        put(&bench, &first, &json!({"x": 3, "y": 3, "dir": "up"})).await,
        200
    );
    assert_eq!(
        put(&bench, &second, &json!({"x": 30, "y": 10, "dir": "left"})).await,
        200
    );
    for cookie in [&first, &second] {
        let body = me(bench.addr(), cookie).await;
        assert_eq!(body["x"], 30, "어느 화면에서 읽어도 마지막 자리다");
        assert_eq!(body["y"], 10);
        assert_eq!(body["dir"], "left");
    }
    assert_eq!(
        put(&bench, &first, &json!({"x": 7, "y": 8, "dir": "down"})).await,
        200
    );
    let body = me(bench.addr(), &second).await;
    assert_eq!((body["x"].clone(), body["y"].clone()), (json!(7), json!(8)));
    bench.close().await;
}
