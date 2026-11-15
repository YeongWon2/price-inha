//! 매매 담당의 주문 안(만드는 것 3-4 · 5번, V4 · P7 · E20).

use pi_agent::{Decision, Error, OrderDraft, Role, Side, TurnFailure};
use pi_store::{BlockKind, BlockReason, Judgment, Rationale, TradeId, TradeState, UnixMillis};

use crate::common::{Env, NOW, SYMBOL, Step, request, say, script_after_discussion, trader_buy};

fn judge(env: &Env, trade: TradeId) {
    let judgment = Judgment {
        decided_at: NOW,
        rationale: Rationale::default(),
        sources: vec![],
        scores: vec![],
        consensus_score: 0,
    };
    env.store.judge_trade(trade, judgment).unwrap();
}

// 검증: V4 (팀장이 사기로 정한 매매에 매매 담당의 주문 안이 생긴다)
#[tokio::test]
async fn 사기로_정하면_주문_안이_생긴다() {
    let env = Env::new();
    let trade = env.trade();
    env.ai.script(Role::Scout, vec![say("공시 요약")]);
    script_after_discussion(&env.ai);
    let out = env.team().discuss(request(trade)).await.unwrap();
    let draft = OrderDraft {
        trade,
        symbol: SYMBOL.into(),
        side: Side::Buy,
        note: "공시 뒤 사기".into(),
    };
    assert_eq!(out.decision, Decision::Buy);
    assert_eq!(out.order, Some(draft.clone()));
    assert_eq!(env.tools.drafts(), [draft]);
    assert_eq!(env.ai.calls(Role::Trader), 1);
}

// 검증: P7 V4 (수량 · 가격은 주문 안에 담지 않는다)
#[tokio::test]
async fn 매매_담당이_적은_수량과_가격은_버린다() {
    let env = Env::new();
    let trade = env.trade();
    judge(&env, trade);
    let reply = "주문 안입니다 {\"side\": \"buy\", \"note\": \"분할 매수\", \"quantity\": 1000, \
                 \"price\": 71000, \"order_type\": \"market\"}";
    env.ai.script(Role::Trader, vec![say(reply)]);
    let draft = env.team().draft_order(trade).await.unwrap();
    let expected = OrderDraft {
        trade,
        symbol: SYMBOL.into(),
        side: Side::Buy,
        note: "분할 매수".into(),
    };
    assert_eq!(draft, expected);
    assert_eq!(env.tools.drafts(), [expected]);
}

// 검증: E20 (판단됨이 아닌 매매로는 주문 안을 내지 않는다, 표로)
#[tokio::test]
async fn 판단됨이_아닌_매매로는_주문_안을_내지_않는다() {
    let env = Env::new();
    let discussing = env.trade();
    let blocked = env.trade();
    judge(&env, blocked);
    let reason = BlockReason {
        kind: BlockKind::PrincipleOrRationale,
        detail: "근거 부족".into(),
    };
    env.store
        .block_trade(blocked, reason, UnixMillis(NOW.0 + 1))
        .unwrap();
    env.ai
        .script(Role::Trader, vec![Step::Forever(trader_buy("사자"))]);
    let team = env.team();
    for (trade, state) in [
        (discussing, TradeState::Discussing),
        (blocked, TradeState::Blocked),
    ] {
        let result = team.draft_order(trade).await;
        match result {
            Err(Error::TradeNotJudged { trade: t, state: s }) => {
                assert_eq!((t, s), (trade, state));
            }
            other => panic!("거부되지 않음: {other:?}"),
        }
    }
    assert_eq!(env.ai.calls(Role::Trader), 0);
    assert!(env.tools.drafts().is_empty());
}

// 검증: E20 V4 (매매 담당의 답이 틀리면 주문 안을 넘기지 않는다, 표로)
#[tokio::test]
async fn 매매_담당의_답이_틀리면_주문_안이_없다() {
    let cases = [
        ("팔기", "{\"side\": \"sell\", \"note\": \"x\"}"),
        ("방향 없음", "{\"note\": \"x\"}"),
        ("JSON 아님", "그냥 사자"),
    ];
    for (why, reply) in cases {
        let env = Env::new();
        let trade = env.trade();
        judge(&env, trade);
        env.ai.script(Role::Trader, vec![say(reply)]);
        let result = env.team().draft_order(trade).await;
        assert!(
            matches!(
                result,
                Err(Error::RoleUnavailable {
                    role: Role::Trader,
                    cause: TurnFailure::InvalidStructuredReply
                })
            ),
            "{why}"
        );
        assert!(env.tools.drafts().is_empty(), "{why}");
    }
}
