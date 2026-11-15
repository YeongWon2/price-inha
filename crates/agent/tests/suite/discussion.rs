//! 팀 논의 방: 0라운드 지명 · 멘션 · 넘김 · 넘기기 기록 · 상한(만드는 것 3 · 4번, 결정 14번, V2, E6).

use pi_agent::{Decision, DiscussionLimits, Error, Role, WatchReason};
use pi_ai::AiError;
use pi_store::{RoomActorKind, TradeState};
use serde_json::json;

use crate::common::{Env, Step, of_kind, request, say, script_after_discussion, watch_codes};

fn members_said(env: &Env) -> Vec<(String, u64)> {
    of_kind(&env.room("room-1"), "message.member")
        .iter()
        .map(|e| {
            (
                e.actor_id.clone(),
                e.payload["round"].as_u64().unwrap_or(99),
            )
        })
        .collect()
}

// 검증: V2 (담당이 관련 담당에게 넘긴 것이 방 로그에 남는다)
#[tokio::test]
async fn 넘기기가_방_로그에_남는다() {
    let env = Env::new();
    let trade = env.trade();
    env.ai.script(
        Role::Scout,
        vec![say("공시가 떴다. @strategy @risk @ghost @scout 봐 줘")],
    );
    env.ai.script(Role::Strategy, vec![say("살 만하다")]);
    env.ai.script(Role::Risk, vec![say("비중은 작게")]);
    script_after_discussion(&env.ai);
    let out = env.team().discuss(request(trade)).await.unwrap();
    assert_eq!(out.decision, Decision::Buy);
    let events = env.room("room-1");
    let said = of_kind(&events, "message.member");
    let scout = said.iter().find(|e| e.actor_id == "scout").unwrap();
    assert_eq!(scout.actor_kind, RoomActorKind::Member);
    assert_eq!(scout.payload["handoff"], json!(["strategy", "risk"]));
    assert_eq!(scout.payload["round"], json!(0));
    assert!(
        scout.payload["text"]
            .as_str()
            .unwrap()
            .contains("공시가 떴다")
    );
    let strategy = said.iter().find(|e| e.actor_id == "strategy").unwrap();
    assert_eq!(strategy.payload["handoff"], json!([]));
}

// 검증: V2 결정 14 (0라운드는 지명된 담당만, 부르지 않으면 조용한 라운드로 끝)
#[tokio::test]
async fn 영라운드는_지명된_담당만_답한다() {
    let env = Env::new();
    let trade = env.trade();
    env.ai
        .script(Role::Scout, vec![say("특별한 건 없지만 기록한다")]);
    script_after_discussion(&env.ai);
    let out = env.team().discuss(request(trade)).await.unwrap();
    assert_eq!(out.decision, Decision::Buy);
    assert_eq!(members_said(&env), [("scout".to_string(), 0)]);
}

// 검증: V2 결정 14 (뒤 라운드는 불린 담당만, 라운드마다 순서를 한 칸씩 돌린다)
#[tokio::test]
async fn 불린_담당이_다음_라운드에_돌아가며_답한다() {
    let env = Env::new();
    let trade = env.trade();
    env.ai
        .script(Role::Scout, vec![say("@strategy @risk 공시 봐 줘")]);
    env.ai.script(Role::Strategy, vec![say("전략 의견")]);
    env.ai.script(Role::Risk, vec![say("리스크 의견")]);
    script_after_discussion(&env.ai);
    env.team().discuss(request(trade)).await.unwrap();
    let expected = [("scout", 0), ("risk", 1), ("strategy", 1)];
    let expected: Vec<(String, u64)> = expected.iter().map(|(a, r)| (a.to_string(), *r)).collect();
    assert_eq!(members_said(&env), expected);
}

// 검증: 결정 14 (넘김은 글을 남기지 않는다)
#[tokio::test]
async fn 넘김은_글을_남기지_않는다() {
    let env = Env::new();
    let trade = env.trade();
    env.ai
        .script(Role::Scout, vec![say("@strategy @risk 봐 줘")]);
    env.ai.script(Role::Strategy, vec![say("살 만하다")]);
    env.ai.script(Role::Risk, vec![say(" (PASS). ")]);
    script_after_discussion(&env.ai);
    let out = env.team().discuss(request(trade)).await.unwrap();
    assert_eq!(out.decision, Decision::Buy);
    let speakers: Vec<String> = members_said(&env).into_iter().map(|(a, _)| a).collect();
    assert_eq!(speakers, ["scout", "strategy"]);
}

// 검증: E6 결정 14 (지명된 담당의 넘김은 응답 실패)
#[tokio::test]
async fn 지명된_담당이_넘기면_응답_실패로_지켜본다() {
    let env = Env::new();
    let trade = env.trade();
    env.ai.script(Role::Scout, vec![say("(pass)")]);
    let out = env.team().discuss(request(trade)).await.unwrap();
    assert_eq!(
        out.decision,
        Decision::Watch(WatchReason::RoleUnavailable(Role::Scout))
    );
    assert_eq!(out.order, None);
}

// 검증: E6 M2 (논의 중 담당 하나가 실패하면 지켜보기, 매매는 논의중 그대로)
#[tokio::test]
async fn 논의_중_담당이_실패하면_지켜보기다() {
    let env = Env::new();
    let trade = env.trade();
    env.ai
        .script(Role::Scout, vec![say("@strategy @risk 봐 줘")]);
    env.ai
        .script(Role::Strategy, vec![Step::Fail(AiError::Connection)]);
    env.ai.script(Role::Risk, vec![say("비중은 작게")]);
    script_after_discussion(&env.ai);
    let out = env.team().discuss(request(trade)).await.unwrap();
    assert_eq!(
        out.decision,
        Decision::Watch(WatchReason::RoleUnavailable(Role::Strategy))
    );
    assert_eq!(out.order, None);
    assert!(env.tools.drafts().is_empty());
    assert_eq!(
        env.store.get_trade(trade).unwrap().state,
        TradeState::Discussing
    );
    let events = env.room("room-1");
    let failed = of_kind(&events, "turn.failed");
    assert_eq!(failed.len(), 1);
    assert_eq!(failed[0].actor_kind, RoomActorKind::Gateway);
    assert_eq!(failed[0].payload["member"], json!("strategy"));
    assert_eq!(watch_codes(&events), ["role_unavailable"]);
}

// 검증: 결정 14 E27 (라운드 상한 3에 닿으면 지켜보기)
#[tokio::test]
async fn 라운드_상한에_닿으면_지켜보기다() {
    let env = Env::new();
    let trade = env.trade();
    env.ai.script(
        Role::Scout,
        vec![say("@strategy 어때?"), say("@strategy 다시 봐")],
    );
    env.ai.script(Role::Strategy, vec![say("@scout 글쎄")]);
    let out = env.team().discuss(request(trade)).await.unwrap();
    assert_eq!(
        out.decision,
        Decision::Watch(WatchReason::DiscussionBounded)
    );
    assert_eq!(out.order, None);
    assert_eq!(
        env.store.get_trade(trade).unwrap().state,
        TradeState::Discussing
    );
    assert_eq!(members_said(&env).len(), 3);
    assert_eq!(watch_codes(&env.room("room-1")), ["discussion_bounded"]);
}

// 검증: 결정 14 (글 수 상한에 닿으면 남은 담당을 부르지 않고 지켜보기)
#[tokio::test]
async fn 글_수_상한에_닿으면_지켜보기다() {
    let env = Env::new();
    let trade = env.trade();
    env.ai.script(Role::Scout, vec![say("시세 의견")]);
    env.ai.script(Role::Strategy, vec![say("전략 의견")]);
    let mut req = request(trade);
    req.nominated = vec![Role::Scout, Role::Strategy, Role::Risk];
    req.limits = DiscussionLimits {
        max_messages: 2,
        ..DiscussionLimits::default()
    };
    let out = env.team().discuss(req).await.unwrap();
    assert_eq!(
        out.decision,
        Decision::Watch(WatchReason::DiscussionBounded)
    );
    assert_eq!(env.ai.calls(Role::Risk), 0);
}

// 검증: 결정 14 (참가 담당 2~6명, 겹침 없음, 지명은 참가 담당 가운데)
#[tokio::test]
async fn 참가_담당이_틀리면_거부하고_아무것도_쓰지_않는다() {
    let env = Env::new();
    let trade = env.trade();
    let cases: [(&str, Vec<Role>, Vec<Role>); 5] = [
        ("한 명", vec![Role::Scout], vec![Role::Scout]),
        (
            "겹침",
            vec![Role::Scout, Role::Scout, Role::Risk],
            vec![Role::Scout],
        ),
        ("지명 없음", vec![Role::Scout, Role::Risk], vec![]),
        (
            "지명이 참가 밖",
            vec![Role::Scout, Role::Risk],
            vec![Role::Review],
        ),
        (
            "상한 넘음",
            vec![Role::Scout, Role::Strategy, Role::Risk],
            vec![Role::Scout],
        ),
    ];
    for (why, members, nominated) in cases {
        let mut req = request(trade);
        req.members = members;
        req.nominated = nominated;
        if why == "상한 넘음" {
            req.limits.max_members = 2;
        }
        let result = env.team().discuss(req).await;
        assert!(matches!(result, Err(Error::InvalidMembers)), "{why}");
    }
    assert!(env.room("room-1").is_empty());
    assert!(env.ai.requests().is_empty());
}
