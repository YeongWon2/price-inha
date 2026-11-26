//! 하루 AI 잔량(C22 · M13 · N14 · E52 · V64 · R5-12 · R1-5).
//!
//! - 팀 상한 · 그날 쓴 수(막는 수 그대로) · 팀장 예비분(상한의 10% 올림, 최소 2번) · 모드(남은 호출이 "예비분 +
//!   상한의 20% 올림" 이하면 정리, 예비분만 남으면 팀장 몫만) · 담당별 쓴 호출 수(담당 턴마다 하나)와 몫(바의 분모:
//!   (상한 − 예비분) ÷ 6, 팀장은 거기에 예비분을 더한다) · 사용률 · 색 단계(0.33 · 0.66을 넘으면 바뀐다).
//! - 계산은 서버가 한다. 화면은 받은 값을 그린다.

use chrono::Duration as Days;
use price_inha::{KIND_TEAM_AI_USED, OFFICE_NOTICE_PATH, TEAM_BUDGET_PATH};
use serde_json::Value;

use crate::common::{
    AGENTS, Bench, Plan, events, member, members_total, morning, notice_body, settled_prompts,
    used_of, wait_for,
};

/// 잔량을 읽는다(200이어야 한다).
async fn budget(bench: &Bench) -> Value {
    let reply = bench.get(TEAM_BUDGET_PATH).await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    reply.body
}

/// 소수 둘이 거의 같은가.
fn near(value: &Value, want: f64) -> bool {
    value.as_f64().is_some_and(|v| (v - want).abs() < 1e-6)
}

// 검증: C22 M13 V64 R5-12
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 상한이_없으면_상한_없음과_쓴_수만_주고_담당별로_턴마다_센다() {
    let bench = Bench::start().await;
    settled_prompts(&bench).await;
    let first = budget(&bench).await;
    assert_eq!(first["limit"], Value::Null, "{first}");
    assert_eq!(first["used"], 0, "{first}");
    assert_eq!(first["reserve"], 0, "상한이 없으면 예비분도 없다: {first}");
    assert_eq!(first["mode"], "normal", "{first}");
    let members = first["members"].as_array().expect("담당 목록");
    assert_eq!(members.len(), 6, "담당 여섯: {first}");
    for agent in AGENTS {
        let one = member(&first, agent);
        assert_eq!(one["used"], 0, "{agent}: {first}");
        assert_eq!(
            one["share"],
            Value::Null,
            "상한이 없으면 몫(바의 분모)도 없다: {first}"
        );
    }

    crate::common::discuss(&bench, "잔량 첫 논의").await;
    let after = budget(&bench).await;
    assert_eq!(
        after["used"], 1,
        "논의 하나는 팀 상한에서 하나로 센다(지금처럼): {after}"
    );
    assert!(
        used_of(&after, "news") >= 2,
        "뉴스 담당은 논의 턴과 점수 턴을 썼다: {after}"
    );
    for agent in ["strategy", "risk", "leader"] {
        assert!(
            used_of(&after, agent) >= 1,
            "{agent}도 호출을 썼다: {after}"
        );
    }
    assert_eq!(
        used_of(&after, "review"),
        0,
        "복기 담당은 논의에 나오지 않는다: {after}"
    );
    let progress = events(&bench.store(), price_inha::KIND_OFFICE_PROGRESS);
    let last = progress.last().expect("논의 진행 이벤트가 있어야 한다");
    assert_eq!(
        last["calls"].as_u64(),
        Some(members_total(&after)),
        "담당별 쓴 수의 합은 그 논의의 호출 수와 같다: {last} / {after}"
    );
    let used: Vec<u64> = after["members"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["used"].as_u64().unwrap())
        .collect();
    let mut sorted = used.clone();
    sorted.sort_unstable_by(|a, b| b.cmp(a));
    assert_eq!(
        used, sorted,
        "담당 목록은 많이 쓴 순서로 서버가 정렬한다: {after}"
    );
    bench.close().await;
}

// 검증: N14 M13 E52 R1-5 V64
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 상한_10이면_예비분_2_몫_8을_6으로_나누고_남은_4에서_정리_2에서_팀장만이다() {
    let bench = Plan::demo().ai_limit(10).start(morning()).await;
    settled_prompts(&bench).await;
    let fresh = budget(&bench).await;
    assert_eq!(fresh["limit"], 10, "{fresh}");
    assert_eq!(fresh["reserve"], 2, "상한의 10% 올림, 최소 2번: {fresh}");
    assert_eq!(fresh["mode"], "normal", "{fresh}");
    assert!(near(&member(&fresh, "news")["share"], 8.0 / 6.0), "{fresh}");
    assert!(
        near(&member(&fresh, "leader")["share"], 8.0 / 6.0 + 2.0),
        "팀장 몫은 담당 몫에 예비분을 더한다: {fresh}"
    );

    for round in 1..=5 {
        crate::common::discuss(&bench, &format!("{round}번째")).await;
    }
    let five = budget(&bench).await;
    assert_eq!(five["used"], 5, "{five}");
    assert_eq!(five["mode"], "normal", "남은 5는 문턱(2 + 2) 위다: {five}");
    let news = member(&five, "news");
    assert_eq!(
        news["level"], "high",
        "뉴스 담당은 몫을 넘게 썼다(0.66 넘음): {five}"
    );
    assert!(news["ratio"].as_f64().is_some_and(|r| r > 0.66), "{five}");
    let review = member(&five, "review");
    assert!(near(&review["ratio"], 0.0), "{five}");
    assert_eq!(review["level"], "low", "{five}");

    crate::common::discuss(&bench, "6번째").await;
    let six = budget(&bench).await;
    assert_eq!(six["used"], 6, "{six}");
    assert_eq!(
        six["mode"], "wrap_up",
        "남은 4는 예비분 + 상한의 20% 이하다: {six}"
    );

    for round in 7..=8 {
        crate::common::discuss(&bench, &format!("{round}번째")).await;
    }
    let eight = budget(&bench).await;
    assert_eq!(eight["used"], 8, "{eight}");
    assert_eq!(eight["mode"], "leader_only", "예비분만 남았다: {eight}");

    let shown = events(&bench.store(), KIND_TEAM_AI_USED);
    let last = shown.last().expect("team.ai_used 이벤트가 있어야 한다");
    assert_eq!(last["used"], 8, "{last}");
    assert_eq!(last["limit"], 10, "{last}");
    assert_eq!(last["mode"], "leader_only", "{last}");
    assert_eq!(
        last["members"].as_array().map(Vec::len),
        Some(6),
        "이벤트에도 담당별 사용이 실린다: {last}"
    );
    bench.close().await;
}

// 검증: M13 C22
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 담당별_사용은_다시_켜도_그날_것은_남고_다음_날은_새로_센다() {
    let bench = Bench::start().await;
    settled_prompts(&bench).await;
    crate::common::discuss(&bench, "다시 켜기 전").await;
    let before = budget(&bench).await;
    assert!(members_total(&before) > 0, "{before}");

    let bench = bench.reopen().await;
    settled_prompts(&bench).await;
    let again = budget(&bench).await;
    assert_eq!(
        again["used"], before["used"],
        "그날 쓴 수가 이어진다: {again}"
    );
    for agent in AGENTS {
        assert_eq!(
            used_of(&again, agent),
            used_of(&before, agent),
            "{agent}: 다시 켜도 그날 담당별 사용이 남는다: {again}"
        );
    }

    bench.set(morning() + Days::days(1));
    let next = budget(&bench).await;
    assert_eq!(next["used"], 0, "다음 날은 새로 센다: {next}");
    assert_eq!(
        members_total(&next),
        0,
        "다음 날은 담당별로도 새로 센다: {next}"
    );
    bench.close().await;
}

// 검증: M13 C22 E52
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 공지_확인_답도_그_담당의_호출로_센다() {
    let bench = Bench::start().await;
    settled_prompts(&bench).await;
    let reply = bench
        .post(OFFICE_NOTICE_PATH, &notice_body("오늘은 보수적으로", "b-1"))
        .await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    wait_for(
        "여섯 담당이 확인 답을 하고 잔량에 셈이 닿는다",
        || {
            events(&bench.store(), KIND_TEAM_AI_USED)
                .last()
                .is_some_and(|last| last["used"] == 6)
        },
    )
    .await;
    let now = budget(&bench).await;
    assert_eq!(now["used"], 6, "확인 답 여섯 번: {now}");
    for agent in AGENTS {
        assert_eq!(used_of(&now, agent), 1, "{agent}: 확인 답 한 번: {now}");
    }
    bench.close().await;
}
