//! 3차 작업(후보 일괄 처리): 저장 전 가리기(E38), 뒤 라운드의 빈 답은 넘김, 같은 시각 재논의의 방 로그.

use pi_agent::{Decision, Role, TurnFailure};
use pi_ai::AiError;
use pi_store::{RoomEvent, TradeId};

use crate::common::{
    Env, MASK, SECRET_MARK, Step, leader_buy, of_kind, request, say, score_reply,
    script_after_discussion, trader_buy, watch_codes,
};

/// 방 로그 줄들의 내용 글 전부.
fn payloads(events: &[RoomEvent]) -> String {
    events
        .iter()
        .map(|e| e.payload.to_string())
        .collect::<Vec<_>>()
        .join("\n")
}

/// 매매에 남은 판단(근거 · 점수 · 자료)을 글로.
fn judgment_text(env: &Env, trade: TradeId) -> String {
    let found = env.store.get_trade(trade).unwrap();
    format!(
        "{:?} {:?} {:?}",
        found.rationale, found.scores, found.sources
    )
}

// 검증: E38 (기록에도 가린 글만: 세션 메시지 · 방 로그 · 판단 · 주문 안)
#[tokio::test]
async fn 저장하는_글도_가린다() {
    let env = Env::new();
    let trade = env.trade();
    let mut req = request(trade);
    req.data[0].content = Some(format!("공시 본문 {SECRET_MARK}"));
    env.ai.script(
        Role::Scout,
        vec![Step::Say(format!("공시 요약 {SECRET_MARK}"))],
    );
    env.ai.script(
        Role::Scout,
        vec![Step::Say(score_reply(70, &format!("좋다 {SECRET_MARK}")))],
    );
    env.ai
        .script(Role::Strategy, vec![Step::Say(score_reply(80, "추세"))]);
    env.ai
        .script(Role::Risk, vec![Step::Say(score_reply(60, "작게"))]);
    let leader = leader_buy().replace("실적 개선", &format!("실적 {SECRET_MARK}"));
    env.ai.script(Role::Leader, vec![Step::Say(leader)]);
    env.ai.script(
        Role::Trader,
        vec![Step::Say(trader_buy(&format!("사기 {SECRET_MARK}")))],
    );
    let out = env.team().discuss(req).await.unwrap();
    assert_eq!(out.decision, Decision::Buy);
    for role in Role::ALL {
        let text = env.session_text(role.id());
        assert!(!text.contains(SECRET_MARK), "{role:?} 세션: {text}");
    }
    assert!(env.session_text("scout").contains(MASK));
    let room = payloads(&env.room("room-1"));
    assert!(!room.contains(SECRET_MARK), "방 로그: {room}");
    assert!(room.contains(MASK));
    let judged = judgment_text(&env, trade);
    assert!(!judged.contains(SECRET_MARK), "판단: {judged}");
    let drafts = env.tools.drafts();
    assert_eq!(drafts.len(), 1);
    assert!(!drafts[0].note.contains(SECRET_MARK));
    assert!(out.scores.iter().all(|s| !s.reason.contains(SECRET_MARK)));
}

// 검증: E38 (공급자 오류 문구도 가려서 `turn.failed`에 남긴다)
#[tokio::test]
async fn 실패_까닭도_가린다() {
    let env = Env::new();
    let trade = env.trade();
    let error = AiError::InvalidRequest {
        message: format!("bad key {SECRET_MARK}"),
    };
    env.ai.script(Role::Scout, vec![Step::Fail(error)]);
    env.team().discuss(request(trade)).await.unwrap();
    let events = env.room("room-1");
    let failed = of_kind(&events, "turn.failed");
    assert_eq!(failed.len(), 1);
    let reason = failed[0].payload["reason"].as_str().unwrap_or("");
    assert!(!reason.contains(SECRET_MARK), "{reason}");
    assert!(reason.contains(MASK), "{reason}");
}

// 검증: 결정 14 (원본: 뒤 라운드의 빈 답은 응답 실패가 아니라 넘김)
#[tokio::test]
async fn 뒤_라운드의_빈_답은_넘김이다() {
    let env = Env::new();
    let trade = env.trade();
    env.ai
        .script(Role::Scout, vec![say("공시 요약 @strategy 의견?")]);
    env.ai
        .script(Role::Strategy, vec![say(""), say("  "), say("")]);
    script_after_discussion(&env.ai);
    let out = env.team().discuss(request(trade)).await.unwrap();
    assert_eq!(out.decision, Decision::Buy);
    let events = env.room("room-1");
    assert!(of_kind(&events, "turn.failed").is_empty());
    let speakers: Vec<&str> = of_kind(&events, "message.member")
        .iter()
        .map(|e| e.actor_id.as_str())
        .collect();
    assert_eq!(speakers, ["scout", "leader"]);
    // 원본처럼 빈 답을 다 쓰면 `(empty)` 표식을 assistant 행으로 남긴다.
    assert!(env.session_text("strategy").contains("(empty)"));
}

// 검증: E6 (턴 실행기 자체는 여전히 빈 답 3번이면 응답 실패다)
#[tokio::test]
async fn 빈_답_표식을_남기고_실패한다() {
    let env = Env::new();
    env.ai
        .script(Role::Scout, vec![Step::Forever(String::new())]);
    let config = pi_agent::RoleConfig::new(Role::Scout, crate::common::model(Role::Scout));
    let result = env
        .runner()
        .run_turn(&config, pi_agent::TimeLimits::MARKET, "t", "안녕")
        .await;
    match result {
        Err(pi_agent::Error::RoleUnavailable { cause, .. }) => {
            assert_eq!(cause, TurnFailure::EmptyReply)
        }
        other => panic!("실패가 아님: {other:?}"),
    }
    assert!(env.session_text("scout").contains("(empty)"));
}

// 검증: V2 (같은 시각에 같은 매매를 다시 논의해도 방 로그 줄이 겹쳐 사라지거나 부딪히지 않는다)
#[tokio::test]
async fn 같은_시각_재논의도_방_로그에_남는다() {
    let env = Env::new();
    let trade = env.trade();
    let mut missing = request(trade);
    missing.data[1].content = None;
    let team = env.team();
    team.discuss(missing.clone()).await.unwrap();
    team.discuss(missing).await.unwrap();
    assert_eq!(watch_codes(&env.room("room-1")).len(), 2);
    env.ai
        .script(Role::Scout, vec![Step::Fail(AiError::Timeout)]);
    team.discuss(request(trade)).await.unwrap();
    let events = env.room("room-1");
    assert_eq!(of_kind(&events, "turn.failed").len(), 1);
    assert_eq!(watch_codes(&events).len(), 3);
}
