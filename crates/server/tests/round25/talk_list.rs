//! 1번 둘째 줄: 화면을 새로 열면 `GET` [`OFFICE_TALK_PATH`]로 최근 [`OFFICE_TALK_RECENT`]줄을 시각 순서로 받는다.
//! 저장된 [`KIND_OFFICE_TALK`] 이벤트에서 읽으므로 다시 켜도 같다. 세션이 없으면 401이다.

use price_inha::{KIND_OFFICE_TALK, OFFICE_TALK_PATH, OFFICE_TALK_RECENT};
use serde_json::Value;

use crate::common::{
    Bench, Plan, WATCHED, events, get, get_anonymous, kst, monday, plus, wait_for,
};

/// 저장된 `office.talk`가 `at_least`줄 넘게 쌓일 때까지 논의를 연다(논의마다 시계를 1분씩 옮긴다).
async fn pile_up(bench: &Bench, at_least: usize) {
    for round in 0..40 {
        if events(&bench.store(), KIND_OFFICE_TALK).len() > at_least {
            return;
        }
        bench.set(plus(bench.now(), 60));
        let _outcome = bench
            .crew()
            .discuss(WATCHED, &format!("쌓기 {round}"))
            .await;
    }
    wait_for("office.talk가 충분히 쌓인다", || {
        events(&bench.store(), KIND_OFFICE_TALK).len() > at_least
    })
    .await;
}

// 검증: V14 C2 E3
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 최근_50줄을_시각_순서로_주고_다시_켜도_같다() {
    let bench = Plan::demo().start(kst(monday(), 10, 0)).await;
    bench.ready().await;
    let empty = get(bench.addr(), OFFICE_TALK_PATH).await;
    assert_eq!(empty.status, 200, "{}", empty.text);
    assert_eq!(
        empty.body.as_array().map(Vec::len),
        Some(0),
        "논의 전에는 빈 목록이다: {}",
        empty.text
    );

    pile_up(&bench, OFFICE_TALK_RECENT + 5).await;
    let stored = events(&bench.store(), KIND_OFFICE_TALK);
    let recent: Vec<Value> = stored[stored.len() - OFFICE_TALK_RECENT..].to_vec();

    let reply = get(bench.addr(), OFFICE_TALK_PATH).await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    let lines = reply.body.as_array().cloned().expect("목록");
    assert_eq!(lines.len(), OFFICE_TALK_RECENT, "최근 50줄만 준다");
    assert_eq!(lines, recent, "저장된 office.talk의 최근 50줄 그대로다");
    let times: Vec<i64> = lines.iter().map(|l| l["at"].as_i64().unwrap()).collect();
    assert!(
        times.windows(2).all(|w| w[0] <= w[1]),
        "시각 순서(오래된 것 먼저)다: {times:?}"
    );
    assert!(
        times.first() < times.last(),
        "여러 논의의 시각이 섞여 있다(순서를 볼 수 있다)"
    );

    let at = bench.now();
    let bench = bench.reopen(at).await;
    let again = get(bench.addr(), OFFICE_TALK_PATH).await;
    assert_eq!(again.status, 200, "{}", again.text);
    assert_eq!(
        again.body.as_array().cloned().unwrap_or_default(),
        recent,
        "다시 켜도 저장된 줄을 준다"
    );
    bench.close().await;
}

// 검증: C2 D2
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 세션이_없으면_401이다() {
    let bench = Plan::demo().start(kst(monday(), 10, 0)).await;
    let reply = get_anonymous(bench.addr(), OFFICE_TALK_PATH).await;
    assert_eq!(
        reply.status, 401,
        "세션이 없으면 열리지 않는다: {}",
        reply.text
    );
    let with = get(bench.addr(), OFFICE_TALK_PATH).await;
    assert_eq!(with.status, 200, "세션이 있으면 열린다: {}", with.text);
    bench.close().await;
}
