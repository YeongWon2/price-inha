//! 41차 "발판 먼저": 새 시험 발판이 **지금 구현에서** 서는지 본다 — 팀을 이어 운용 중으로 남음 · 논의를 돌리면
//! `office.talk`가 나옴 · 사무실 팀 방 멘션에 그 담당이 답함 · 사무실 공지에 여섯 담당이 확인 답 · 지금 알림이 메신저와
//! 폰 푸시로 감 · 최대 손실 전체 정지가 됨 · 상한을 정해 다시 켜도 페어링이 남음. 이 파일의 시험은 구현 전에도 통과하는
//! 것이 정상이다(나머지 41차 시험이 이 발판 위에서 red인 까닭이 발판이 아니라 빠진 기능임을 보인다).

use std::time::Duration;

use pi_store::{TeamCause, TeamState};
use price_inha::{KIND_OFFICE_TALK, OFFICE_CHAT_PATH, OFFICE_NOTICE_PATH};
use serde_json::{Value, json};

use crate::common::{
    Bench, FakeTelegram, LEADER_WATCH, RISK_TALK, STRATEGY_TALK, URGENT, is_approval_alert, post,
    wait_for,
};

/// 연결된 사람.
const OWNER: i64 = 87_000_001;

/// 그 담당의 `office.talk` 글들.
fn talks_of(bench: &Bench, agent: &str) -> Vec<String> {
    bench
        .events(KIND_OFFICE_TALK)
        .into_iter()
        .filter(|e| e["agent"] == agent)
        .filter_map(|e| e["text"].as_str().map(str::to_string))
        .collect()
}

/// 그 글 번호에 붙은 그 담당의 끝난 답(`office.chat` `complete`).
fn complete_reply(bench: &Bench, source: &str, agent: &str) -> Option<Value> {
    bench.events(pi_office::EVENT_CHAT).into_iter().find(|e| {
        e["sourceMessageId"] == source && e["agent"] == agent && e["status"] == "complete"
    })
}

// 검증: V89 F4 M1
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 발판은_팀을_잇고_운용_중이_되고_그대로_남는다() {
    let tg = FakeTelegram::start().await;
    let bench = Bench::linked(&tg, OWNER).await;
    bench.ready().await;
    let _crew = bench.crew();
    let store = bench.store();
    assert_eq!(store.team_state().unwrap(), TeamState::Operating);
    tokio::time::sleep(Duration::from_secs(2)).await;
    assert_eq!(
        store.team_state().unwrap(),
        TeamState::Operating,
        "시세가 흐르므로 이상 감시가 팀을 멈추지 않는다"
    );
    bench.close().await;
}

// 검증: V58 V89
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 발판에서_논의를_돌리면_office_talk가_나온다() {
    let tg = FakeTelegram::start().await;
    let bench = Bench::linked(&tg, OWNER).await;
    bench.ready().await;
    bench.discuss("발판 논의 시험").await;
    wait_for("strategy · risk · 팀장의 office.talk", || {
        talks_of(&bench, "strategy")
            .iter()
            .any(|t| t == STRATEGY_TALK)
            && talks_of(&bench, "risk").iter().any(|t| t == RISK_TALK)
            && talks_of(&bench, "leader")
                .iter()
                .any(|t| t.starts_with(LEADER_WATCH))
    })
    .await;
    bench.close().await;
}

// 검증: V58 C18 V89
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 발판에서_사무실_팀_방_멘션에_그_담당이_답한다() {
    let tg = FakeTelegram::start().await;
    let bench = Bench::linked(&tg, OWNER).await;
    bench.ready().await;
    bench
        .ai()
        .script_chat("strategy", "전략입니다. 발판에서 답합니다.");
    let body = json!({"room": "team", "text": "@[전략] 발판에서 어떻게 보세요?", "sourceMessageId": "b-1"});
    let reply = post(bench.addr(), OFFICE_CHAT_PATH, &body).await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    assert_eq!(
        reply.body["responders"],
        json!(["strategy"]),
        "{}",
        reply.text
    );
    wait_for("전략의 답이 끝난다", || {
        complete_reply(&bench, "b-1", "strategy").is_some()
    })
    .await;
    let done = complete_reply(&bench, "b-1", "strategy").unwrap();
    assert_eq!(done["content"], "전략입니다. 발판에서 답합니다.", "{done}");
    assert!(complete_reply(&bench, "b-1", "risk").is_none());
    assert_eq!(
        bench.ai().roles_with("발판에서 어떻게 보세요?"),
        vec!["strategy"]
    );
    bench.close().await;
}

// 검증: V58 V59 C19
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 발판에서_사무실_공지에_여섯_담당이_확인_답을_한다() {
    let tg = FakeTelegram::start().await;
    let bench = Bench::linked(&tg, OWNER).await;
    bench.ready().await;
    let body = json!({"text": "발판 공지입니다.", "sourceMessageId": "bn-1"});
    let reply = post(bench.addr(), OFFICE_NOTICE_PATH, &body).await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    let notice = reply.body["notice"].clone();
    wait_for("여섯 담당이 확인 답을 한다", || {
        bench
            .events("office.notice_ack")
            .iter()
            .filter(|e| e["notice"] == notice && e["state"] == "answered")
            .count()
            == 6
    })
    .await;
    bench.close().await;
}

// 검증: V89 V90 M1
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 발판에서_승인_요청을_만들면_메신저와_폰_푸시로_간다() {
    let tg = FakeTelegram::start().await;
    let bench = Bench::linked(&tg, OWNER).await;
    bench.ready().await;
    let (_number, _fingerprint) = bench.awaiting().await;
    tg.wait_sent(OWNER, "승인 요청 알림", is_approval_alert)
        .await;
    bench.push().wait(URGENT, 1).await;
    bench.close().await;
}

// 검증: V9 R14-1 T13
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 발판에서_보유가_최대_손실에_닿으면_전체_정지가_된다() {
    let tg = FakeTelegram::start().await;
    let bench = Bench::linked(&tg, OWNER).await;
    bench.ready_for_loss().await;
    bench.hold().await;
    bench.crash().await;
    let store = bench.store();
    wait_for("최대 손실로 전체 정지", || {
        store.team_state().unwrap() == TeamState::FullStop
    })
    .await;
    let last = store.team_history().unwrap().pop().unwrap();
    assert_eq!(last.cause, TeamCause::MaxLoss, "{last:?}");
    bench.close().await;
}

// 검증: V87 V88
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 발판에서_하루_ai_상한을_정해_다시_켜도_페어링이_남는다() {
    let tg = FakeTelegram::start().await;
    let bench = Bench::linked(&tg, OWNER).await;
    let bench = bench.with_ai_limit(&tg, 2).await;
    let status = bench.status().await;
    assert_eq!(status["telegram"]["paired"], json!(true), "{status}");
    bench.close().await;
}
