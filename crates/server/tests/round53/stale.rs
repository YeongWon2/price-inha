//! 오래된 "일하는 중"(E55 · N15 · V67 · R5-15): 담당이 생각 중인 채로 300초 동안 새 소식이 없으면 서버의 사무실 시계
//! 굴리기(39차, 250ms마다)가 한가로 풀어 `office.agent` 이벤트를 낸다. 화면은 그 이벤트로 표시를 지운다.
//! 가짜 AI 답에 뜸을 두어 답이 오기 전에 시계를 300초 옮긴다(실제로 300초를 자지 않는다).

use std::time::Duration;

use price_inha::OFFICE_CHAT_PATH;
use serde_json::{Value, json};

use crate::common::{Bench, events, stays_false, wait_for};

/// 가짜 AI 답의 뜸(실제로 잔다): 그 사이에 시계를 옮겨 풀기를 본다.
const PAUSE: Duration = Duration::from_secs(4);

/// 그 담당의 모습 이벤트들(번호, 일).
fn works(bench: &Bench, agent: &str) -> Vec<(u64, String)> {
    events(&bench.store(), pi_office::EVENT_AGENT)
        .into_iter()
        .filter(|(_, p)| p["agent"] == agent)
        .map(|(seq, p)| (seq, p["work"].as_str().unwrap_or_default().to_owned()))
        .collect()
}

/// 그 담당의 답이 끝났는가(`office.chat` complete).
fn answered(bench: &Bench, agent: &str) -> bool {
    events(&bench.store(), pi_office::EVENT_CHAT)
        .into_iter()
        .any(|(_, p): (u64, Value)| p["agent"] == agent && p["status"] == "complete")
}

/// 생각 중 이벤트 뒤에 한가 이벤트가 있는가.
fn released_after_thinking(bench: &Bench, agent: &str) -> bool {
    let seen = works(bench, agent);
    let Some(thinking) = seen.iter().position(|(_, w)| w == "thinking") else {
        return false;
    };
    seen[thinking + 1..].iter().any(|(_, w)| w == "idle")
}

// 검증: V67 E55 N15 R5-15
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 생각_중인_채로_300초가_지나면_서버가_한가로_풀어_이벤트를_낸다() {
    let bench = Bench::start(PAUSE).await;
    let body = json!({"room": "team", "text": "@[리스크] 잠깐요", "sourceMessageId": "s-53"});
    let reply = bench.post(OFFICE_CHAT_PATH, &body).await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    wait_for("리스크가 생각 중이 된다", || {
        works(&bench, "risk").iter().any(|(_, w)| w == "thinking")
    })
    .await;
    // 299초까지는 풀지 않는다.
    bench.pass(299);
    stays_false("300초 전에는 풀지 않는다", || {
        released_after_thinking(&bench, "risk")
    })
    .await;
    bench.pass(2);
    wait_for("300초가 지나면 한가로 풀린다", || {
        released_after_thinking(&bench, "risk")
    })
    .await;
    assert!(
        !answered(&bench, "risk"),
        "답이 오기 전에 시계 굴리기가 풀었다(답이 끝나서 풀린 것이 아니다)"
    );
    bench.close().await;
}
