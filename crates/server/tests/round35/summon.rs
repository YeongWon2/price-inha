//! 33차 4번: 담당 부르기(C20 · E47 · E48 · N10 · V60). 시간은 넣어 준 시계를 옮겨 잰다(실제로 10초 · 30초를 자지 않는다).

use std::time::Duration;

use pi_agent::Role;
use price_inha::{
    DemoTurn, OFFICE_CHAT_PATH, OFFICE_MEETING_RESERVED, OFFICE_SUMMON_ARRIVED_PATH,
    OFFICE_SUMMON_PATH, OFFICE_SUMMON_TICK, OFFICE_SUMMON_VIEW_PATH, OFFICE_UNKNOWN_AGENT,
};
use serde_json::json;

use crate::common::{
    Bench, Plan, QUIET, WATCHED, events, has_phase, morning, stays_false, summons, wait_for,
};

/// 그 담당의 단계 이벤트 단계들(번호 순서).
fn phases(bench: &Bench, agent: &str) -> Vec<String> {
    summons(&bench.store(), agent)
        .iter()
        .map(|e| e["phase"].as_str().unwrap_or_default().to_owned())
        .collect()
}

// 검증: V60 C20 N10 R5-8
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 부르면_오는_중이고_도착하면_곁에서_기다리다_10초_뒤_돌아가고_돌아오면_자리다() {
    assert!(
        QUIET > OFFICE_SUMMON_TICK * 2,
        "지켜보는 시간이 시계 굴리기 간격보다 길다"
    );
    let bench = Bench::start().await;
    let reply = bench
        .post(OFFICE_SUMMON_PATH, &json!({"agent": "risk"}))
        .await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    assert_eq!(reply.body, json!({"agent": "risk", "phase": "called"}));
    wait_for("오는 중 이벤트", || {
        has_phase(&bench.store(), "risk", "called")
    })
    .await;
    let called = summons(&bench.store(), "risk")[0].clone();
    assert_eq!(called["reason"], "call", "{called}");
    assert_eq!(called["at"].as_i64(), Some(bench.now().timestamp_millis()));

    let reply = bench
        .post(OFFICE_SUMMON_ARRIVED_PATH, &json!({"agent": "risk"}))
        .await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    assert_eq!(reply.body, json!({"agent": "risk", "phase": "waiting"}));
    wait_for("곁에서 기다림 이벤트", || {
        has_phase(&bench.store(), "risk", "waiting")
    })
    .await;

    bench.pass(9);
    stays_false("10초 전에는 돌아가지 않는다", || {
        has_phase(&bench.store(), "risk", "returning")
    })
    .await;
    bench.pass(1);
    wait_for("대화 없이 10초면 돌아가는 중", || {
        has_phase(&bench.store(), "risk", "returning")
    })
    .await;
    let back = summons(&bench.store(), "risk")
        .into_iter()
        .find(|e| e["phase"] == "returning")
        .unwrap();
    assert_eq!(back["reason"], "call", "{back}");

    // 자리로 돌아온 것도 같은 도착 알림으로 알린다(원본 `npc:arrived`).
    let reply = bench
        .post(OFFICE_SUMMON_ARRIVED_PATH, &json!({"agent": "risk"}))
        .await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    assert_eq!(reply.body, json!({"agent": "risk", "phase": "idle"}));
    wait_for("자리 이벤트", || {
        has_phase(&bench.store(), "risk", "idle")
    })
    .await;
    assert_eq!(
        phases(&bench, "risk"),
        vec!["called", "waiting", "returning", "idle"]
    );
    bench.close().await;
}

// 검증: V60 C20 N10
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 곁에서_대화가_있으면_10초를_다시_센다() {
    let bench = Bench::start().await;
    bench.script(DemoTurn::Chat(Role::Risk), "리스크입니다.");
    let reply = bench
        .post(OFFICE_SUMMON_PATH, &json!({"agent": "risk"}))
        .await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    let reply = bench
        .post(OFFICE_SUMMON_ARRIVED_PATH, &json!({"agent": "risk"}))
        .await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    bench.pass(8);
    let body = json!({"room": "direct", "agent": "risk", "text": "지금 어때요?", "sourceMessageId": "c-1"});
    let reply = bench.post(OFFICE_CHAT_PATH, &body).await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    wait_for("답이 끝난다", || {
        events(&bench.store(), pi_office::EVENT_CHAT)
            .iter()
            .any(|e| e["sourceMessageId"] == "c-1" && e["status"] == "complete")
    })
    .await;
    // 도착 뒤 12초지만 마지막 대화 뒤로는 4초다.
    bench.pass(4);
    stays_false("대화 뒤 10초 전에는 돌아가지 않는다", || {
        has_phase(&bench.store(), "risk", "returning")
    })
    .await;
    bench.pass(7);
    wait_for("마지막 대화 뒤 10초면 돌아간다", || {
        has_phase(&bench.store(), "risk", "returning")
    })
    .await;
    bench.close().await;
}

// 검증: V60 C20 N10 E48
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 오는_중에_30초_동안_도착_알림이_없으면_자리로_돌아간다() {
    let bench = Bench::start().await;
    let reply = bench
        .post(OFFICE_SUMMON_PATH, &json!({"agent": "trader"}))
        .await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    bench.pass(29);
    stays_false("30초 전에는 그대로 오는 중", || {
        has_phase(&bench.store(), "trader", "returning")
    })
    .await;
    bench.pass(1);
    wait_for("30초면 자리로 둔다", || {
        has_phase(&bench.store(), "trader", "idle")
    })
    .await;
    assert_eq!(
        phases(&bench, "trader"),
        vec!["called", "returning", "idle"],
        "돌아가는 중을 거쳐 자리"
    );
    bench.close().await;
}

// 검증: V60 C20 E48
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 부름으로_온_담당은_방을_바꿔도_남는다() {
    let bench = Bench::start().await;
    let reply = bench
        .post(OFFICE_SUMMON_PATH, &json!({"agent": "review"}))
        .await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    let reply = bench
        .post(OFFICE_SUMMON_ARRIVED_PATH, &json!({"agent": "review"}))
        .await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    let reply = bench
        .post(OFFICE_SUMMON_VIEW_PATH, &json!({"room": "direct"}))
        .await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    stays_false(
        "부름으로 온 담당은 방을 바꿔도 돌아가지 않는다",
        || has_phase(&bench.store(), "review", "returning"),
    )
    .await;
    // 이미 곁에 있는 담당을 다시 부르면 그대로다(이벤트 없음).
    let reply = bench
        .post(OFFICE_SUMMON_PATH, &json!({"agent": "review"}))
        .await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    assert_eq!(reply.body["phase"], "waiting");
    assert_eq!(phases(&bench, "review"), vec!["called", "waiting"]);
    bench.close().await;
}

// 검증: V60 C20
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 모르는_담당을_부르면_400이고_아무것도_바뀌지_않는다() {
    let bench = Bench::start().await;
    for body in [json!({"agent": "nobody"}), json!({}), json!({"agent": 7})] {
        for path in [OFFICE_SUMMON_PATH, OFFICE_SUMMON_ARRIVED_PATH] {
            let reply = bench.post(path, &body).await;
            assert_eq!(reply.status, 400, "{path} {body}: {}", reply.text);
            assert_eq!(reply.body["error"], OFFICE_UNKNOWN_AGENT, "{path} {body}");
        }
    }
    assert!(events(&bench.store(), price_inha::KIND_OFFICE_SUMMON).is_empty());
    bench.close().await;
}

// 검증: V60 C20 E47 R5-8
#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
async fn 논의_중인_담당을_부르면_409이고_걸어오지_않는다() {
    let bench = Plan::demo()
        .pause(Duration::from_millis(700))
        .start(morning())
        .await;
    bench.ready().await;
    let crew = bench.crew();
    let discussion = tokio::spawn(async move {
        let _ = crew.discuss(WATCHED, "부르기 시험").await;
    });
    let store = bench.store();
    // 지금 미팅룸에 있는 담당: 그 담당의 마지막 모습 이벤트가 미팅룸인 담당.
    let in_meeting = || {
        let seen = events(&store, pi_office::EVENT_AGENT);
        pi_office::Agent::ALL
            .iter()
            .map(|a| a.id())
            .find(|id| {
                seen.iter()
                    .rev()
                    .find(|e| e["agent"] == *id)
                    .is_some_and(|e| e["place"] == "meeting")
            })
            .map(str::to_owned)
    };
    wait_for("논의에 든 담당이 미팅룸에 간다", || {
        in_meeting().is_some()
    })
    .await;
    let agent = in_meeting().unwrap();
    let reply = bench
        .post(OFFICE_SUMMON_PATH, &json!({"agent": agent}))
        .await;
    assert_eq!(reply.status, 409, "{}", reply.text);
    assert_eq!(reply.body["error"], OFFICE_MEETING_RESERVED);
    assert!(
        summons(&bench.store(), &agent).is_empty(),
        "회의 중인 담당은 걸어오지 않는다"
    );
    tokio::time::timeout(Duration::from_secs(60), discussion)
        .await
        .expect("논의가 끝난다")
        .unwrap();
    bench.close().await;
}
