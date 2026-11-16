//! 매매 방향을 저장소에 남기고 그것으로 사기 주문 안을 막는다(store 12차 API · V4 · E20).

use pi_agent::{Decision, Error, Role};
use pi_store::{Judgment, Rationale, TradeSide, UnixMillis};

use crate::common::{Env, Step, request, say, score_reply, trader_buy};

/// 점수 70 · 80 · 60(합 210) 뒤 팀장이 이 방향으로 답한다.
fn script_leader(env: &Env, decision: &str) {
    env.ai.script(
        Role::Scout,
        vec![say("공시 요약"), Step::Say(score_reply(70, "a"))],
    );
    env.ai
        .script(Role::Strategy, vec![Step::Say(score_reply(80, "b"))]);
    env.ai
        .script(Role::Risk, vec![Step::Say(score_reply(60, "c"))]);
    let reply = format!(
        "{{\"decision\": \"{decision}\", \"hypothesis\": \"실적\", \"trigger\": \"공시\", \
         \"entry\": 70000, \"stop\": 65000, \"target\": 80000, \"expected_value\": 3000}}"
    );
    env.ai.script(Role::Leader, vec![say(&reply)]);
    env.ai
        .script(Role::Trader, vec![Step::Forever(trader_buy("사자"))]);
}

// 검증: V4 E20 (판단과 함께 방향을 저장소에 남긴다)
#[tokio::test]
async fn 판단의_방향을_저장소에_남긴다() {
    let env = Env::new();
    let sold = env.trade();
    script_leader(&env, "sell");
    let out = env.team().discuss(request(sold)).await.unwrap();
    assert_eq!(out.decision, Decision::Sell);
    assert_eq!(env.store.trade_side(sold).unwrap(), Some(TradeSide::Sell));

    let bought = env.trade();
    script_leader(&env, "buy");
    let out = env.team().discuss(request(bought)).await.unwrap();
    assert_eq!(out.decision, Decision::Buy);
    assert_eq!(env.store.trade_side(bought).unwrap(), Some(TradeSide::Buy));
}

// 검증: V4 E20 (다시 띄운 팀도 팔기 매매로는 주문 안을 내지 않는다)
#[tokio::test]
async fn 새_팀도_팔기_매매로는_주문_안을_내지_않는다() {
    let env = Env::new();
    let trade = env.trade();
    script_leader(&env, "sell");
    env.team().discuss(request(trade)).await.unwrap();
    let fresh = env.team();
    match fresh.draft_order(trade).await {
        Err(Error::TradeNotBuy { trade: t }) => assert_eq!(t, trade),
        other => panic!("거부되지 않음: {other:?}"),
    }
    assert_eq!(env.ai.calls(Role::Trader), 0);
    assert!(env.tools.drafts().is_empty());
}

// 검증: V4 E20 (저장소에 팔기로 판단된 매매는 팀을 거치지 않았어도 막는다)
#[tokio::test]
async fn 저장소의_팔기_방향으로_막는다() {
    let env = Env::new();
    let trade = env.trade();
    let judgment = Judgment {
        decided_at: UnixMillis(1),
        rationale: Rationale {
            hypothesis: Some("고점".to_string()),
            ..Rationale::default()
        },
        sources: Vec::new(),
        scores: Vec::new(),
        consensus_score: 210,
    };
    env.store
        .judge_trade_with_side(trade, judgment, TradeSide::Sell)
        .unwrap();
    env.ai
        .script(Role::Trader, vec![Step::Forever(trader_buy("사자"))]);
    match env.team().draft_order(trade).await {
        Err(Error::TradeNotBuy { trade: t }) => assert_eq!(t, trade),
        other => panic!("거부되지 않음: {other:?}"),
    }
    assert_eq!(env.ai.calls(Role::Trader), 0);
}
