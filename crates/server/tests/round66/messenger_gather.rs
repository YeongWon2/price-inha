//! 모이기(결정 29) — 메신저 팀 방 글: 메신저(가짜 텔레그램)로 담당 둘을 멘션해 보내면 멘션된 담당 모두가 곧바로(답 턴보다
//! 먼저) 오는 중이 된다. 멘션하지 않은 담당은 오지 않는다.

use std::time::{Duration, Instant};

use price_inha::{KIND_OFFICE_MESSAGE, KIND_OFFICE_SUMMON, MESSENGER_VIA};
use serde_json::Value;

use crate::common::STEP;
use crate::messenger_bench::{FakeTelegram, MessengerBench};

/// 연결된 사람.
const OWNER: i64 = 87_660_001;

fn log(bench: &MessengerBench) -> Vec<(String, Value)> {
    bench
        .store()
        .read_after(0, 100_000)
        .unwrap()
        .into_iter()
        .map(|event| (event.kind, event.payload))
        .collect()
}

/// 그 담당이 멘션으로 오는 중이 된 이벤트의 자리.
fn called_at(log: &[(String, Value)], agent: &str) -> Option<usize> {
    log.iter().position(|(kind, p)| {
        kind == KIND_OFFICE_SUMMON
            && p["agent"] == agent
            && p["phase"] == "called"
            && p["reason"] == "mention"
    })
}

/// 그 담당의 답 턴이 시작된(생각 중) 이벤트의 자리.
fn thinking_at(log: &[(String, Value)], agent: &str) -> Option<usize> {
    log.iter().position(|(kind, p)| {
        kind == pi_office::EVENT_CHAT && p["agent"] == agent && p["status"] == "thinking"
    })
}

/// 메신저에서 온 팀 방 글 이벤트의 자리.
fn posted_at(log: &[(String, Value)]) -> Option<usize> {
    log.iter().position(|(kind, p)| {
        kind == KIND_OFFICE_MESSAGE && p["room"] == "team" && p["via"] == MESSENGER_VIA
    })
}

// 검증: V59 C18 C20 V58 (메신저 팀 방 멘션 글: 멘션된 담당 모두가 답 턴보다 먼저 오는 중이 된다)
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn 메신저로_둘을_멘션하면_둘_다_답_턴보다_먼저_오는_중이_된다() {
    let tg = FakeTelegram::start().await;
    let bench = MessengerBench::linked(&tg, OWNER).await;
    tg.push_dm(OWNER, "@리스크 @전략 모여 주세요");

    let deadline = Instant::now() + STEP;
    let now = loop {
        let now = log(&bench);
        if called_at(&now, "risk").is_some() && called_at(&now, "strategy").is_some() {
            break now;
        }
        assert!(
            Instant::now() < deadline,
            "둘 다 오는 중이 되어야 한다: {now:?}"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    };
    let posted = posted_at(&now).expect("메신저에서 온 팀 방 글로 남는다");
    for agent in ["risk", "strategy"] {
        let called = called_at(&now, agent).unwrap();
        assert!(posted < called, "{agent}: 글을 받은 뒤에 부른다");
        if let Some(thinking) = thinking_at(&now, agent) {
            assert!(called < thinking, "{agent}는 답 턴보다 먼저 온다");
        }
    }
    // 첫 담당의 답 턴이 시작되기 전에 둘째 담당도 이미 오는 중이다(차례로 부르지 않는다).
    if let Some(first) = thinking_at(&now, "risk") {
        assert!(
            called_at(&now, "strategy").unwrap() < first,
            "둘째도 첫 답 턴 전에 온다"
        );
    }
    for agent in ["news", "review", "trader", "leader"] {
        assert!(
            called_at(&log(&bench), agent).is_none(),
            "멘션하지 않은 담당({agent})은 오지 않는다"
        );
    }
    bench.close().await;
}
