//! 트리거가 팀을 깨운다: 하루 시작 · 하루 운용 판단 논의 · 마감(T11, V5 · E16).

use std::time::Duration;

use chrono::NaiveDate;
use pi_store::{RoomEvent, Store, TeamState};
use price_inha::{
    DAY_ROOM, DayPlan, DemoTurn, KIND_DAY_CLOSE, KIND_DAY_DISCUSSION, KIND_DAY_PLAN, KIND_DAY_START,
};
use serde_json::{Value, json};

use crate::common::*;

fn on_date(store: &Store, kind: &str, date: NaiveDate) -> Vec<Value> {
    let date = json!(date.format("%Y-%m-%d").to_string());
    events(store, kind)
        .into_iter()
        .filter(|payload| payload["date"] == date)
        .collect()
}

fn day_room(store: &Store) -> Vec<RoomEvent> {
    store.room_events(DAY_ROOM, 0, 1000).unwrap()
}

/// 장 시작 전에 띄우고 장 시작을 지나게 해 하루 운용 판단 논의와 그날 하루 계획(팀 상태를 바꾼 뒤 남는다)이
/// 남을 때까지 기다린다(13차 3번: 논의만 기다리면 상태가 바뀌기 전에 단언할 수 있다).
async fn open_day(bench: &Bench) {
    bench.script(DemoTurn::Verdict, &watch_verdict());
    bench.set(kst(monday(), 9, 1));
    let store = bench.store();
    wait_for(
        "장 시작에 하루 운용 판단 논의와 그날 하루 계획이 남는다",
        || {
            !on_date(&store, KIND_DAY_DISCUSSION, monday()).is_empty()
                && !on_date(&store, KIND_DAY_PLAN, monday()).is_empty()
        },
    )
    .await;
}

// 검증: V5, T11
#[tokio::test]
async fn 아무것도_누르지_않아도_하루_시작과_논의와_마감_기록이_남는다() {
    let bench = Bench::start(kst(monday(), 8, 50)).await;
    let store = bench.store();
    assert!(
        on_date(&store, KIND_DAY_DISCUSSION, monday()).is_empty(),
        "장 시작 전이다"
    );

    open_day(&bench).await;
    assert_eq!(on_date(&store, KIND_DAY_START, monday()).len(), 1);
    let discussion = on_date(&store, KIND_DAY_DISCUSSION, monday());
    assert_eq!(discussion.len(), 1, "그날 한 번");
    assert_eq!(discussion[0]["plan"], json!("trade"));
    assert!(
        !discussion[0]["text"]
            .as_str()
            .unwrap_or_default()
            .is_empty(),
        "팀장의 글이 남는다"
    );
    let leader = day_room(&store)
        .into_iter()
        .filter(|e| e.kind == "message.member" && e.actor_id == "leader")
        .count();
    assert_eq!(leader, 1, "팀장의 하루 판단이 방 로그에 남는다");
    assert_eq!(store.team_state().unwrap(), TeamState::Operating);

    bench.set(kst(monday(), 15, 31));
    wait_for("마감 기록이 남는다", || {
        on_date(&store, KIND_DAY_CLOSE, monday()).len() == 1
    })
    .await;
    bench.close().await;
}

// 검증: V5, V7, E19
#[tokio::test]
async fn 팀장이_쉬기로_정하면_쉬는_이유가_남고_쉬는_중이다() {
    let bench = Bench::start(kst(monday(), 8, 50)).await;
    let reason = "변동성이 커서 오늘은 쉰다";
    bench.script(
        DemoTurn::DayPlan,
        &json!({"plan": "rest", "reason": reason}).to_string(),
    );

    open_day(&bench).await;
    let store = bench.store();
    let discussion = on_date(&store, KIND_DAY_DISCUSSION, monday());
    assert_eq!(discussion[0]["plan"], json!("rest"));
    assert_eq!(discussion[0]["reason"], json!(reason));
    assert_eq!(store.team_state().unwrap(), TeamState::Resting);
    bench.close().await;
}

// 검증: E16, T11
#[tokio::test]
async fn 다시_켜도_그날의_하루_판단은_한_번뿐이다() {
    let bench = Bench::start(kst(monday(), 8, 50)).await;
    open_day(&bench).await;
    let dir = bench.close().await;

    let bench = Bench::with(dir, kst(monday(), 9, 10), None).await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    let plan = bench.crew().plan_day().await.expect("하루 판단");
    assert_eq!(plan, DayPlan::Trade, "그날 정한 판단을 돌려준다");
    let store = bench.store();
    assert_eq!(
        on_date(&store, KIND_DAY_DISCUSSION, monday()).len(),
        1,
        "같은 날 두 번 논의하지 않는다"
    );
    let asked = bench
        .ai()
        .prompts()
        .iter()
        .filter(|p| p.contains("[하루 판단]"))
        .count();
    assert_eq!(asked, 0, "다시 켠 뒤 팀장에게 다시 묻지 않는다");
    bench.close().await;
}
