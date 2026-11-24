//! 1번: 논의 방(`trade-<번호>` · `day`)에 담당 글(`message.member`)이 쌓이면 1초 안에 사무실 이벤트
//! [`KIND_OFFICE_TALK`]가 하나씩 나오고, 내용이 방 로그 글과 같다(가린 글). 점수 턴은 나오지 않는다.
//! (25차 2번으로 고침) 팀장의 판단 글만 `text`가 사람이 읽는 문장이다([`leader_sentence`]).

use std::time::{Duration, Instant};

use pi_agent::Role;
use pi_market::Market;
use pi_store::RoomEvent;
use price_inha::{
    DAY_ROOM, DemoTurn, KIND_DAY_DISCUSSION, KIND_OFFICE_TALK, OFFICE_TEXT_MAX, TRADE_ROOM_PREFIX,
    leader_sentence,
};
use serde_json::{Value, json};

use crate::common::{Bench, DART_KEY, Plan, WATCHED, events, kst, monday, on_date, room, wait_for};

/// 담당 글이 방 로그에 남은 뒤 `office.talk`가 나와야 하는 시간.
const WITHIN: Duration = Duration::from_secs(1);

/// 점수 턴의 까닭에만 쓰는 글(이 글이 `office.talk`에 보이면 점수 턴이 나온 것이다).
const SCORE_ONLY: &str = "점수턴에만있는글";

/// 방의 담당 글들.
fn members(events: &[RoomEvent]) -> Vec<&RoomEvent> {
    events
        .iter()
        .filter(|event| event.kind == "message.member")
        .collect()
}

/// 그 방의 `office.talk` 내용들(번호 순서).
fn talks_in(bench: &Bench, room_name: &str) -> Vec<Value> {
    events(&bench.store(), KIND_OFFICE_TALK)
        .into_iter()
        .filter(|talk| talk["room"] == room_name)
        .collect()
}

/// `office.talk`가 그 수만큼 나올 때까지 [`WITHIN`]만 기다린다.
async fn talks_within(bench: &Bench, room_name: &str, count: usize) -> Vec<Value> {
    let deadline = Instant::now() + WITHIN;
    loop {
        let talks = talks_in(bench, room_name);
        if talks.len() >= count || Instant::now() >= deadline {
            return talks;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// 방 로그 글 하나와 같은 `office.talk` 내용. (25차 2번으로 고침) 팀장 판단 글의 `text`만 사람이 읽는 문장
/// ([`leader_sentence`], 판단 모양이 아니면 원문)이고 나머지는 방 로그 글 그대로다.
fn expected(event: &RoomEvent, topic: &str) -> Value {
    let raw = event.payload["text"].as_str().unwrap_or_default();
    let text = if event.actor_id == "leader" {
        leader_sentence(raw, Market::Kr).unwrap_or_else(|| raw.to_string())
    } else {
        raw.to_string()
    };
    json!({
        "room": event.room,
        "agent": event.actor_id,
        "text": text,
        "at": event.at.0,
        "topic": topic,
    })
}

/// `office.talk` 내용에서 다섯 칸만 고른다(다른 칸이 더 있어도 이 다섯 칸이 같아야 한다).
fn five(talk: &Value) -> Value {
    json!({
        "room": talk["room"],
        "agent": talk["agent"],
        "text": talk["text"],
        "at": talk["at"],
        "topic": talk["topic"],
    })
}

// 검증: V14 V13 E38 N8 E3
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 종목_논의의_담당_글마다_office_talk가_1초_안에_나오고_방_로그_글과_같다() {
    let bench = Plan::demo().start(kst(monday(), 10, 0)).await;
    bench.ready().await;
    let leak = format!("공시를 봤다 키는 {DART_KEY} 이다 @strategy @risk");
    bench.script(DemoTurn::Talk(Role::Scout), &leak);
    bench.script(
        DemoTurn::Score(Role::Risk),
        &json!({"score": 70, "reason": SCORE_ONLY}).to_string(),
    );

    let discussed = bench
        .crew()
        .discuss(WATCHED, "말풍선 시험")
        .await
        .expect("논의가 돈다");
    let room_name = format!("{TRADE_ROOM_PREFIX}{}", discussed.trade.0);
    let log = room(&bench.store(), &room_name);
    let said = members(&log);
    assert!(
        said.len() >= 3,
        "담당 셋 이상(뉴스 공시 시세 · 넘겨받은 담당 · 팀장)이 말했다: {}",
        said.len()
    );
    assert!(
        said.iter().any(|e| e.actor_id == "leader"),
        "팀장 판단 글도 방 로그에 있다"
    );

    let talks = talks_within(&bench, &room_name, said.len()).await;
    assert_eq!(
        talks.len(),
        said.len(),
        "담당 글 하나마다 office.talk 하나가 1초 안에 나온다: {talks:?}"
    );
    for (talk, event) in talks.iter().zip(&said) {
        assert_eq!(
            five(talk),
            expected(event, WATCHED),
            "내용이 방 로그 글과 같다(방 · 쓴 쪽 · 가린 글 · 시각 · 종목)"
        );
    }
    let leader = talks
        .iter()
        .find(|talk| talk["agent"] == "leader")
        .expect("팀장 판단 글도 같은 길로 나온다");
    assert_eq!(leader["topic"], WATCHED);
    let sentence = leader["text"].as_str().unwrap_or_default();
    assert!(
        !sentence.trim_start().starts_with('{'),
        "(25차 2번) 팀장 판단 글은 날 JSON이 아니라 문장이다: {sentence}"
    );

    let scout = talks
        .iter()
        .find(|talk| talk["agent"] == "scout")
        .expect("뉴스 공시 시세 담당의 글");
    let text = scout["text"].as_str().unwrap_or_default();
    assert!(text.contains("공시를 봤다"), "가린 글은 남는다: {text}");
    for talk in events(&bench.store(), KIND_OFFICE_TALK) {
        let whole = talk.to_string();
        assert!(!whole.contains(DART_KEY), "office.talk에 비밀이 없다");
        assert!(
            !whole.contains(SCORE_ONLY),
            "점수 턴은 office.talk로 나오지 않는다: {whole}"
        );
    }
    bench.close().await;
}

// 검증: V14 E38
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 긴_담당_글은_500자에서_잘려_나온다() {
    let bench = Plan::demo().start(kst(monday(), 10, 0)).await;
    bench.ready().await;
    let long = format!("{} @strategy @risk", "가".repeat(OFFICE_TEXT_MAX + 200));
    bench.script(DemoTurn::Talk(Role::Scout), &long);

    let discussed = bench
        .crew()
        .discuss(WATCHED, "긴 글")
        .await
        .expect("논의가 돈다");
    let room_name = format!("{TRADE_ROOM_PREFIX}{}", discussed.trade.0);
    let log = room(&bench.store(), &room_name);
    let said = members(&log);
    let original = said
        .iter()
        .find(|e| e.actor_id == "scout")
        .and_then(|e| e.payload["text"].as_str())
        .expect("방 로그에 긴 글이 있다")
        .to_string();
    assert!(original.chars().count() > OFFICE_TEXT_MAX);

    let talks = talks_within(&bench, &room_name, said.len()).await;
    let scout = talks
        .iter()
        .find(|talk| talk["agent"] == "scout")
        .expect("긴 글도 office.talk로 나온다");
    let text = scout["text"].as_str().unwrap_or_default();
    assert!(
        text.chars().count() <= OFFICE_TEXT_MAX,
        "500자에서 자른다: {}자",
        text.chars().count()
    );
    let head: String = original.chars().take(OFFICE_TEXT_MAX - 20).collect();
    assert!(text.starts_with(&head), "앞부분은 방 로그 글 그대로다");
    bench.close().await;
}

// 검증: V14 V5 E3
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 하루_판단_방의_팀장_글도_office_talk로_나오고_주제는_day다() {
    let bench = Plan::demo().start(kst(monday(), 8, 50)).await;
    bench.set(kst(monday(), 9, 1));
    let store = bench.store();
    wait_for(
        "장 시작에 하루 운용 판단 논의가 남는다",
        || on_date(&store, KIND_DAY_DISCUSSION, monday()) > 0,
    )
    .await;

    let log = room(&store, DAY_ROOM);
    let said = members(&log);
    assert!(!said.is_empty(), "하루 판단 글이 방 로그에 있다");
    let talks = talks_within(&bench, DAY_ROOM, said.len()).await;
    assert_eq!(
        talks.len(),
        said.len(),
        "하루 판단 방의 담당 글마다 office.talk가 나온다: {talks:?}"
    );
    for (talk, event) in talks.iter().zip(&said) {
        assert_eq!(five(talk), expected(event, "day"));
    }
    let leader = talks
        .iter()
        .find(|talk| talk["agent"] == "leader")
        .expect("팀장의 하루 판단 글");
    assert_eq!(
        leader["text"], "오늘은 운용합니다.",
        "(25차 2번) 하루 판단 운용은 문장으로 나온다"
    );
    bench.close().await;
}
