//! 손실 · 멈춤 판단을 저장소 전이로(V8 · V9 · E23 · E24)와 이상 감지 멈춤(E13).

use pi_paper::Fault;
use pi_rules::{Reason, TeamStatus};
use pi_store::{TeamCause, TeamState};
use price_inha::{DayPlan, KIND_DAY_START, Placed, TEAM_RESTART_PATH, TradingError};
use serde_json::json;

use crate::common::{
    Bench, HYNIX, NAVER, buy, choose_swing, kst, monday, on_date, post, register_keys, swing_with,
    tuesday, wait_for,
};

// 검증: V8 T13 T6
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 하루_손실_한도에_닿으면_그날_매수가_멈추고_다음_거래일에_다시_판단한다() {
    let bench = Bench::demo(kst(monday(), 10, 0)).await;
    let addr = bench.addr();
    swing_with(
        addr,
        json!({"daily_loss_limit_pct": "0.5", "max_drawdown_pct": "5"}),
    )
    .await;
    bench.trade_today();
    bench.hold(HYNIX).await;

    // 5주 × 12,000원 하락 = 평가액의 0.6% 손실(하루 한도 0.5%에 닿고 최대 낙폭 5%에는 닿지 않는다).
    bench.quote(HYNIX, 88_000, kst(monday(), 10, 5)).await;
    assert_eq!(bench.team(), TeamState::DailyLimitHalt);
    let last = bench.store().team_history().unwrap().pop().unwrap();
    assert_eq!(
        (last.from, last.cause),
        (TeamState::Operating, TeamCause::DailyLossLimit)
    );

    let at = kst(monday(), 10, 6);
    bench.quote(NAVER, 100_000, at).await;
    let trade = bench.judged(NAVER);
    let placed = bench.place(buy(trade, NAVER, at)).await;
    assert!(
        matches!(&placed, Placed::Blocked(b)
            if b.reason == Reason::TeamNotOperating(TeamStatus::DailyLimitHalt)),
        "그날은 사지 않는다: {placed:?}"
    );
    let again = bench.trading().decide_day(DayPlan::Trade);
    assert!(
        matches!(again, Err(TradingError::Halted(TeamState::DailyLimitHalt))),
        "그날 다시 운용을 시작할 수 없다: {again:?}"
    );

    bench.set(kst(tuesday(), 9, 1));
    wait_for(
        "다음 거래일 시작에 하루 한도 멈춤이 풀린다",
        || bench.team() == TeamState::Resting,
    )
    .await;
    let last = bench.store().team_history().unwrap().pop().unwrap();
    assert_eq!(
        (last.from, last.cause),
        (TeamState::DailyLimitHalt, TeamCause::NextTradingDayStart)
    );

    bench.trade_today();
    let at = kst(tuesday(), 9, 30);
    bench.quote(NAVER, 100_000, at).await;
    let trade = bench.judged(NAVER);
    let placed = bench.place(buy(trade, NAVER, at)).await;
    assert!(
        matches!(placed, Placed::Ordered(_)),
        "다음 거래일에는 다시 판단해 산다: {placed:?}"
    );
    bench.close().await;
}

// 검증: V9 E24 C8 C16 T13
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 최대_손실에_닿으면_전체_정지가_되고_내가_다시_시작하기_전에는_돌지_않는다() {
    let dir = tempfile::tempdir().unwrap();
    register_keys(dir.path()).await;
    let bench = Bench::open(dir, kst(monday(), 10, 0), true).await;
    let addr = bench.addr();
    let store = bench.store();
    swing_with(
        addr,
        json!({"daily_loss_limit_pct": "0.5", "max_drawdown_pct": "1"}),
    )
    .await;
    bench.trade_today();
    bench.hold(HYNIX).await;

    // 5주 × 22,000원 하락 = 1.1% 손실: 하루 한도와 최대 낙폭에 함께 닿으면 전체 정지가 이긴다.
    bench.quote(HYNIX, 78_000, kst(monday(), 10, 5)).await;
    assert_eq!(bench.team(), TeamState::FullStop);
    let last = store.team_history().unwrap().pop().unwrap();
    assert_eq!(
        (last.from, last.cause),
        (TeamState::Operating, TeamCause::MaxLoss)
    );

    bench.set(kst(tuesday(), 9, 1));
    wait_for("둘째 날 시작", || {
        on_date(&store, KIND_DAY_START, tuesday()) == 1
    })
    .await;
    assert_eq!(
        bench.team(),
        TeamState::FullStop,
        "다음 날이 돼도 풀리지 않는다"
    );
    let again = bench.trading().decide_day(DayPlan::Trade);
    assert!(
        matches!(again, Err(TradingError::Halted(TeamState::FullStop))),
        "팀장의 판단으로는 다시 돌지 않는다: {again:?}"
    );
    assert_eq!(bench.team(), TeamState::FullStop);

    let reply = post(addr, TEAM_RESTART_PATH, None).await;
    assert_eq!(
        reply.status, 200,
        "내가 다시 시작을 누르면 된다: {}",
        reply.body
    );
    assert_eq!(reply.body["state"], json!("resting"));
    assert_eq!(bench.team(), TeamState::Resting);
    let last = store.team_history().unwrap().pop().unwrap();
    assert_eq!(
        (last.from, last.to, last.cause, last.owner_approved),
        (
            TeamState::FullStop,
            TeamState::Resting,
            TeamCause::OwnerRestart,
            true
        )
    );
    bench.close().await;
}

// 검증: E13 T7 T13
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 증권사가_주문을_받지_않으면_이상_감지_멈춤이_되고_새로_사지_않는다() {
    let bench = Bench::demo(kst(monday(), 10, 0)).await;
    choose_swing(bench.addr()).await;
    bench.trade_today();
    let at = kst(monday(), 10, 1);
    bench.quote(HYNIX, 100_000, at).await;
    let trade = bench.judged(HYNIX);

    bench.trading().paper().set_fault(Some(Fault::RejectOrders));
    let result = bench.trading().place(buy(trade, HYNIX, at)).await;
    assert!(
        matches!(result, Err(TradingError::Broker(_))),
        "증권사가 받지 않으면 실패다: {result:?}"
    );
    assert_eq!(bench.team(), TeamState::AnomalyHalt);
    let last = bench.store().team_history().unwrap().pop().unwrap();
    assert_eq!(
        (last.from, last.cause),
        (TeamState::Operating, TeamCause::AnomalyDetected)
    );

    bench.trading().paper().set_fault(None);
    let other = bench.judged(HYNIX);
    let placed = bench.place(buy(other, HYNIX, at)).await;
    assert!(
        matches!(&placed, Placed::Blocked(b)
            if b.reason == Reason::TeamNotOperating(TeamStatus::AnomalyHalt)),
        "이상 감지 멈춤 중에는 사지 않는다: {placed:?}"
    );
    bench.close().await;
}
