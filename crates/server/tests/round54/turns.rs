//! 셈 단위(기능 F 1번 · N14 · C22 · R5-12): 팀의 "쓴 수"는 실제 AI 턴 수다.
//!
//! - 논의 안의 담당 턴 · 다시 시도 · 점수 턴 · 팀장 판단이 하나씩이다. 그래서 팀의 쓴 수 = 담당별 사용의 합 =
//!   논의 진행 이벤트의 호출 수(`calls`)다.
//! - 논의 밖의 턴(사무실 1:1 채팅 답)도 그 담당에 하나, 팀에 하나다.
//! - 다시 켜도 그날 것이 남는다(지금 방식 — round51이 본다).

use price_inha::{KIND_OFFICE_PROGRESS, OFFICE_CHAT_PATH};
use serde_json::json;

use crate::common::{
    Bench, budget, discuss, events, members_total, settled_prompts, used_of, wait_for,
};

// 검증: N14 C22 M13 V64
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 상한이_없어도_팀의_쓴_수는_담당별_사용의_합이고_논의의_호출_수와_같다() {
    let bench = Bench::start().await;
    settled_prompts(&bench).await;
    let before = budget(&bench).await;
    discuss(&bench, "턴 단위 첫 논의").await;
    let after = budget(&bench).await;
    let used = after["used"].as_u64().expect("쓴 수");
    let progress = events(&bench.store(), KIND_OFFICE_PROGRESS);
    let calls = progress
        .last()
        .and_then(|last| last["calls"].as_u64())
        .expect("논의 진행 이벤트의 호출 수");
    assert!(calls >= 3, "시연 논의는 턴을 여럿 쓴다: {calls}");
    assert_eq!(
        used - before["used"].as_u64().unwrap_or(0),
        calls,
        "논의 하나는 그 논의가 부른 AI 턴 수만큼 센다: {after}"
    );
    assert_eq!(
        used,
        members_total(&after),
        "팀의 쓴 수는 담당별 사용의 합이다: {after}"
    );
    bench.close().await;
}

// 검증: N14 C22 M13
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 사무실_일대일_채팅_답도_그_담당과_팀의_호출로_하나씩_센다() {
    let bench = Bench::start().await;
    settled_prompts(&bench).await;
    let body = json!({"room": "direct", "agent": "risk", "text": "오늘 위험은?", "sourceMessageId": "f-1"});
    let reply = bench.post(OFFICE_CHAT_PATH, &body).await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    wait_for("채팅 답이 잔량에 닿는다", || {
        events(&bench.store(), price_inha::KIND_TEAM_AI_USED)
            .last()
            .is_some_and(|e| e["used"] == 1)
    })
    .await;
    let last = budget(&bench).await;
    assert_eq!(last["used"], 1, "채팅 답 하나는 팀의 호출 하나: {last}");
    assert_eq!(used_of(&last, "risk"), 1, "그 담당의 호출 하나: {last}");
    assert_eq!(members_total(&last), 1, "{last}");
    bench.close().await;
}
