//! 결정 29: 멘션이 정해진 그때 멘션된 담당 모두가 곧바로 내 곁으로 온다(답은 지금처럼 차례로 쓴다).

use std::time::Duration;

use pi_agent::Role;
use price_inha::{DemoTurn, KIND_OFFICE_SUMMON, OFFICE_CHAT_PATH, OFFICE_SUMMON_VIEW_PATH};
use serde_json::{Value, json};

use crate::common::{Bench, Plan, has_phase, morning, summons, team_text, wait_for};

/// 가짜 AI 답 하나의 뜸: 첫 담당이 답을 쓰는 동안 다음 담당의 턴은 아직 시작하지 않는다.
const SLOW: Duration = Duration::from_millis(800);

/// 저장된 이벤트 전체(번호 순서)의 종류와 내용.
fn log(bench: &Bench) -> Vec<(String, Value)> {
    bench
        .store()
        .read_after(0, 100_000)
        .unwrap()
        .into_iter()
        .map(|event| (event.kind, event.payload))
        .collect()
}

/// 그 담당이 멘션으로 오는 중이 된 이벤트의 자리(로그 안 순번).
fn called_at(log: &[(String, Value)], agent: &str) -> Option<usize> {
    log.iter().position(|(kind, p)| {
        kind == KIND_OFFICE_SUMMON
            && p["agent"] == agent
            && p["phase"] == "called"
            && p["reason"] == "mention"
    })
}

/// 그 사용자 글에 붙은 그 담당의 응답이 생각 중이 된 이벤트의 자리.
fn thinking_at(log: &[(String, Value)], source: &str, agent: &str) -> Option<usize> {
    log.iter().position(|(kind, p)| {
        kind == pi_office::EVENT_CHAT
            && p["sourceMessageId"] == source
            && p["agent"] == agent
            && p["status"] == "thinking"
    })
}

/// 그 사용자 글에 붙은 그 담당의 응답이 끝났는가.
fn finished(bench: &Bench, source: &str, agent: &str) -> bool {
    log(bench).iter().any(|(kind, p)| {
        kind == pi_office::EVENT_CHAT
            && p["sourceMessageId"] == source
            && p["agent"] == agent
            && (p["status"] == "complete" || p["status"] == "failed")
    })
}

// 검증: V59 C18 C20
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 팀_방_글을_보내면_멘션된_담당_모두가_답_턴보다_먼저_오는_중이_된다() {
    let bench = Plan::demo().pause(SLOW).start(morning()).await;
    bench.script(DemoTurn::Chat(Role::Risk), "리스크입니다.");
    bench.script(DemoTurn::Chat(Role::Strategy), "전략입니다.");
    let reply = bench
        .post(
            OFFICE_CHAT_PATH,
            &team_text("@[리스크] @[전략] 모여요", "g-1"),
        )
        .await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    assert_eq!(reply.body["responders"], json!(["risk", "strategy"]));
    // 글을 받은 응답이 돌아온 때에는 둘 다 이미 오는 중이다(첫 답을 기다리지 않는다).
    let now = log(&bench);
    let risk = called_at(&now, "risk").expect("리스크가 곧바로 오는 중이 된다");
    let strategy = called_at(&now, "strategy").expect("전략도 곧바로 오는 중이 된다");
    for (agent, at) in [("risk", risk), ("strategy", strategy)] {
        if let Some(thinking) = thinking_at(&now, "g-1", agent) {
            assert!(at < thinking, "{agent}는 답 턴보다 먼저 온다");
        }
    }
    // 답은 지금처럼 멘션한 차례로 쓴다.
    wait_for("두 담당의 답이 끝난다", || {
        finished(&bench, "g-1", "risk") && finished(&bench, "g-1", "strategy")
    })
    .await;
    let all = log(&bench);
    assert!(thinking_at(&all, "g-1", "risk") < thinking_at(&all, "g-1", "strategy"));
    for agent in ["risk", "strategy"] {
        let called = summons(&bench.store(), agent)
            .into_iter()
            .filter(|e| e["phase"] == "called")
            .count();
        assert_eq!(
            called, 1,
            "{agent}는 한 번만 불린다(답 턴이 다시 부르지 않는다)"
        );
    }
    for agent in ["news", "review", "trader", "leader"] {
        assert!(
            summons(&bench.store(), agent).is_empty(),
            "멘션하지 않은 담당({agent})은 오지 않는다"
        );
    }
    bench.close().await;
}

// 검증: V59 C18 C20
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 답_속_멘션으로_이어_말할_담당은_그_턴이_시작되기_전에_온다() {
    let bench = Bench::start().await;
    bench.script(DemoTurn::Chat(Role::Risk), "@[전략] 의견 주세요");
    bench.script(DemoTurn::Chat(Role::Strategy), "전략입니다.");
    let reply = bench
        .post(OFFICE_CHAT_PATH, &team_text("@[리스크] 어때요", "g-2"))
        .await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    wait_for("이어 말한 전략의 답이 끝난다", || {
        finished(&bench, "g-2", "strategy")
    })
    .await;
    let all = log(&bench);
    let called = called_at(&all, "strategy").expect("이어 말할 전략도 곁으로 온다");
    let thinking = thinking_at(&all, "g-2", "strategy").expect("전략의 답 턴");
    assert!(
        called < thinking,
        "이어 말하기가 정해진 그때 온다(답 턴이 시작되기 전): called {called} · thinking {thinking}"
    );
    bench.close().await;
}

// 검증: V59 C20
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 팀_방을_떠난_뒤에_시작하는_답_턴은_담당을_다시_부르지_않는다() {
    let bench = Plan::demo().pause(SLOW).start(morning()).await;
    bench.script(DemoTurn::Chat(Role::Risk), "리스크입니다.");
    bench.script(DemoTurn::Chat(Role::Strategy), "전략입니다.");
    let reply = bench
        .post(
            OFFICE_CHAT_PATH,
            &team_text("@[리스크] @[전략] 모여요", "g-3"),
        )
        .await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    // 첫 담당이 답을 쓰는 동안 팀 방을 떠난다: 모였던 담당은 모두 돌아간다.
    let reply = bench
        .post(OFFICE_SUMMON_VIEW_PATH, &json!({"room": "direct"}))
        .await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    wait_for("두 담당의 답이 끝난다", || {
        finished(&bench, "g-3", "risk") && finished(&bench, "g-3", "strategy")
    })
    .await;
    for agent in ["risk", "strategy"] {
        assert!(
            has_phase(&bench.store(), agent, "returning"),
            "{agent}는 돌아간다"
        );
        let last = summons(&bench.store(), agent).pop().unwrap();
        assert_eq!(
            last["phase"], "returning",
            "{agent}: 떠난 뒤 시작한 답 턴이 다시 부르지 않는다 — {last}"
        );
    }
    bench.close().await;
}
