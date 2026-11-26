//! 논의에 예산 넘기기(기능 F 2번 · N14 · E52 · V53 · R1-5): 논의를 열 때 남은 호출 · 팀장 예비분 · 정리 문턱을
//! agent에 넘긴다.
//!
//! - 남은 호출이 정리 문턱 이하가 되면 새 라운드 없이 점수 · 팀장 판단으로 넘어간다(방 로그 `wrap_up`).
//! - 예비분만 남으면 담당 턴 없이 팀장만 부른다. 예비분도 없으면 논의를 열지 않는다(상한 오류 · 알림 그대로).
//! - 쓴 수는 상한을 넘지 않고, 잔량의 모드(서버가 내려 준 `mode`)는 쓴 수로 본 N14 판정과 같다.

use price_inha::{DemoTurn, KIND_TEAM_AI_USED, TeamError};
use serde_json::{Value, json};

use crate::common::{
    Plan, WATCH_VERDICT, budget, discuss_result, events, members_total, morning, room_payloads,
    settled_prompts, thresholds, used_of,
};

/// 쓴 수로 본 N14 모드.
fn mode_of(limit: u32, used: u64) -> &'static str {
    let (reserve, wrap_up_at) = thresholds(limit);
    let left = u64::from(limit).saturating_sub(used);
    if left <= u64::from(reserve) {
        "leader_only"
    } else if left <= u64::from(wrap_up_at) {
        "wrap_up"
    } else {
        "normal"
    }
}

// 검증: N14 E52 V53 R1-5
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 상한_10이면_논의가_정리_모드로_넘어가고_예비분을_남긴_채_팀장이_판단한다() {
    let bench = Plan::demo().ai_limit(10).start(morning()).await;
    let before = settled_prompts(&bench).await;
    bench.demo_ai().script(DemoTurn::Verdict, WATCH_VERDICT);
    // 정리 모드는 새 라운드를 열기 전에 남은 호출을 본다(N14): 상한 10의 첫 논의는 문턱(4)보다 많이 남긴 채
    // 끝나므로, 문턱에 닿는 둘째 논의에서 정리 모드가 든다.
    discuss_result(&bench, "정리 모드 시험 첫째")
        .await
        .expect("남은 호출이 있으면 논의가 열린다");
    let done = discuss_result(&bench, "정리 모드 시험 둘째")
        .await
        .expect("남은 호출이 있으면 논의가 열린다");
    let spent = bench.prompts().len() - before;
    let rooms = room_payloads(&bench.store(), done.trade);
    assert!(
        rooms.iter().any(|p| p["status"] == "wrap_up"),
        "남은 호출이 문턱(4) 이하가 되면 정리 모드로 넘어간다: {rooms:?}"
    );
    let now = budget(&bench).await;
    let used = now["used"].as_u64().expect("쓴 수");
    assert_eq!(used, spent as u64, "가짜 AI가 받은 턴 수만큼 센다: {now}");
    assert!(used <= 10, "상한을 넘지 않는다: {now}");
    assert!(
        used_of(&now, "leader") >= 1,
        "팀장 판단은 예비분에서 나간다: {now}"
    );
    assert_eq!(used, members_total(&now), "{now}");
    bench.close().await;
}

// 검증: N14 E52 V53 R1-5 E30
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 예비분만_남으면_팀장만_부르고_예비분도_없으면_논의를_열지_않는다() {
    // 상한 2 = 예비분 2: 처음부터 팀장 몫만 남았다.
    let bench = Plan::demo().ai_limit(2).start(morning()).await;
    let before = settled_prompts(&bench).await;
    bench.demo_ai().script(DemoTurn::Verdict, WATCH_VERDICT);
    discuss_result(&bench, "팀장만 첫째")
        .await
        .expect("예비분이 남으면 논의는 열린다");
    let prompts = bench.prompts();
    let asked: Vec<&String> = prompts[before..].iter().collect();
    assert_eq!(
        asked.len(),
        1,
        "담당 턴 없이 팀장 판단 하나만 부른다: {asked:?}"
    );
    assert!(
        asked[0].contains("판단을 JSON 객체 하나로"),
        "그 하나는 팀장 판단 턴이다: {asked:?}"
    );
    let one = budget(&bench).await;
    assert_eq!(one["used"], 1, "{one}");
    assert_eq!(used_of(&one, "leader"), 1, "{one}");

    discuss_result(&bench, "팀장만 둘째")
        .await
        .expect("예비분 하나가 남았다");
    assert_eq!(bench.prompts().len(), before + 2, "둘째도 팀장 하나만");
    let third = discuss_result(&bench, "셋째").await;
    assert!(
        matches!(third, Err(TeamError::AiLimit)),
        "예비분도 없으면 논의를 열지 않는다(상한 오류): {:?}",
        third.as_ref().map(|d| d.trade)
    );
    assert_eq!(bench.prompts().len(), before + 2, "AI를 부르지 않는다");
    let last = budget(&bench).await;
    assert_eq!(last["used"], 2, "상한을 넘지 않는다: {last}");
    assert_eq!(last["mode"], "leader_only", "{last}");
    bench.close().await;
}

// 검증: N14 E52 C22 V64
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 상한까지_논의를_거듭해도_쓴_수는_상한을_넘지_않고_잔량의_모드는_n14_판정과_같다() {
    let limit = 12;
    let bench = Plan::demo().ai_limit(limit).start(morning()).await;
    let before = settled_prompts(&bench).await;
    bench.demo_ai().script(DemoTurn::Verdict, WATCH_VERDICT);
    let mut opened = 0;
    for round in 0..limit {
        match discuss_result(&bench, &format!("{round}번째")).await {
            Ok(_) => opened += 1,
            Err(TeamError::AiLimit) => break,
            Err(other) => panic!("상한 말고는 실패하지 않는다: {other}"),
        }
    }
    assert!(opened >= 2, "논의 여럿이 열렸다: {opened}");
    assert!(opened < limit, "논의 하나가 하나로 세이지 않는다: {opened}");
    let now = budget(&bench).await;
    let used = now["used"].as_u64().expect("쓴 수");
    assert_eq!(used, u64::from(limit), "상한을 다 쓰고 넘지 않는다: {now}");
    assert_eq!(
        bench.prompts().len() - before,
        limit as usize,
        "가짜 AI가 받은 턴 수 = 상한"
    );
    let shown: Vec<Value> = events(&bench.store(), KIND_TEAM_AI_USED);
    assert!(!shown.is_empty());
    for event in &shown {
        let used = event["used"].as_u64().unwrap_or_default();
        assert_eq!(
            event["mode"],
            json!(mode_of(limit, used)),
            "이벤트의 모드는 쓴 수로 본 판정과 같다: {event}"
        );
    }
    assert_eq!(now["mode"], "leader_only", "{now}");
    bench.close().await;
}
