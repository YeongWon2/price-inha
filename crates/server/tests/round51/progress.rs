//! 논의 진행 단계(C21 · E51 · V63 · R5-11).
//!
//! 논의가 도는 동안 `office.progress` `{"room", "topic", "members": [{"agent", "state", "retry"?, "retry_max": 3}],
//! "calls", "started_at", "at"}`를 내고, `GET /api/office`의 `progress` 칸에 지금 도는 논의를 싣는다(없으면 null).
//! 논의가 끝나면 마지막 상태를 한 번 내고 비운다. 담당 턴이 실패하면 3번까지 다시 시도하고 "다시 시도 n/3"을 싣는다.

use std::time::Duration;

use pi_agent::Role;
use price_inha::{DemoTurn, KIND_OFFICE_PROGRESS, OFFICE_PATH, TRADE_ROOM_PREFIX};
use serde_json::Value;

use crate::common::{AGENTS, Bench, Plan, events, morning, settled_prompts};

const STATES: [&str; 4] = ["waiting", "running", "done", "error"];

/// 진행 이벤트에서 그 담당의 칸.
fn state_of<'a>(progress: &'a Value, agent: &str) -> &'a Value {
    progress["members"]
        .as_array()
        .and_then(|list| list.iter().find(|m| m["agent"] == agent))
        .unwrap_or_else(|| panic!("진행에 {agent}가 있어야 한다: {progress}"))
}

/// 진행 이벤트 모양을 본다.
fn check_shape(progress: &Value) {
    let members = progress["members"].as_array().expect("members");
    let agents: Vec<&str> = members
        .iter()
        .map(|m| m["agent"].as_str().unwrap())
        .collect();
    assert_eq!(agents, AGENTS, "담당 여섯, 사무실 순서: {progress}");
    for one in members {
        let state = one["state"].as_str().unwrap_or_default();
        assert!(STATES.contains(&state), "단계는 넷 가운데 하나: {progress}");
        assert_eq!(one["retry_max"], 3, "다시 시도 상한 3: {progress}");
    }
    assert!(
        progress["room"]
            .as_str()
            .is_some_and(|r| r.starts_with(TRADE_ROOM_PREFIX))
    );
    assert!(progress["calls"].is_u64(), "{progress}");
    assert!(progress["started_at"].is_i64(), "{progress}");
    assert!(progress["at"].is_i64(), "{progress}");
}

// 검증: V63 C21 R5-11
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 논의가_도는_동안_진행이_보이고_끝나면_마지막_상태를_한_번_내고_비운다() {
    let bench = Plan::demo()
        .pause(Duration::from_millis(250))
        .start(morning())
        .await;
    settled_prompts(&bench).await;
    let idle = bench.get(OFFICE_PATH).await;
    assert_eq!(idle.status, 200, "{}", idle.text);
    assert!(
        idle.body.get("progress").is_some_and(Value::is_null),
        "도는 논의가 없으면 progress는 null: {}",
        idle.text
    );
    let started_at = bench.now().timestamp_millis();

    let crew = bench.crew();
    let running =
        tokio::spawn(async move { crew.discuss(crate::common::WATCHED, "진행 확인").await });
    let mut seen = Value::Null;
    let deadline = std::time::Instant::now() + crate::common::STEP;
    while seen.is_null() {
        assert!(
            std::time::Instant::now() < deadline,
            "논의 중 progress가 보여야 한다"
        );
        let reply = bench.get(OFFICE_PATH).await;
        seen = reply.body["progress"].clone();
        tokio::time::sleep(Duration::from_millis(30)).await;
    }
    check_shape(&seen);
    assert!(
        seen["topic"]
            .as_str()
            .is_some_and(|t| t.contains("진행 확인")),
        "{seen}"
    );
    assert_eq!(seen["started_at"], started_at, "{seen}");

    // 결과는 보지 않는다: 템플릿을 고르지 않아 판단 뒤 주문 안 넘기기가 실패할 수 있다(이 기능과 무관).
    let _outcome = tokio::time::timeout(crate::common::STEP * 2, running)
        .await
        .expect("논의가 끝나야 한다")
        .expect("작업이 끝나야 한다");
    let deadline = std::time::Instant::now() + crate::common::STEP;
    loop {
        let after = bench.get(OFFICE_PATH).await;
        if after.body.get("progress").is_some_and(Value::is_null) {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "끝나면 비운다: {}",
            after.text
        );
        tokio::time::sleep(Duration::from_millis(30)).await;
    }

    let shown = events(&bench.store(), KIND_OFFICE_PROGRESS);
    assert!(shown.len() >= 3, "여러 번 바뀌어 보인다: {shown:?}");
    for one in &shown {
        check_shape(one);
    }
    let first = &shown[0];
    assert!(
        AGENTS
            .iter()
            .all(|a| state_of(first, a)["state"] == "waiting"),
        "처음은 모두 대기: {first}"
    );
    assert_eq!(first["calls"], 0, "{first}");
    assert!(
        shown
            .iter()
            .any(|p| state_of(p, "news")["state"] == "running"),
        "뉴스 담당이 진행으로 보인 적이 있다"
    );
    let calls: Vec<u64> = shown.iter().map(|p| p["calls"].as_u64().unwrap()).collect();
    assert!(
        calls.windows(2).all(|w| w[0] <= w[1]),
        "호출 수는 늘기만 한다: {calls:?}"
    );
    let last = shown.last().unwrap();
    for agent in ["news", "strategy", "risk", "leader"] {
        assert_eq!(state_of(last, agent)["state"], "done", "{agent}: {last}");
    }
    for agent in ["review", "trader"] {
        assert_eq!(
            state_of(last, agent)["state"],
            "waiting",
            "{agent}는 논의에 나오지 않는다: {last}"
        );
    }
    assert_eq!(last["done"], 4, "끝난 담당 수(담당 n/6의 n): {last}");
    assert!(last["calls"].as_u64().unwrap() >= 5, "{last}");
    bench.close().await;
}

// 검증: E51 V63
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 가짜_ai가_한_번_실패하면_오류_뒤_다시_시도_1_3이_보이고_논의는_이어진다() {
    let bench = Bench::start().await;
    settled_prompts(&bench).await;
    bench.demo_ai().fail_next(DemoTurn::Talk(Role::Strategy), 1);
    // 결과는 보지 않는다(템플릿이 없어 판단 뒤 주문 안 넘기기가 실패할 수 있다). 이어졌는지는 팀장 완료로 본다.
    let _outcome = bench
        .crew()
        .discuss(crate::common::WATCHED, "다시 시도")
        .await;

    let shown = events(&bench.store(), KIND_OFFICE_PROGRESS);
    let errored = shown
        .iter()
        .position(|p| state_of(p, "strategy")["state"] == "error")
        .expect("전략 담당이 오류로 보인 적이 있다");
    let retried = shown
        .iter()
        .position(|p| {
            let one = state_of(p, "strategy");
            one["retry"] == 1 && one["retry_max"] == 3 && one["state"] == "running"
        })
        .expect("다시 시도 1/3으로 진행 중인 것이 보인 적이 있다");
    assert!(errored < retried, "오류 다음에 다시 시도: {shown:?}");
    let last = shown.last().unwrap();
    assert_eq!(state_of(last, "strategy")["state"], "done", "{last}");
    assert_eq!(
        state_of(last, "leader")["state"],
        "done",
        "다시 시도로 이어져 팀장 판단까지 갔다: {last}"
    );
    bench.close().await;
}
