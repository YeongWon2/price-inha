//! 2번 · 3번: 시연 조립의 가짜 공시 간격을 줄이면(조립 값) 지켜보는 종목의 "시연" 공시가 와서 종목 논의가 열리고,
//! 그 논의 방의 담당 글마다 `office.talk`(topic = 종목 코드)가 나온다. 시계는 시험이 1초씩 흘린다.

use std::time::{Duration, Instant};

use price_inha::{DEMO_DISCLOSURE_MARK, KIND_OFFICE_TALK, TRADE_ROOM_PREFIX};
use serde_json::Value;

use crate::common::{Bench, Plan, STEP, WATCHED, events, kst, plus, room, saturday};

/// 지켜보는 종목의 종목 논의 방에서 나온 `office.talk`들.
fn trade_talks(bench: &Bench) -> Vec<Value> {
    events(&bench.store(), KIND_OFFICE_TALK)
        .into_iter()
        .filter(|talk| {
            talk["topic"] == WATCHED
                && talk["room"]
                    .as_str()
                    .is_some_and(|name| name.starts_with(TRADE_ROOM_PREFIX))
        })
        .collect()
}

/// 시계를 1초씩 흘리며 종목 논의의 `office.talk`가 나올 때까지 기다린다(가장 길게 [`STEP`]).
pub async fn until_trade_talk(bench: &Bench) -> Vec<Value> {
    let deadline = Instant::now() + STEP;
    loop {
        let talks = trade_talks(bench);
        if !talks.is_empty() {
            return talks;
        }
        assert!(
            Instant::now() < deadline,
            "가짜 공시로 종목 논의가 열리고 office.talk가 나와야 한다"
        );
        bench.set(plus(bench.now(), 1));
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

// 검증: V54 R2-3 D14
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 가짜_공시_간격을_줄이면_종목_논의가_열리고_담당_글_office_talk가_나온다() {
    let bench = Plan::demo()
        .every(Duration::from_secs(1))
        .start(kst(saturday(), 14, 0))
        .await;
    bench.ready().await;

    let talks = until_trade_talk(&bench).await;
    let room_name = talks[0]["room"].as_str().unwrap().to_string();
    for talk in &talks {
        assert!(
            talk["agent"]
                .as_str()
                .is_some_and(|agent| !agent.is_empty()),
            "말한 담당이 있다: {talk}"
        );
        assert!(
            talk["text"].as_str().is_some_and(|text| !text.is_empty()),
            "담당 글이 있다: {talk}"
        );
    }
    let log = room(&bench.store(), &room_name);
    assert!(
        log.iter().any(|event| event.kind == "message.member"),
        "종목 논의 방에 담당 글이 쌓였다: {room_name}"
    );
    let prompts = bench.prompts();
    assert!(
        prompts
            .iter()
            .any(|prompt| prompt.contains(DEMO_DISCLOSURE_MARK) && prompt.contains(WATCHED)),
        "논의는 지켜보는 종목의 '시연' 가짜 공시로 열렸다(담당이 받은 글에 보인다)"
    );
    bench.close().await;
}
