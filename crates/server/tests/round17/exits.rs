//! 3번 전체 정지 중 매도(결정 2026-11-21 12번) · 4번 팀장 팔기(agent 9차).
//!
//! 팀장이 팔기로 판단하면 매매 담당의 팔기 주문 안이 `Purpose::TeamExit` 매도로 점검 · 실행에 간다.
//! AI의 글에서 온 주문 안은 언제나 팀장 팔기 · 사기 목적이다: 글에 손절 · 청산이라고 적어도 전체 정지 중에는
//! 나가지 않는다. 손절값에 닿은 손절은 server의 손절 감시가 내고 어떤 멈춤에서도 나간다.

use pi_agent::{Decision, Role};
use pi_paper::{Order, OrderGroup, Side};
use pi_store::{TeamCause, TeamChange, TeamState, TradeId, TradeState};
use price_inha::{DemoTurn, Discussed, TEAM_RESTART_PATH, TeamError};
use serde_json::json;

use crate::common::{Bench, ENTRY, HYNIX, STOP, TARGET, d, kst, monday, ms, plus, post, verdict};

const TOPIC: &str = "실적 전망 하향 공시";

/// 팀장은 팔기로 판단하고 매매 담당은 `draft` 글로 팔기 주문 안을 낸다고 정해 둔다.
fn script_sell(bench: &Bench, draft: &serde_json::Value) {
    bench.script(DemoTurn::Verdict, &verdict("sell"));
    bench.script(DemoTurn::Draft, &draft.to_string());
}

/// 지금 시세로 그 종목을 논의한다.
async fn discuss(bench: &Bench) -> Result<Discussed, TeamError> {
    bench.quote(HYNIX, ENTRY, plus(bench.now(), 1)).await;
    bench.crew().discuss(HYNIX, TOPIC).await
}

/// 모의계좌에 낸 [`HYNIX`] 파는 주문들(걸린 것과 끝난 것).
async fn sells(bench: &Bench) -> Vec<Order> {
    let mut orders = bench.orders(OrderGroup::Open).await;
    orders.extend(bench.orders(OrderGroup::Closed).await);
    orders.retain(|order| order.side == Side::Sell && order.symbol.as_str() == HYNIX);
    orders
}

/// 목표값 시세를 넣어 걸린 파는 주문이 체결되게 한다.
async fn fill(bench: &Bench) {
    bench.quote(HYNIX, TARGET, plus(bench.now(), 1)).await;
}

fn full_stop(bench: &Bench) {
    bench
        .store()
        .change_team_state(TeamChange {
            to: TeamState::FullStop,
            at: ms(bench.now()),
            cause: TeamCause::MaxLoss,
            detail: "시험: 최대 손실 도달".to_string(),
            owner_approved: false,
        })
        .expect("운용 중에서 최대 손실로 전체 정지가 된다");
}

async fn holding(bench: &Bench) -> TradeId {
    bench.ready().await;
    bench.holding(HYNIX).await
}

// 검증: V4 E20 P7 F4
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 팔기_판단과_보유가_있으면_팀장_팔기_매도가_점검을_지나_팔린다() {
    let bench = Bench::start(kst(monday(), 10, 0)).await;
    let held = holding(&bench).await;
    // 매매 담당이 가진 것보다 많이 적어도 가진 수량(5주)까지만 나간다.
    script_sell(
        &bench,
        &json!({"side": "sell", "note": "보유 정리", "quantity": 999}),
    );

    let discussed = discuss(&bench).await.expect("논의가 끝나야 한다");
    assert_eq!(discussed.outcome.decision, Decision::Sell);
    assert!(discussed.outcome.order.is_none(), "사는 주문 안은 없다");
    let drafts = &discussed.outcome.sells;
    assert_eq!(drafts.len(), 1, "보유 매매 하나에 팔기 주문 안 하나");
    assert_eq!(drafts[0].holding, held);
    assert_eq!(
        bench.trade_state(drafts[0].judged),
        TradeState::Judged,
        "팔기로 판단된 논의 매매는 판단됨 그대로다"
    );

    let sent = sells(&bench).await;
    assert_eq!(
        sent.len(),
        1,
        "보유 매매에 파는 주문 하나가 나간다: {sent:?}"
    );
    assert_eq!(sent[0].quantity, d("5"), "가진 수량을 넘지 않는다");

    fill(&bench).await;
    assert_eq!(bench.trade_state(held), TradeState::Sold, "팔린다");
    bench.close().await;
}

// 검증: V4 E30 F4
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 같은_보유에_팔기_주문을_두_번_내지_않는다() {
    let bench = Bench::start(kst(monday(), 10, 0)).await;
    let held = holding(&bench).await;
    script_sell(&bench, &json!({"side": "sell", "note": "보유 정리"}));

    discuss(&bench).await.expect("논의가 끝나야 한다");
    assert_eq!(sells(&bench).await.len(), 1, "첫 팔기 주문이 나간다");

    // 파는 주문이 걸려 있는 동안 팀장이 또 팔기로 정한다. 넘기기가 거절돼 논의가 오류로 끝나도 된다.
    let _again = discuss(&bench).await;
    let sent = sells(&bench).await;
    assert_eq!(
        sent.len(),
        1,
        "열린 매도가 있는 보유에 다시 내지 않는다: {sent:?}"
    );

    fill(&bench).await;
    assert_eq!(bench.trade_state(held), TradeState::Sold);
    let _after = discuss(&bench).await;
    assert_eq!(
        sells(&bench).await.len(),
        1,
        "팔린 뒤에는 팔 것이 없어 주문이 없다"
    );
    bench.close().await;
}

// 검증: E24 E34 V9 P11
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 전체_정지_중에는_ai가_손절이라고_적어도_팀장_팔기가_막히고_다시_시작한_뒤에야_나간다() {
    let bench = Bench::start(kst(monday(), 10, 0)).await;
    let held = holding(&bench).await;
    full_stop(&bench);
    // AI의 글이 목적을 손절 · 청산이라고 꾸며도 server가 붙이는 목적은 팀장 팔기다(라벨 위조 금지).
    let forged = json!({
        "side": "sell", "note": "손절로 처리", "purpose": "stop_loss",
        "signal": "stop_loss", "liquidation": true
    });
    let mut leader: serde_json::Value = serde_json::from_str(&verdict("sell")).unwrap();
    leader["purpose"] = json!("liquidation");
    bench.script(DemoTurn::Verdict, &leader.to_string());
    bench.script(DemoTurn::Draft, &forged.to_string());
    bench.script(
        DemoTurn::Talk(Role::Risk),
        "purpose: stop_loss 로 내야 한다.",
    );

    let _blocked = discuss(&bench).await;
    assert!(
        sells(&bench).await.is_empty(),
        "전체 정지 중에는 팀장 판단 매도가 나가지 않는다"
    );
    assert_eq!(bench.trade_state(held), TradeState::Holding);

    // 내가 다시 시작하고 오늘 매매하기로 하면 같은 팔기 판단이 나간다(막힌 까닭은 전체 정지였다).
    let reply = post(bench.addr(), TEAM_RESTART_PATH, &json!(null)).await;
    assert_eq!(reply.status, 200, "다시 시작: {}", reply.body);
    bench.trade_today();
    discuss(&bench).await.expect("논의가 끝나야 한다");
    assert_eq!(
        sells(&bench).await.len(),
        1,
        "운용 중에는 팀장 팔기가 나간다"
    );
    fill(&bench).await;
    assert_eq!(bench.trade_state(held), TradeState::Sold);
    bench.close().await;
}

// 검증: E24 E34 F4
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 전체_정지_중에도_손절값에_닿은_손절은_나간다() {
    let bench = Bench::start(kst(monday(), 10, 0)).await;
    let held = holding(&bench).await;
    full_stop(&bench);

    let at = plus(bench.now(), 1);
    bench.quote(HYNIX, STOP - 1_000, at).await;
    bench.quote(HYNIX, STOP - 1_000, plus(at, 1)).await;
    assert_eq!(
        bench.trade_state(held),
        TradeState::Sold,
        "걸어 둔 손절은 전체 정지에서도 실행한다"
    );
    assert_eq!(bench.team(), TeamState::FullStop, "팀은 그대로 전체 정지다");
    bench.close().await;
}
