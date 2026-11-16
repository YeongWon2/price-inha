//! 다시 시작(C8 · E9 · C16)과 키를 넣기 전후(V31).

use pi_store::{TeamCause, TeamState, TradeState, UnixMillis};
use price_inha::{
    DayPlan, RESTART_KEYS_REQUIRED, RESTART_NOT_STOPPED, TEAM_FULL_STOP, TEAM_RESTART_PATH,
    TEAM_RESTING, TradingError,
};
use serde_json::json;

use crate::common::{Bench, HYNIX, buy, choose_swing, kst, monday, post, register_keys, team_view};

// 검증: C8 C16 E9
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 되살린_뒤_키를_다시_등록하기_전에는_다시_시작하지_못하고_등록하면_된다() {
    let bench = Bench::demo(kst(monday(), 10, 0)).await;
    let store = bench.store();
    store
        .mark_restored(UnixMillis(bench.now().timestamp_millis()))
        .unwrap();
    assert_eq!(
        bench.team(),
        TeamState::FullStop,
        "되살리면 멈춘 상태로 시작한다"
    );
    let records = store.team_history().unwrap().len();

    let reply = post(bench.addr(), TEAM_RESTART_PATH, None).await;
    assert_eq!(reply.status, 409);
    assert_eq!(reply.body["error"], json!(RESTART_KEYS_REQUIRED));
    let message = reply.body["message"].as_str().unwrap_or_default();
    assert!(
        message.contains("키"),
        "키를 먼저 등록하라는 안내가 보인다: {message}"
    );
    assert_eq!(bench.team(), TeamState::FullStop, "팀은 멈춘 상태로 남는다");
    assert_eq!(
        store.team_history().unwrap().len(),
        records,
        "기록도 더하지 않는다"
    );
    assert_eq!(
        team_view(bench.addr()).await["state"],
        json!(TEAM_FULL_STOP)
    );

    let dir = bench.close().await;
    register_keys(dir.path()).await;
    let bench = Bench::open(dir, kst(monday(), 10, 1), true).await;
    assert_eq!(bench.team(), TeamState::FullStop, "다시 켜도 멈춘 상태다");

    let reply = post(bench.addr(), TEAM_RESTART_PATH, None).await;
    assert_eq!(
        reply.status, 200,
        "키를 다시 등록하면 다시 시작한다: {}",
        reply.body
    );
    assert_eq!(reply.body["state"], json!(TEAM_RESTING));
    let last = bench.store().team_history().unwrap().pop().unwrap();
    assert_eq!(
        (last.to, last.cause, last.owner_approved),
        (TeamState::Resting, TeamCause::OwnerRestart, true)
    );
    assert_eq!(team_view(bench.addr()).await["state"], json!(TEAM_RESTING));
    bench.close().await;
}

// 검증: C8
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 멈춰_있지_않으면_다시_시작은_거부되고_상태는_그대로다() {
    let dir = tempfile::tempdir().unwrap();
    register_keys(dir.path()).await;
    let bench = Bench::open(dir, kst(monday(), 10, 0), true).await;

    let reply = post(bench.addr(), TEAM_RESTART_PATH, None).await;
    assert_eq!(reply.status, 409);
    assert_eq!(reply.body["error"], json!(RESTART_NOT_STOPPED));
    assert_eq!(bench.team(), TeamState::Resting);
    assert!(
        bench.store().team_history().unwrap().is_empty(),
        "기록이 없다"
    );
    bench.close().await;
}

// 검증: V31 T13
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 키를_넣기_전에는_운용을_시작하지_않고_넣은_뒤에는_시작한다() {
    let bench = Bench::open(tempfile::tempdir().unwrap(), kst(monday(), 10, 0), false).await;
    choose_swing(bench.addr()).await;

    let result = bench.trading().decide_day(DayPlan::Trade);
    assert!(
        matches!(result, Err(TradingError::SetupRequired)),
        "키가 없으면 운용하지 않는다: {result:?}"
    );
    assert_eq!(bench.team(), TeamState::Resting);
    let trade = bench.judged(HYNIX);
    let result = bench.trading().place(buy(trade, HYNIX, bench.now())).await;
    assert!(
        matches!(result, Err(TradingError::SetupRequired)),
        "키가 없으면 주문 안도 받지 않는다: {result:?}"
    );
    assert_eq!(bench.trade_state(trade), TradeState::Judged);
    assert!(bench.orders().await.is_empty());

    let dir = bench.close().await;
    register_keys(dir.path()).await;
    let bench = Bench::open(dir, kst(monday(), 10, 1), false).await;
    bench.trade_today();
    bench.close().await;
}
