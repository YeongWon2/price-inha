//! 9차 작업: 팀장이 팔기로 정하면 매매 담당이 같은 종목 보유를 모두 파는 주문 안을 만든다
//! (V4 · E20 · P7 · P9 · E38, 결정 2026-11-21 13번).
//!
//! 잠긴 `common.rs`의 `FakeTools`는 그대로 두고, 팔기 주문 안을 기록하는 가짜 도구를 여기에 둔다.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use pi_agent::{
    ActionCall, Decision, Error, OrderDraft, Ports, Role, RoleConfig, SellDraft, Team, TimeLimits,
    ToolError, Tools, TurnFailure,
};
use pi_store::{
    AccountKind, Amount, Judgment, NewTrade, Rationale, TradeId, TradeSide, TradeState, UnixMillis,
};
use rust_decimal::Decimal;

use crate::common::{
    Env, FixedClock, MASK, MarkRedactor, NOW, SECRET_MARK, SYMBOL, Step, model, of_kind, request,
    say, score_reply, trader_buy,
};

/// 다른 종목.
const OTHER: &str = "000660";

/// 팔기 주문 안까지 기록하는 가짜 도구. `fail_at`번째(1부터) 팔기 넘기기는 실패한다.
#[derive(Default)]
struct SellTools {
    orders: Mutex<Vec<OrderDraft>>,
    sells: Mutex<Vec<SellDraft>>,
    fail_at: Option<usize>,
}

impl SellTools {
    fn failing_at(n: usize) -> SellTools {
        SellTools {
            fail_at: Some(n),
            ..SellTools::default()
        }
    }

    fn sells(&self) -> Vec<SellDraft> {
        self.sells.lock().unwrap().clone()
    }

    fn orders(&self) -> Vec<OrderDraft> {
        self.orders.lock().unwrap().clone()
    }
}

#[async_trait]
impl Tools for SellTools {
    fn names(&self, _role: Role) -> Vec<String> {
        vec!["quote".into()]
    }

    async fn call(&self, _role: Role, call: &ActionCall) -> Result<String, ToolError> {
        Ok(format!("결과:{}", call.name))
    }

    async fn submit_order_draft(&self, draft: &OrderDraft) -> Result<(), ToolError> {
        self.orders.lock().unwrap().push(draft.clone());
        Ok(())
    }

    async fn submit_sell_draft(&self, draft: &SellDraft) -> Result<(), ToolError> {
        let mut sells = self.sells.lock().unwrap();
        if self.fail_at == Some(sells.len() + 1) {
            return Err(ToolError("넘기기 실패".into()));
        }
        sells.push(draft.clone());
        Ok(())
    }
}

/// 이 가짜 도구를 쓰는 팀.
fn team_with(env: &Env, tools: &Arc<SellTools>) -> Team {
    let ports = Ports {
        ai: env.ai.clone(),
        tools: tools.clone(),
        store: env.store.clone(),
        redactor: Arc::new(MarkRedactor),
        clock: Arc::new(FixedClock),
    };
    let roles = Role::ALL
        .iter()
        .map(|role| RoleConfig::new(*role, model(*role)))
        .collect();
    Team::new(ports, roles, TimeLimits::MARKET).unwrap()
}

/// 근거 6칸이 모두 찬 판단.
fn full_judgment(hypothesis: &str) -> Judgment {
    Judgment {
        decided_at: UnixMillis(NOW.0 - 30_000),
        rationale: Rationale {
            hypothesis: Some(hypothesis.into()),
            trigger: Some("공시".into()),
            entry: Some(Amount(70_000)),
            stop: Some(Amount(65_000)),
            target: Some(Amount(80_000)),
            expected_value: Some(Amount(3_000)),
        },
        sources: Vec::new(),
        scores: Vec::new(),
        consensus_score: 210,
    }
}

/// 보유중인 매매 하나(논의중 → 판단됨 → 주문됨 → 보유중).
fn holding(env: &Env, symbol: &str, account: AccountKind) -> TradeId {
    let new = NewTrade {
        symbol: symbol.into(),
        account,
        at: UnixMillis(NOW.0 - 120_000),
    };
    let id = env.store.create_trade(new).unwrap().id;
    env.store
        .judge_trade(id, full_judgment("예전 매수"))
        .unwrap();
    env.store
        .transition_trade(id, TradeState::Ordered, UnixMillis(NOW.0 - 20_000))
        .unwrap();
    env.store
        .transition_trade(id, TradeState::Holding, UnixMillis(NOW.0 - 10_000))
        .unwrap();
    id
}

/// 저장소에 이 방향으로 판단된 논의 매매(팀을 거치지 않음).
fn judged(env: &Env, side: TradeSide) -> TradeId {
    let trade = env.trade();
    env.store
        .judge_trade_with_side(trade, full_judgment("고점 신호"), side)
        .unwrap();
    trade
}

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
        "{{\"decision\": \"{decision}\", \"hypothesis\": \"고점 신호\", \"trigger\": \"공시\", \
         \"entry\": 70000, \"stop\": 65000, \"target\": 80000, \"expected_value\": 3000}}"
    );
    env.ai.script(Role::Leader, vec![say(&reply)]);
}

/// 매매 담당의 팔기 답 글.
fn trader_sell(note: &str) -> String {
    format!("{{\"side\": \"sell\", \"note\": \"{note}\"}}")
}

/// 기대하는 팔기 주문 안(참고값 없음).
fn sell(judged: TradeId, holding: TradeId, note: &str) -> SellDraft {
    SellDraft {
        judged,
        holding,
        symbol: SYMBOL.into(),
        note: note.into(),
        ai_quantity: None,
        ai_price: None,
    }
}

// 검증: V4 P7 M2 (팔기 판단 → 같은 종목 보유 둘을 파는 주문 안 둘, 매매 담당 1번, 사기 주문 안 없음)
#[tokio::test]
async fn 팔기로_정하면_같은_종목_보유마다_파는_주문_안이_생긴다() {
    let env = Env::new();
    let first = holding(&env, SYMBOL, AccountKind::Paper);
    let second = holding(&env, SYMBOL, AccountKind::Paper);
    let trade = env.trade();
    script_leader(&env, "sell");
    env.ai
        .script(Role::Trader, vec![say(&trader_sell("고점 정리"))]);
    let tools = Arc::new(SellTools::default());
    let out = team_with(&env, &tools)
        .discuss(request(trade))
        .await
        .unwrap();

    let expected = vec![
        sell(trade, first, "고점 정리"),
        sell(trade, second, "고점 정리"),
    ];
    assert_eq!(out.decision, Decision::Sell);
    assert_eq!(out.order, None);
    assert_eq!(out.sells, expected);
    assert_eq!(tools.sells(), expected);
    assert!(tools.orders().is_empty());
    assert_eq!(env.ai.calls(Role::Trader), 1);
    // 판단 매매는 판단됨 그대로, 보유 매매의 상태는 포트 너머의 몫이다.
    assert_eq!(
        env.store.get_trade(trade).unwrap().state,
        TradeState::Judged
    );
    for id in [first, second] {
        assert_eq!(env.store.get_trade(id).unwrap().state, TradeState::Holding);
    }
}

// 검증: V4 E20 (보유가 없으면 매매 담당을 부르지 않고 방 로그에 nothing_held를 남긴다)
#[tokio::test]
async fn 보유가_없으면_매매_담당을_부르지_않는다() {
    let env = Env::new();
    let trade = env.trade();
    script_leader(&env, "sell");
    env.ai
        .script(Role::Trader, vec![Step::Forever(trader_sell("정리"))]);
    let tools = Arc::new(SellTools::default());
    let out = team_with(&env, &tools)
        .discuss(request(trade))
        .await
        .unwrap();

    assert_eq!(out.decision, Decision::Sell);
    assert!(out.sells.is_empty());
    assert_eq!(out.order, None);
    assert_eq!(env.ai.calls(Role::Trader), 0);
    assert!(tools.sells().is_empty());
    let events = env.room("room-1");
    let held: Vec<_> = of_kind(&events, "room.activity")
        .into_iter()
        .filter(|e| e.payload["status"] == "nothing_held")
        .collect();
    assert_eq!(held.len(), 1);
    assert_eq!(held[0].payload["trade"], serde_json::json!(trade.0));
    assert_eq!(
        env.store.get_trade(trade).unwrap().state,
        TradeState::Judged
    );
}

// 검증: V4 (다른 종목 · 다른 계좌 종류의 보유는 고르지 않는다)
#[tokio::test]
async fn 다른_종목과_다른_계좌의_보유는_고르지_않는다() {
    let env = Env::new();
    let _other_symbol = holding(&env, OTHER, AccountKind::Paper);
    let _live = holding(&env, SYMBOL, AccountKind::Live);
    let mine = holding(&env, SYMBOL, AccountKind::Paper);
    let trade = judged(&env, TradeSide::Sell);
    env.ai
        .script(Role::Trader, vec![Step::Forever(trader_sell("정리"))]);
    let tools = Arc::new(SellTools::default());
    let drafts = team_with(&env, &tools).draft_sell(trade).await.unwrap();

    assert_eq!(drafts, vec![sell(trade, mine, "정리")]);
    assert_eq!(tools.sells(), drafts);
    assert_eq!(env.ai.calls(Role::Trader), 1);
}

// 검증: V4 E20 (다른 종목 · 다른 계좌 종류의 보유만 있으면 보유 없음과 같다: AI 0번, 빈 목록)
#[tokio::test]
async fn 맞는_보유가_없으면_빈_목록이다() {
    let env = Env::new();
    holding(&env, OTHER, AccountKind::Paper);
    holding(&env, SYMBOL, AccountKind::Live);
    let trade = judged(&env, TradeSide::Sell);
    env.ai
        .script(Role::Trader, vec![Step::Forever(trader_sell("정리"))]);
    let tools = Arc::new(SellTools::default());
    let drafts = team_with(&env, &tools).draft_sell(trade).await.unwrap();

    assert!(drafts.is_empty());
    assert!(tools.sells().is_empty());
    assert_eq!(env.ai.calls(Role::Trader), 0);
}

// 검증: V4 E20 (매매 담당의 답이 틀리면 응답 실패, 넘기지 않는다, 표로)
#[tokio::test]
async fn 매매_담당의_팔기_답이_틀리면_응답_실패다() {
    let cases = [
        ("사기", "{\"side\": \"buy\", \"note\": \"x\"}"),
        ("방향 없음", "{\"note\": \"x\"}"),
        ("설명이 글이 아님", "{\"side\": \"sell\", \"note\": 3}"),
        ("JSON 아님", "그냥 팔자"),
    ];
    for (why, reply) in cases {
        let env = Env::new();
        holding(&env, SYMBOL, AccountKind::Paper);
        let trade = judged(&env, TradeSide::Sell);
        env.ai.script(Role::Trader, vec![say(reply)]);
        let tools = Arc::new(SellTools::default());
        let result = team_with(&env, &tools).draft_sell(trade).await;
        assert!(
            matches!(
                result,
                Err(Error::RoleUnavailable {
                    role: Role::Trader,
                    cause: TurnFailure::InvalidStructuredReply
                })
            ),
            "{why}: {result:?}"
        );
        assert!(tools.sells().is_empty(), "{why}");
    }
}

// 검증: V4 P9 (논의 중 매매 담당이 팔기 답을 틀리면 실패를 남기고 판단은 팔기 그대로)
#[tokio::test]
async fn 논의_뒤_매매_담당이_틀리면_팔기_판단은_그대로다() {
    for (why, reply) in [
        ("사기", trader_buy("사자")),
        ("깨진 JSON", "{\"side\": \"sell\", \"note\": ".to_string()),
    ] {
        let env = Env::new();
        holding(&env, SYMBOL, AccountKind::Paper);
        let trade = env.trade();
        script_leader(&env, "sell");
        env.ai.script(Role::Trader, vec![say(&reply)]);
        let tools = Arc::new(SellTools::default());
        let out = team_with(&env, &tools)
            .discuss(request(trade))
            .await
            .unwrap();

        assert_eq!(out.decision, Decision::Sell, "{why}");
        assert!(out.sells.is_empty(), "{why}");
        assert_eq!(out.order, None, "{why}");
        assert!(tools.sells().is_empty(), "{why}");
        assert_eq!(
            env.store.get_trade(trade).unwrap().state,
            TradeState::Judged,
            "{why}"
        );
        let events = env.room("room-1");
        let failed: Vec<_> = of_kind(&events, "turn.failed")
            .into_iter()
            .filter(|e| e.payload["member"] == "trader")
            .collect();
        assert_eq!(failed.len(), 1, "{why}");
    }
}

// 검증: P7 V4 (참고 수량 · 가격은 버리지 않고 참고값으로, 0 · 음수 · 숫자 아님은 None, 표로)
#[tokio::test]
async fn 팔기_참고_수량과_가격은_참고값으로만_담는다() {
    let cases = [
        (
            "둘 다 씀",
            "{\"side\": \"sell\", \"note\": \"n\", \"quantity\": 10, \"price\": 71000}",
            Some(Decimal::from(10)),
            Some(Decimal::from(71_000)),
        ),
        (
            "0과 음수",
            "{\"side\": \"sell\", \"note\": \"n\", \"quantity\": 0, \"price\": -5}",
            None,
            None,
        ),
        (
            "숫자 아님",
            "{\"side\": \"sell\", \"note\": \"n\", \"quantity\": \"many\", \"price\": 70500}",
            None,
            Some(Decimal::from(70_500)),
        ),
    ];
    for (why, reply, quantity, price) in cases {
        let env = Env::new();
        let first = holding(&env, SYMBOL, AccountKind::Paper);
        let second = holding(&env, SYMBOL, AccountKind::Paper);
        let trade = judged(&env, TradeSide::Sell);
        env.ai.script(Role::Trader, vec![say(reply)]);
        let tools = Arc::new(SellTools::default());
        let drafts = team_with(&env, &tools).draft_sell(trade).await.unwrap();

        let expected: Vec<SellDraft> = [first, second]
            .into_iter()
            .map(|holding| SellDraft {
                ai_quantity: quantity,
                ai_price: price,
                ..sell(trade, holding, "n")
            })
            .collect();
        assert_eq!(drafts, expected, "{why}");
        assert_eq!(tools.sells(), expected, "{why}");
    }
}

// 검증: E20 (사기 · 방향 없는 판단은 TradeNotSell, 논의중은 TradeNotJudged, 둘 다 AI 0번)
#[tokio::test]
async fn 팔기_판단이_아니면_팔기_주문_안을_내지_않는다() {
    let env = Env::new();
    holding(&env, SYMBOL, AccountKind::Paper);
    let bought = judged(&env, TradeSide::Buy);
    let no_side = env.trade();
    env.store
        .judge_trade(no_side, full_judgment("방향 없음"))
        .unwrap();
    let discussing = env.trade();
    env.ai
        .script(Role::Trader, vec![Step::Forever(trader_sell("정리"))]);
    let tools = Arc::new(SellTools::default());
    let team = team_with(&env, &tools);

    for trade in [bought, no_side] {
        match team.draft_sell(trade).await {
            Err(Error::TradeNotSell { trade: t }) => assert_eq!(t, trade),
            other => panic!("거부되지 않음: {other:?}"),
        }
    }
    match team.draft_sell(discussing).await {
        Err(Error::TradeNotJudged { trade: t, state }) => {
            assert_eq!((t, state), (discussing, TradeState::Discussing));
        }
        other => panic!("거부되지 않음: {other:?}"),
    }
    assert_eq!(env.ai.calls(Role::Trader), 0);
    assert!(tools.sells().is_empty());
}

// 검증: V4 E20 (사기 길은 그대로: 팔기 판단 매매의 draft_order는 보유가 있어도 TradeNotBuy)
#[tokio::test]
async fn 팔기_판단_매매의_사기_주문_안은_여전히_막는다() {
    let env = Env::new();
    holding(&env, SYMBOL, AccountKind::Paper);
    let trade = judged(&env, TradeSide::Sell);
    env.ai
        .script(Role::Trader, vec![Step::Forever(trader_buy("사자"))]);
    let tools = Arc::new(SellTools::default());
    match team_with(&env, &tools).draft_order(trade).await {
        Err(Error::TradeNotBuy { trade: t }) => assert_eq!(t, trade),
        other => panic!("거부되지 않음: {other:?}"),
    }
    assert_eq!(env.ai.calls(Role::Trader), 0);
    assert!(tools.orders().is_empty());
    assert!(tools.sells().is_empty());
}

// 검증: V4 (사기 판단은 지금처럼: 같은 종목 보유가 있어도 sells는 비고 order만 있다)
#[tokio::test]
async fn 사기로_정하면_팔기_주문_안은_없다() {
    let env = Env::new();
    holding(&env, SYMBOL, AccountKind::Paper);
    let trade = env.trade();
    script_leader(&env, "buy");
    env.ai.script(Role::Trader, vec![say(&trader_buy("사자"))]);
    let tools = Arc::new(SellTools::default());
    let out = team_with(&env, &tools)
        .discuss(request(trade))
        .await
        .unwrap();

    assert_eq!(out.decision, Decision::Buy);
    assert!(out.order.is_some());
    assert!(out.sells.is_empty());
    assert!(tools.sells().is_empty());
    assert_eq!(tools.orders().len(), 1);
}

// 검증: V4 E20 (다시 띄운 팀도 저장소의 방향으로 같은 결과)
#[tokio::test]
async fn 다시_띄운_팀도_저장소의_방향으로_판다() {
    let env = Env::new();
    let first = holding(&env, SYMBOL, AccountKind::Paper);
    let trade = env.trade();
    script_leader(&env, "sell");
    env.ai
        .script(Role::Trader, vec![Step::Forever(trader_sell("정리"))]);
    let before = Arc::new(SellTools::default());
    let out = team_with(&env, &before)
        .discuss(request(trade))
        .await
        .unwrap();
    assert_eq!(out.sells, vec![sell(trade, first, "정리")]);

    let after = Arc::new(SellTools::default());
    let again = team_with(&env, &after).draft_sell(trade).await.unwrap();
    assert_eq!(again, vec![sell(trade, first, "정리")]);
    assert_eq!(after.sells(), again);
}

// 검증: E38 (넘긴 설명과 저장된 세션에서 표시 글자가 가려진다)
#[tokio::test]
async fn 팔기_설명의_표시_글자를_가린다() {
    let env = Env::new();
    holding(&env, SYMBOL, AccountKind::Paper);
    let trade = judged(&env, TradeSide::Sell);
    let note = format!("정리 {SECRET_MARK}");
    env.ai.script(Role::Trader, vec![say(&trader_sell(&note))]);
    let tools = Arc::new(SellTools::default());
    let drafts = team_with(&env, &tools).draft_sell(trade).await.unwrap();

    assert_eq!(drafts.len(), 1);
    for draft in drafts.iter().chain(tools.sells().iter()) {
        assert!(!draft.note.contains(SECRET_MARK), "{}", draft.note);
        assert!(draft.note.contains(MASK), "{}", draft.note);
    }
    assert!(!env.session_text(Role::Trader.id()).contains(SECRET_MARK));
}

// 검증: P7 V4 (매매 담당 입력에 판단 근거와 시장별 금액 단위 안내가 들어간다)
#[tokio::test]
async fn 매매_담당_입력에_근거와_금액_단위가_들어간다() {
    let env = Env::new();
    holding(&env, SYMBOL, AccountKind::Paper);
    let trade = judged(&env, TradeSide::Sell);
    env.ai.script(Role::Trader, vec![say(&trader_sell("정리"))]);
    let tools = Arc::new(SellTools::default());
    team_with(&env, &tools).draft_sell(trade).await.unwrap();

    let trader = model(Role::Trader);
    let text: String = env
        .ai
        .requests()
        .iter()
        .filter(|r| r.model == trader)
        .flat_map(|r| r.messages.iter().map(|m| m.content.clone()))
        .collect();
    assert!(text.contains("고점 신호"), "{text}");
    assert!(text.contains("최소 단위"), "{text}");
    assert!(text.contains("원"), "{text}");
    assert!(text.contains("센트"), "{text}");
}

// 검증: V4 (넘기기가 실패하면 그 오류로 끝나고, 앞에서 넘긴 것은 이미 넘어갔다)
#[tokio::test]
async fn 팔기_넘기기가_실패하면_그_오류로_끝난다() {
    let env = Env::new();
    let first = holding(&env, SYMBOL, AccountKind::Paper);
    holding(&env, SYMBOL, AccountKind::Paper);
    let trade = judged(&env, TradeSide::Sell);
    env.ai.script(Role::Trader, vec![say(&trader_sell("정리"))]);
    let tools = Arc::new(SellTools::failing_at(2));
    let result = team_with(&env, &tools).draft_sell(trade).await;

    match result {
        Err(Error::Tool(ToolError(text))) => assert_eq!(text, "넘기기 실패"),
        other => panic!("실패하지 않음: {other:?}"),
    }
    assert_eq!(tools.sells(), vec![sell(trade, first, "정리")]);
    assert_eq!(env.ai.calls(Role::Trader), 1);
}

// 검증: V4 P21 (팔기 넘기기를 구현하지 않은 도구 포트는 기본으로 받지 않는다)
#[tokio::test]
async fn 기본_도구는_팔기_주문_안을_받지_않는다() {
    let env = Env::new();
    holding(&env, SYMBOL, AccountKind::Paper);
    let trade = judged(&env, TradeSide::Sell);
    env.ai.script(Role::Trader, vec![say(&trader_sell("정리"))]);
    match env.team().draft_sell(trade).await {
        Err(Error::Tool(ToolError(text))) => {
            assert_eq!(text, "팔기 주문 안을 받지 않습니다");
        }
        other => panic!("받아들임: {other:?}"),
    }
    assert!(env.tools.drafts().is_empty());
}
