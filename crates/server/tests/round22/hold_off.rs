//! 16차 1번: 전체 정지가 되면 실행기에 남은 다시 내기 대기 가운데 팀장 팔기(`TeamExit`)만 지운다. 손절 · 청산 ·
//! 익절 대기는 남는다(E34). 지운 매매마다 방 기록에 한 줄을 남기고, 다시 시작한 뒤에도 정지 전 팀장 판단은 나가지 않는다.

use std::time::Instant;

use pi_paper::Fault;
use pi_rules::Purpose;
use pi_store::{TeamState, TradeId};
use price_inha::{DemoTurn, TEAM_RESTART_PATH, TRADE_ROOM_PREFIX};
use serde_json::{Value, json};

use crate::common::{
    Bench, ENTRY, HYNIX, HYUNDAI, NAVER, Plan, STEP, STOP, TARGET, kst, monday, order, plus, post,
    verdict,
};

/// 전체 정지로 팀장 팔기 대기를 거둘 때 그 매매의 방에 남기는 글(지시서 글자 그대로).
const WITHDRAWN: &str = "전체 정지로 팀장 팔기 다시 내기를 거둠";

/// 그 매매의 방에 남은 줄 가운데 팀장 팔기 대기를 거뒀다는 줄의 수.
fn withdrawn_lines(bench: &Bench, trade: TradeId) -> usize {
    let room = format!("{TRADE_ROOM_PREFIX}{}", trade.0);
    bench
        .store()
        .room_events(&room, 0, 1000)
        .unwrap()
        .iter()
        .filter(|event| event.payload.to_string().contains(WITHDRAWN))
        .count()
}

/// 그 매매의 다시 내기 대기의 목적들.
fn purposes(bench: &Bench, trade: TradeId) -> Vec<String> {
    bench
        .pending_sells(trade)
        .iter()
        .map(|pending| pending["purpose"].as_str().unwrap_or_default().to_string())
        .collect()
}

/// 보유 셋이 각각 팀장 팔기 · 익절 · 손절 대기를 갖게 한다: 증권사가 주문을 받지 않는 동안 팀장이 팔기로 판단하고,
/// 익절을 내고, 세 번째 종목이 `stop_price`로 내려 손절값에 닿는다. 계좌는 아직 받지 않는다.
/// (팀장 팔기 매매, 익절 매매, 손절 매매)를 돌려준다.
async fn three_waits(
    bench: &Bench,
    changes: Value,
    stop_price: i64,
) -> (TradeId, TradeId, TradeId) {
    bench.data.disclose(HYNIX, "20261116000123");
    bench.ready_with(changes).await;
    let leader_exit = bench.holding(HYNIX).await;
    let profit = bench.holding(NAVER).await;
    let stop = bench.holding(HYUNDAI).await;
    assert_eq!(
        bench.team(),
        TeamState::Operating,
        "손실 전에는 운용 중이다"
    );

    // 모의계좌가 주문을 받지 않는다: 팀장 팔기 · 익절 · 손절이 모두 실패해 다시 내기를 기다린다.
    bench.trading().paper().set_fault(Some(Fault::RejectOrders));
    bench.script(DemoTurn::Verdict, &verdict("sell"));
    let draft = json!({"side": "sell", "note": "보유 정리"});
    bench.script(DemoTurn::Draft, &draft.to_string());
    bench.quote(HYNIX, ENTRY, plus(bench.now(), 1)).await;
    let _failed = bench.crew().discuss(HYNIX, "실적 전망 하향 공시").await;
    let at = bench.now();
    let _failed = bench
        .trading()
        .place(order(profit, NAVER, Purpose::TakeProfit, TARGET, at))
        .await;
    bench.quote(HYUNDAI, stop_price, plus(bench.now(), 1)).await;
    assert_eq!(
        purposes(bench, leader_exit),
        ["team_exit"],
        "팀장 팔기 대기"
    );
    assert_eq!(purposes(bench, profit), ["take_profit"], "익절 대기");
    assert_eq!(purposes(bench, stop), ["stop_loss"], "손절 대기");
    (leader_exit, profit, stop)
}

// 검증: E30 E34 C8 F4
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 최대_손실로_전체_정지가_되면_팀장_팔기_대기만_지워지고_방_기록_한_줄이_남고_손절_익절_대기는_남는다()
 {
    let bench = Plan::demo().start(kst(monday(), 10, 0)).await;
    // 손절값 아래로 크게 내려 손절이 실패하고(대기), 같은 시세로 최대 낙폭에 닿는다(5주 × 4만 원 = 평가액의 2%).
    let changes = json!({
        "symbol_weight_pct": "10",
        "daily_loss_limit_pct": "0.5",
        "max_drawdown_pct": "1",
    });
    let (leader_exit, profit, stop) = three_waits(&bench, changes, STOP - 35_000).await;

    assert_eq!(bench.team(), TeamState::FullStop, "최대 손실로 전체 정지다");
    // 지우는 일이 뒤에서 도는 일을 거쳐도 몇 바퀴 안에 끝난다.
    let deadline = Instant::now() + STEP;
    while !bench.pending_sells(leader_exit).is_empty() {
        assert!(
            Instant::now() < deadline,
            "전체 정지가 되면 팀장 팔기 대기를 지워야 한다"
        );
        bench.advance(20).await;
    }
    for _ in 0..5 {
        bench.advance(20).await;
    }

    assert_eq!(
        withdrawn_lines(&bench, leader_exit),
        1,
        "팀장 팔기 대기를 지운 매매의 방에 한 줄이 한 번만 남는다"
    );
    assert_eq!(
        withdrawn_lines(&bench, profit) + withdrawn_lines(&bench, stop),
        0,
        "지우지 않은 매매의 방에는 남기지 않는다"
    );
    assert_eq!(purposes(&bench, stop), ["stop_loss"], "손절 대기는 남는다");
    assert_eq!(
        purposes(&bench, profit),
        ["take_profit"],
        "익절 대기도 남는다"
    );
    bench.close().await;
}

// 검증: E30 E34 C8 V9
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 전체_정지_뒤_손절_대기는_계속_나가고_다시_시작해도_정지_전_팀장_팔기는_나가지_않는다() {
    let bench = Plan::demo().start(kst(monday(), 10, 0)).await;
    // 손절값 바로 아래라 손실은 한도 안이다. 전체 정지는 다른 길(저장소가 바로 바꾼다)로 만든다.
    let changes = json!({"symbol_weight_pct": "10"});
    let (leader_exit, _profit, stop) = three_waits(&bench, changes, STOP - 1_000).await;
    bench.full_stop();
    let deadline = Instant::now() + STEP;
    while !bench.pending_sells(leader_exit).is_empty() {
        assert!(
            Instant::now() < deadline,
            "어떤 길로든 전체 정지가 되면 팀장 팔기 대기를 지워야 한다"
        );
        bench.advance(20).await;
    }

    // 계좌가 돌아오면 손절은 전체 정지 중에도 다시 나간다(E34).
    bench.trading().paper().set_fault(None);
    let deadline = Instant::now() + STEP;
    while bench.sells(HYUNDAI).await.is_empty() {
        assert!(
            Instant::now() < deadline,
            "전체 정지 중에도 손절 대기는 다시 나가야 한다"
        );
        bench.advance(20).await;
    }
    assert!(
        bench.pending_sells(stop).is_empty(),
        "다시 나간 손절은 대기에서 빠진다"
    );

    // 내가 다시 시작해 오늘 매매하기로 해도 정지 전의 팀장 판단은 저절로 나가지 않는다.
    let reply = post(bench.addr(), TEAM_RESTART_PATH, &json!({})).await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    assert_eq!(
        bench.team(),
        TeamState::Resting,
        "다시 시작하면 쉬는 중이다"
    );
    bench.trade_today();
    for _ in 0..6 {
        bench.advance(20).await;
    }
    assert!(
        bench.sells(HYNIX).await.is_empty(),
        "정지 전 팀장 팔기 판단은 다시 시작 뒤에도 나가지 않는다"
    );
    assert!(
        bench.pending_sells(leader_exit).is_empty(),
        "팀장 팔기 대기는 되살아나지 않는다"
    );
    bench.close().await;
}
