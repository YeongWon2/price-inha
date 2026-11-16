//! 2차 작업: 사기 주문 안은 사기 판단에만, 근거 칸 검사, 매매 담당 참고값(V4 · P7 · P9 · E20).

use pi_agent::{Decision, Error, Role, WatchReason};
use pi_store::TradeState;
use rust_decimal::Decimal;

use crate::common::{Env, Step, request, say, score_reply, trader_buy, watch_codes};

/// 팀장 답 글. 칸 값은 그대로 JSON 조각이다.
fn leader(decision: &str, stop: &str, entry: &str, target: &str, expected: &str) -> String {
    format!(
        "{{\"decision\": \"{decision}\", \"hypothesis\": \"실적\", \"trigger\": \"공시\", \
         \"entry\": {entry}, \"stop\": {stop}, \"target\": {target}, \"expected_value\": {expected}}}"
    )
}

/// 점수 70 · 80 · 60(합 210) 뒤 팀장이 이 글로 답한다.
fn script_leader(env: &Env, reply: &str) {
    env.ai.script(
        Role::Scout,
        vec![say("공시 요약"), Step::Say(score_reply(70, "a"))],
    );
    env.ai
        .script(Role::Strategy, vec![Step::Say(score_reply(80, "b"))]);
    env.ai
        .script(Role::Risk, vec![Step::Say(score_reply(60, "c"))]);
    env.ai.script(Role::Leader, vec![say(reply)]);
}

// 검증: V4 E20 (팔기로 판단된 매매에는 사기 주문 안을 내지 않는다)
#[tokio::test]
async fn 팔기로_판단된_매매로는_주문_안을_내지_않는다() {
    let env = Env::new();
    let trade = env.trade();
    script_leader(&env, &leader("sell", "65000", "70000", "80000", "3000"));
    env.ai
        .script(Role::Trader, vec![Step::Forever(trader_buy("사자"))]);
    let team = env.team();
    let out = team.discuss(request(trade)).await.unwrap();
    assert_eq!(out.decision, Decision::Sell);
    assert_eq!(out.order, None);
    assert_eq!(
        env.store.get_trade(trade).unwrap().state,
        TradeState::Judged
    );
    let result = team.draft_order(trade).await;
    match result {
        Err(Error::TradeNotBuy { trade: t }) => assert_eq!(t, trade),
        other => panic!("거부되지 않음: {other:?}"),
    }
    assert_eq!(env.ai.calls(Role::Trader), 0);
    assert!(env.tools.drafts().is_empty());
}

// 검증: P9 (팔기도 근거 칸이 비면 지켜보기, 판단을 저장하지 않는다)
#[tokio::test]
async fn 근거가_빈_팔기는_지켜본다() {
    let env = Env::new();
    let trade = env.trade();
    let no_stop = "{\"decision\": \"sell\", \"hypothesis\": \"고점\", \"trigger\": \"공시\", \
                   \"entry\": 70000, \"target\": 80000, \"expected_value\": 3000}";
    script_leader(&env, no_stop);
    let out = env.team().discuss(request(trade)).await.unwrap();
    assert_eq!(
        out.decision,
        Decision::Watch(WatchReason::RationaleIncomplete)
    );
    assert_eq!(out.order, None);
    assert_eq!(
        env.store.get_trade(trade).unwrap().state,
        TradeState::Discussing
    );
    assert_eq!(watch_codes(&env.room("room-1")), ["rationale_incomplete"]);
}

// 검증: P9 (금액 칸이 0 · 음수면 근거가 찬 것으로 보지 않는다, 사기 · 팔기 표로)
#[tokio::test]
async fn 금액_칸이_0이거나_음수면_지켜본다() {
    let cases = [
        (
            "buy",
            leader("buy", "0", "70000", "80000", "3000"),
            "손절 0",
        ),
        (
            "buy",
            leader("buy", "65000", "-1", "80000", "3000"),
            "진입 음수",
        ),
        (
            "sell",
            leader("sell", "65000", "70000", "0", "3000"),
            "목표 0",
        ),
        (
            "sell",
            leader("sell", "65000", "70000", "80000", "-5"),
            "기대값 음수",
        ),
    ];
    for (side, reply, why) in cases {
        let env = Env::new();
        let trade = env.trade();
        script_leader(&env, &reply);
        let out = env.team().discuss(request(trade)).await.unwrap();
        assert_eq!(
            out.decision,
            Decision::Watch(WatchReason::RationaleIncomplete),
            "{side} {why}"
        );
        assert_eq!(out.order, None, "{side} {why}");
        assert_eq!(env.ai.calls(Role::Trader), 0, "{side} {why}");
        assert_eq!(
            env.store.get_trade(trade).unwrap().state,
            TradeState::Discussing,
            "{side} {why}"
        );
    }
}

// 검증: P9 (금액 칸이 모두 양수면 사기 · 팔기 모두 판단됨이 된다)
#[tokio::test]
async fn 금액_칸이_모두_양수면_판단된다() {
    for (side, expected) in [("buy", Decision::Buy), ("sell", Decision::Sell)] {
        let env = Env::new();
        let trade = env.trade();
        script_leader(&env, &leader(side, "65000", "70000", "80000", "3000"));
        env.ai
            .script(Role::Trader, vec![Step::Say(trader_buy("사자"))]);
        let out = env.team().discuss(request(trade)).await.unwrap();
        assert_eq!(out.decision, expected, "{side}");
        assert_eq!(
            env.store.get_trade(trade).unwrap().state,
            TradeState::Judged,
            "{side}"
        );
    }
}

// 검증: P7 V4 (매매 담당이 적은 수량 · 가격은 참고값으로 넘어간다)
#[tokio::test]
async fn 매매_담당의_수량과_가격은_참고값으로_넘어간다() {
    let env = Env::new();
    let trade = env.trade();
    script_leader(&env, &leader("buy", "65000", "70000", "80000", "3000"));
    let reply = "{\"side\": \"buy\", \"note\": \"분할\", \"quantity\": 30, \"price\": 71000}";
    env.ai.script(Role::Trader, vec![say(reply)]);
    let out = env.team().discuss(request(trade)).await.unwrap();
    let order = out.order.expect("주문 안");
    assert_eq!(order.ai_quantity, Some(Decimal::from(30)));
    assert_eq!(order.ai_price, Some(Decimal::from(71000)));
    assert_eq!(env.tools.drafts(), [order]);
}

// 검증: P7 (쓸 수 없는 수량 · 가격은 참고값 없음으로 두고 주문 안은 그대로 낸다, 표로)
#[tokio::test]
async fn 쓸_수_없는_참고값은_비워_둔다() {
    let cases = [
        ("안 적음", "{\"side\": \"buy\", \"note\": \"x\"}"),
        ("0", "{\"side\": \"buy\", \"quantity\": 0, \"price\": 0}"),
        (
            "음수",
            "{\"side\": \"buy\", \"quantity\": -3, \"price\": -100}",
        ),
        (
            "글",
            "{\"side\": \"buy\", \"quantity\": \"많이\", \"price\": \"싸게\"}",
        ),
        (
            "null",
            "{\"side\": \"buy\", \"quantity\": null, \"price\": null}",
        ),
    ];
    for (why, reply) in cases {
        let env = Env::new();
        let trade = env.trade();
        script_leader(&env, &leader("buy", "65000", "70000", "80000", "3000"));
        env.ai.script(Role::Trader, vec![say(reply)]);
        let out = env.team().discuss(request(trade)).await.unwrap();
        let order = out.order.unwrap_or_else(|| panic!("주문 안 없음: {why}"));
        assert_eq!(order.ai_quantity, None, "{why}");
        assert_eq!(order.ai_price, None, "{why}");
    }
}
