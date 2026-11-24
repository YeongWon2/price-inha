//! 4번: 담당 외형 저장 API. `GET` [`OFFICE_LOOKS_PATH`] · `POST` [`OFFICE_LOOK_PATH`].
//! 모양은 `src/reach.rs`의 경로 문서 주석에 있다.

use std::collections::BTreeSet;

use price_inha::{OFFICE_LOOK_PATH, OFFICE_LOOKS_PATH};
use serde_json::{Value, json};

use crate::common::{Bench, get, look_ids, origin_of, post, session};

const AGENTS: [&str; 6] = ["news", "strategy", "risk", "review", "trader", "leader"];

/// 응답 `agents`를 (담당 → 외형) 순서로.
fn picked(body: &Value) -> Vec<String> {
    AGENTS
        .iter()
        .map(|a| {
            body["agents"][a]
                .as_str()
                .unwrap_or_else(|| panic!("{a}의 외형이 글로 있다: {body}"))
                .to_owned()
        })
        .collect()
}

async fn looks(bench: &Bench, cookie: &str) -> Value {
    let reply = get(bench.addr(), OFFICE_LOOKS_PATH, Some(cookie)).await;
    assert_eq!(reply.status, 200, "{}", reply.text());
    reply.body
}

async fn choose(bench: &Bench, cookie: &str, agent: &str, look: &str) -> u16 {
    let origin = origin_of(bench.addr());
    post(
        bench.addr(),
        OFFICE_LOOK_PATH,
        Some(cookie),
        Some(&origin),
        &json!({"agent": agent, "look": look}),
    )
    .await
    .status
}

// 검증: V15 V1
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 기본_외형은_여섯이_모두_다르고_원본_외형_50개_안에서_고른다() {
    let bench = Bench::start().await;
    let cookie = session(bench.addr()).await;
    let body = looks(&bench, &cookie).await;
    let defaults = picked(&body);
    let unique: BTreeSet<&String> = defaults.iter().collect();
    assert_eq!(
        unique.len(),
        6,
        "기본 외형은 여섯 모두 다르다: {defaults:?}"
    );
    let available: Vec<String> = body["available"]
        .as_array()
        .expect("고를 수 있는 외형 목록")
        .iter()
        .map(|v| v.as_str().expect("외형 id").to_owned())
        .collect();
    assert_eq!(available.len(), 50, "원본 외형 50종");
    let mut sorted = available.clone();
    sorted.sort();
    assert_eq!(
        sorted,
        look_ids(),
        "목록은 원본 외형 id(해시 표의 characters/office/*.glb)와 같다"
    );
    for look in &defaults {
        assert!(
            available.contains(look),
            "기본 외형 {look}이 목록 안에 있다"
        );
    }
    bench.close().await;
}

// 검증: V15 V1 E3
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 고른_외형은_저장되어_서버를_다시_켜도_같고_다른_담당은_그대로다() {
    let bench = Bench::start().await;
    let cookie = session(bench.addr()).await;
    let before = picked(&looks(&bench, &cookie).await);
    // 기본값이 아닌 외형을 하나 고른다.
    let ids = look_ids();
    let new_look = ids
        .iter()
        .find(|id| !before.contains(id))
        .expect("쓰지 않는 외형이 있다")
        .clone();
    assert_eq!(choose(&bench, &cookie, "risk", &new_look).await, 200);
    let after = picked(&looks(&bench, &cookie).await);
    assert_eq!(after[2], new_look, "고른 외형이 바로 보인다");
    for i in [0, 1, 3, 4, 5] {
        assert_eq!(after[i], before[i], "{}의 외형은 그대로다", AGENTS[i]);
    }
    // 다시 켜도, 새 세션(다른 기기)으로 읽어도 같다.
    let bench = bench.restart().await;
    let cookie = session(bench.addr()).await;
    assert_eq!(
        picked(&looks(&bench, &cookie).await),
        after,
        "서버를 다시 켜도 같다"
    );
    bench.close().await;
}

// 검증: V15 V1
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 목록에_없는_외형과_모르는_담당은_거부하고_저장하지_않는다() {
    let bench = Bench::start().await;
    let cookie = session(bench.addr()).await;
    let before = looks(&bench, &cookie).await;
    for bad in [
        "no-such-look",
        "eunchae-mpfb",
        "../../etc/passwd",
        "",
        "office-male-base.glb",
    ] {
        let status = choose(&bench, &cookie, "news", bad).await;
        assert_eq!(status, 400, "목록에 없는 외형 {bad:?}은 거부한다");
    }
    let known = look_ids().remove(0);
    assert_eq!(
        choose(&bench, &cookie, "ghost", &known).await,
        400,
        "모르는 담당은 거부한다"
    );
    let origin = origin_of(bench.addr());
    let reply = post(
        bench.addr(),
        OFFICE_LOOK_PATH,
        Some(&cookie),
        Some(&origin),
        &json!({"agent": "news"}),
    )
    .await;
    assert_eq!(reply.status, 400, "외형이 빠지면 거부한다");
    assert_eq!(
        looks(&bench, &cookie).await,
        before,
        "거부한 요청은 아무것도 바꾸지 않는다"
    );
    bench.close().await;
}

// 검증: V15 V1
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 세션이_없거나_다른_출처이면_읽기도_바꾸기도_거부한다() {
    let bench = Bench::start().await;
    let cookie = session(bench.addr()).await;
    let before = looks(&bench, &cookie).await;
    let other = look_ids()
        .into_iter()
        .find(|id| !picked(&before).contains(id))
        .expect("쓰지 않는 외형");
    let body = json!({"agent": "news", "look": other});
    let origin = origin_of(bench.addr());

    let anonymous = get(bench.addr(), OFFICE_LOOKS_PATH, None).await;
    assert_eq!(anonymous.status, 401, "세션 없이는 읽을 수 없다");
    let no_session = post(bench.addr(), OFFICE_LOOK_PATH, None, Some(&origin), &body).await;
    assert_eq!(no_session.status, 401, "세션 없이는 바꿀 수 없다");
    let evil = post(
        bench.addr(),
        OFFICE_LOOK_PATH,
        Some(&cookie),
        Some("http://evil.example"),
        &body,
    )
    .await;
    assert!(
        (400..500).contains(&evil.status),
        "다른 출처는 거부한다: {}",
        evil.status
    );
    let none = post(bench.addr(), OFFICE_LOOK_PATH, Some(&cookie), None, &body).await;
    assert!(
        (400..500).contains(&none.status),
        "Origin이 없으면 거부한다: {}",
        none.status
    );
    assert_eq!(
        looks(&bench, &cookie).await,
        before,
        "거부한 요청은 아무것도 바꾸지 않는다"
    );
    bench.close().await;
}
