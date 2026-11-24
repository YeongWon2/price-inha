//! 2번 · 1번: 실행 파일의 시연 조립으로 켜면 주말 · 밤에도 깨우기 엔진 첫 tick이 성공하고(503으로 실패하지 않는다),
//! 장 시간이면 팀이 하루를 시작한다(하루 시작 이벤트 · 하루 판단 · 하루 판단 방의 `office.talk` topic `day`).
//! 자정 앞뒤 10분은 장 밖이라 하루를 시작하지 않고, 23:55에 하루를 마감한 뒤 00:05에 다음 날을 다시 시작한다.

use chrono::NaiveDate;
use price_inha::{DAY_ROOM, KIND_DAY_CLOSE, KIND_DAY_PLAN, KIND_DAY_START, KIND_OFFICE_TALK};
use serde_json::Value;

use crate::common::{
    Bench, OPEN_JOB, Plan, events, kst, on_date, saturday, stays_false, sunday, tuesday, wait_for,
};

/// 하루 판단 방의 `office.talk`들.
fn day_talks(bench: &Bench) -> Vec<Value> {
    events(&bench.store(), KIND_OFFICE_TALK)
        .into_iter()
        .filter(|talk| talk["topic"] == "day")
        .collect()
}

/// 띄운 바로 뒤(기다리지 않고) 첫 tick이 성공해 깨우기 작업 상태가 저장돼 있는지 본다.
fn first_tick_succeeded(bench: &Bench) {
    let jobs = bench.trigger_jobs();
    assert!(
        jobs.iter().any(|job| job == OPEN_JOB),
        "띄울 때 돈 첫 tick이 성공해 장 시작 작업 상태가 저장된다(달력 503이면 남지 않는다): {jobs:?}"
    );
}

/// 그 날 하루를 시작했다: 하루 시작 한 번 · 하루 판단 · 하루 판단 방의 담당 글 `office.talk`.
async fn day_begins(bench: &Bench, date: NaiveDate) {
    let store = bench.store();
    wait_for("하루 시작 이벤트", || {
        on_date(&store, KIND_DAY_START, date) >= 1
    })
    .await;
    wait_for("하루 판단", || {
        on_date(&store, KIND_DAY_PLAN, date) >= 1
    })
    .await;
    wait_for("하루 판단 방의 office.talk", || {
        !day_talks(bench).is_empty()
    })
    .await;
    assert_eq!(
        on_date(&store, KIND_DAY_START, date),
        1,
        "{date}: 하루 시작은 한 번만 남는다"
    );
    for talk in day_talks(bench) {
        assert_eq!(talk["room"], DAY_ROOM, "하루 판단 방의 글이다: {talk}");
        assert!(
            talk["agent"]
                .as_str()
                .is_some_and(|agent| !agent.is_empty()),
            "말한 담당이 있다: {talk}"
        );
        assert!(
            talk["text"].as_str().is_some_and(|text| !text.is_empty()),
            "글이 있다: {talk}"
        );
    }
}

// 검증: V54 D14 C33 N19
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 시연_조립으로_토요일_밤에_켜도_첫_tick이_성공하고_팀이_하루를_시작한다() {
    let bench = Plan::demo().start(kst(saturday(), 22, 30)).await;
    first_tick_succeeded(&bench);
    day_begins(&bench, saturday()).await;
    bench.close().await;
}

// 검증: V54 D14 C33 N19
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 시연_조립으로_평일_새벽에_켜도_첫_tick이_성공하고_팀이_하루를_시작한다() {
    let bench = Plan::demo().start(kst(tuesday(), 2, 0)).await;
    first_tick_succeeded(&bench);
    day_begins(&bench, tuesday()).await;
    bench.close().await;
}

// 검증: V54 N19 C33
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 자정_10분_전에_켜면_첫_tick은_성공하지만_장_밖이라_하루를_시작하지_않는다() {
    let bench = Plan::demo().start(kst(saturday(), 23, 58)).await;
    first_tick_succeeded(&bench);
    let store = bench.store();
    stays_false("장 밖에서 하루 시작", || {
        on_date(&store, KIND_DAY_START, saturday()) + on_date(&store, KIND_DAY_START, sunday()) > 0
    })
    .await;
    assert!(
        day_talks(&bench).is_empty(),
        "장 밖에서는 하루 판단 논의가 없다"
    );
    bench.close().await;
}

// 검증: V54 N19 C33
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 자정_10분_뒤에_켜도_장_밖이라_하루를_시작하지_않는다() {
    let bench = Plan::demo().start(kst(sunday(), 0, 2)).await;
    first_tick_succeeded(&bench);
    let store = bench.store();
    stays_false("장 밖에서 하루 시작", || {
        on_date(&store, KIND_DAY_START, saturday()) + on_date(&store, KIND_DAY_START, sunday()) > 0
    })
    .await;
    assert!(
        day_talks(&bench).is_empty(),
        "장 밖에서는 하루 판단 논의가 없다"
    );
    bench.close().await;
}

// 검증: V54 N19 D14
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 시연_하루는_23시_55분에_마감하고_다음_날_00시_05분에_다시_시작한다() {
    let bench = Plan::demo().start(kst(saturday(), 23, 50)).await;
    first_tick_succeeded(&bench);
    let store = bench.store();
    wait_for("토요일 하루 시작", || {
        on_date(&store, KIND_DAY_START, saturday()) == 1
    })
    .await;

    bench.set(kst(saturday(), 23, 57));
    wait_for("토요일 하루 마감", || {
        on_date(&store, KIND_DAY_CLOSE, saturday()) == 1
    })
    .await;
    assert_eq!(
        on_date(&store, KIND_DAY_START, sunday()),
        0,
        "자정 앞 10분에는 다음 날을 시작하지 않는다"
    );

    bench.set(kst(sunday(), 0, 6));
    wait_for("일요일 하루 시작", || {
        on_date(&store, KIND_DAY_START, sunday()) == 1
    })
    .await;
    assert_eq!(
        on_date(&store, KIND_DAY_CLOSE, saturday()),
        1,
        "토요일 마감은 한 번만 남는다"
    );
    bench.close().await;
}
