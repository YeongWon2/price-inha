//! 2번: 시연 조립에서 팀장의 하루 판단 · 종목 판단 글은 `office.talk`의 `text`로 사람이 읽는 문장으로 나오고,
//! 방 로그의 원문(가짜 AI가 답한 JSON)은 그대로다. 문장도 가리고 500자에서 자른다.

use chrono::NaiveDate;
use pi_agent::Role;
use price_inha::{
    DAY_ROOM, DemoTurn, KIND_DAY_START, KIND_OFFICE_TALK, OFFICE_TEXT_MAX, TRADE_ROOM_PREFIX,
};
use serde_json::{Value, json};

use crate::common::{
    Bench, DART_KEY, Plan, WATCHED, events, kst, members, saturday, sunday, wait_for,
};

/// 그 방의 `office.talk` 가운데 팀장 글.
fn leader_talks(bench: &Bench, room: &str) -> Vec<Value> {
    events(&bench.store(), KIND_OFFICE_TALK)
        .into_iter()
        .filter(|talk| talk["room"] == room && talk["agent"] == "leader")
        .collect()
}

/// 그 방의 방 로그 팀장 글 원문.
fn leader_raw(bench: &Bench, room: &str) -> Vec<String> {
    members(&bench.store(), room)
        .into_iter()
        .filter(|event| event.actor_id == "leader")
        .filter_map(|event| event.payload["text"].as_str().map(str::to_string))
        .collect()
}

/// 그 날의 하루 시작 이벤트 수.
fn day_starts(bench: &Bench, date: NaiveDate) -> usize {
    let date = json!(date.format("%Y-%m-%d").to_string());
    events(&bench.store(), KIND_DAY_START)
        .into_iter()
        .filter(|payload| payload["date"] == date)
        .count()
}

/// 팀장 판단을 정해 두고 지켜보는 종목의 논의를 한 번 돌린 뒤 그 방 이름.
async fn discuss_with(bench: &Bench, verdict: &str) -> String {
    bench.ready().await;
    bench.demo_ai().script(DemoTurn::Verdict, verdict);
    let discussed = bench
        .crew()
        .discuss(WATCHED, "판단 글 시험")
        .await
        .expect("논의가 돈다");
    let room = format!("{TRADE_ROOM_PREFIX}{}", discussed.trade.0);
    wait_for("팀장 판단 글의 office.talk", || {
        !leader_talks(bench, &room).is_empty()
    })
    .await;
    room
}

/// 팀장의 첫 `office.talk` 글.
fn first_leader_text(bench: &Bench, room: &str) -> String {
    leader_talks(bench, room)[0]["text"]
        .as_str()
        .unwrap_or_default()
        .to_string()
}

// 검증: V14 R5-2 V54
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 하루_판단_운용은_office_talk에_오늘은_운용합니다로_나오고_방_로그는_원문_그대로다() {
    let bench = Plan::demo().start(kst(saturday(), 22, 30)).await;
    wait_for("하루 판단 방의 팀장 office.talk", || {
        !leader_talks(&bench, DAY_ROOM).is_empty()
    })
    .await;
    assert_eq!(
        leader_talks(&bench, DAY_ROOM)[0]["text"],
        "오늘은 운용합니다.",
        "하루 판단 운용은 문장으로 나온다"
    );
    assert_eq!(leader_talks(&bench, DAY_ROOM)[0]["topic"], "day");
    let raw = leader_raw(&bench, DAY_ROOM);
    assert_eq!(raw.len(), 1, "방 로그에 하루 판단 글 하나: {raw:?}");
    assert_eq!(
        serde_json::from_str::<Value>(&raw[0]).ok(),
        Some(json!({"plan": "trade"})),
        "방 로그의 원문(가짜 AI의 JSON)은 그대로다: {}",
        raw[0]
    );
    bench.close().await;
}

// 검증: V14 R5-2 V54
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 하루_판단_쉼은_office_talk에_이유를_붙인_문장으로_나온다() {
    // 장 밖(자정 앞 10분)에 띄워 답을 정한 뒤, 다음 날 장이 열리면 하루 판단을 묻는다.
    let bench = Plan::demo().start(kst(saturday(), 23, 58)).await;
    let rest = json!({"plan": "rest", "reason": "주말 시연이라 쉰다"}).to_string();
    bench.demo_ai().script(DemoTurn::DayPlan, &rest);
    bench.set(kst(sunday(), 0, 6));
    wait_for("일요일 하루 시작", || {
        day_starts(&bench, sunday()) >= 1
    })
    .await;
    wait_for("하루 판단 방의 팀장 office.talk", || {
        !leader_talks(&bench, DAY_ROOM).is_empty()
    })
    .await;
    assert_eq!(
        leader_talks(&bench, DAY_ROOM)[0]["text"],
        "오늘은 쉽니다 — 주말 시연이라 쉰다"
    );
    assert_eq!(
        leader_raw(&bench, DAY_ROOM),
        vec![rest],
        "방 로그의 원문은 그대로다"
    );
    bench.close().await;
}

// 검증: V14 R5-2 V54
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 사기_판단은_office_talk에_진입_손절_목표와_가설의_문장으로_나오고_방_로그는_원문_그대로다()
{
    let bench = Plan::demo().start(kst(saturday(), 14, 0)).await;
    let verdict = json!({
        "decision": "buy", "hypothesis": "외국인 매수가 이어진다", "trigger": "시연 계기",
        "entry": 71_100, "stop": 67_545, "target": 78_210, "expected_value": 3_555
    })
    .to_string();
    let room = discuss_with(&bench, &verdict).await;
    assert_eq!(
        first_leader_text(&bench, &room),
        "사기로 정했습니다 — 진입 71,100 · 손절 67,545 · 목표 78,210. 외국인 매수가 이어진다"
    );
    assert_eq!(leader_talks(&bench, &room)[0]["topic"], WATCHED);
    assert_eq!(
        leader_raw(&bench, &room),
        vec![verdict],
        "방 로그의 원문은 그대로다"
    );
    bench.close().await;
}

// 검증: V14 R5-2 V54
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 지켜보기_판단은_office_talk에_이유를_붙인_문장으로_나온다() {
    let bench = Plan::demo().start(kst(saturday(), 14, 0)).await;
    let verdict =
        json!({"decision": "watch", "reason": "거래량이 적다", "revisit": "거래량이 늘면"})
            .to_string();
    let room = discuss_with(&bench, &verdict).await;
    assert_eq!(
        first_leader_text(&bench, &room),
        "지켜보기로 했습니다 — 거래량이 적다"
    );
    assert_eq!(
        leader_raw(&bench, &room),
        vec![verdict],
        "방 로그의 원문은 그대로다"
    );
    bench.close().await;
}

// 검증: V14 R5-2 E38
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 판단_문장도_가리고_500자에서_자른다() {
    let bench = Plan::demo().start(kst(saturday(), 14, 0)).await;
    let hypothesis = format!(
        "키는 {DART_KEY} 이다 {}",
        "가".repeat(OFFICE_TEXT_MAX + 100)
    );
    let verdict = json!({
        "decision": "buy", "hypothesis": hypothesis, "trigger": "시연 계기",
        "entry": 71_100, "stop": 67_545, "target": 78_210, "expected_value": 3_555
    })
    .to_string();
    let room = discuss_with(&bench, &verdict).await;
    let text = first_leader_text(&bench, &room);
    assert!(
        text.starts_with("사기로 정했습니다 — 진입 71,100 · 손절 67,545 · 목표 78,210. 키는 "),
        "문장 모양이다: {text}"
    );
    assert!(!text.contains(DART_KEY), "가설 글은 가린 것이다: {text}");
    assert!(
        text.chars().count() <= OFFICE_TEXT_MAX,
        "500자에서 자른다: {}자",
        text.chars().count()
    );
    for talk in events(&bench.store(), KIND_OFFICE_TALK) {
        assert!(
            !talk.to_string().contains(DART_KEY),
            "office.talk에 비밀이 없다: {talk}"
        );
    }
    bench.close().await;
}

// 검증: V14 R5-2
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 담당_글은_문장으로_바꾸지_않고_방_로그_글_그대로_나온다() {
    let bench = Plan::demo().start(kst(saturday(), 14, 0)).await;
    let scout = "새 자료를 확인했다. @strategy @risk 의견을 부탁한다";
    bench.demo_ai().script(DemoTurn::Talk(Role::Scout), scout);
    let verdict =
        json!({"decision": "watch", "reason": "거래량이 적다", "revisit": "거래량이 늘면"})
            .to_string();
    let room = discuss_with(&bench, &verdict).await;
    let talks: Vec<Value> = events(&bench.store(), KIND_OFFICE_TALK)
        .into_iter()
        .filter(|talk| talk["room"] == room.as_str() && talk["agent"] != "leader")
        .collect();
    let said: Vec<String> = members(&bench.store(), &room)
        .into_iter()
        .filter(|event| event.actor_id != "leader")
        .filter_map(|event| event.payload["text"].as_str().map(str::to_string))
        .collect();
    assert!(!said.is_empty(), "담당 글이 방 로그에 있다");
    let texts: Vec<String> = talks
        .iter()
        .filter_map(|talk| talk["text"].as_str().map(str::to_string))
        .collect();
    assert_eq!(texts, said, "팀장이 아닌 담당 글은 방 로그 글 그대로다");
    bench.close().await;
}
