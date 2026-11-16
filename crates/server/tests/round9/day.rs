//! 하루 시작 · 마감(V5)과 쉬는 날(E19 · V7).

use std::time::Duration;

use pi_paper::Side;
use pi_rules::Purpose;
use pi_store::{TeamCause, TeamState, TradeState};
use price_inha::{DayPlan, KIND_DAY_CLOSE, KIND_DAY_PLAN, KIND_DAY_START, Placed, TEAM_RESTING};
use serde_json::json;

use crate::common::{
    Bench, HYNIX, NAVER, buy, choose_swing, events, kst, monday, on_date, sell, team_view, tuesday,
    wait_for,
};

// 검증: V5 T13
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 아무것도_누르지_않아도_하루_시작과_마감이_그_날에_한_번씩_남는다() {
    let bench = Bench::demo(kst(monday(), 8, 50)).await;
    let store = bench.store();
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(
        on_date(&store, KIND_DAY_START, monday()),
        0,
        "장 시작 전에는 하루 시작이 없다"
    );
    assert_eq!(
        bench.team(),
        TeamState::Resting,
        "새 저장소의 팀은 쉬는 중이다"
    );

    bench.set(kst(monday(), 9, 1));
    wait_for("장 시작에 닿으면 하루 시작이 남는다", || {
        on_date(&store, KIND_DAY_START, monday()) == 1
    })
    .await;

    bench.trade_today();
    let plans = events(&store, KIND_DAY_PLAN);
    let plan = plans.last().expect("오늘 판단이 기록에 남는다");
    assert_eq!(plan["date"], json!("2026-11-16"));
    assert_eq!(plan["plan"], json!("trade"));

    bench.set(kst(monday(), 15, 31));
    wait_for("장 마감에 닿으면 하루 마감이 남는다", || {
        on_date(&store, KIND_DAY_CLOSE, monday()) == 1
    })
    .await;
    wait_for("마감 뒤 팀은 쉬는 중이다", || {
        bench.team() == TeamState::Resting
    })
    .await;
    let last = store.team_history().unwrap().pop().unwrap();
    assert_eq!(
        (last.from, last.to, last.cause),
        (
            TeamState::Operating,
            TeamState::Resting,
            TeamCause::MarketClose
        ),
        "운용 중에서 장 마감 사유로 쉬는 중이 된다"
    );

    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(
        on_date(&store, KIND_DAY_START, monday()),
        1,
        "하루 시작은 한 번만"
    );
    assert_eq!(
        on_date(&store, KIND_DAY_CLOSE, monday()),
        1,
        "하루 마감은 한 번만"
    );
    bench.close().await;
}

// 검증: E19 V7 T13
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 쉬는_날에는_이유가_보이고_새로_사지_않고_손절과_청산만_한다() {
    let bench = Bench::demo(kst(monday(), 10, 0)).await;
    let addr = bench.addr();
    let store = bench.store();
    choose_swing(addr).await;
    bench.trade_today();
    let hynix = bench.hold(HYNIX).await;
    let naver = bench.hold(NAVER).await;

    bench.set(kst(monday(), 15, 40));
    wait_for("마감 뒤 쉬는 중", || {
        bench.team() == TeamState::Resting
    })
    .await;
    bench.set(kst(tuesday(), 9, 30));
    wait_for("둘째 날 시작", || {
        on_date(&store, KIND_DAY_START, tuesday()) == 1
    })
    .await;

    let reason = "변동성이 커서 오늘은 쉰다";
    let state = bench
        .trading()
        .decide_day(DayPlan::Rest {
            reason: reason.to_string(),
        })
        .unwrap();
    assert_eq!(state, TeamState::Resting);
    let view = team_view(addr).await;
    assert_eq!(view["state"], json!(TEAM_RESTING));
    assert_eq!(view["plan"], json!("rest"));
    assert_eq!(view["reason"], json!(reason), "쉬는 이유가 화면에 보인다");

    let at = kst(tuesday(), 10, 0);
    bench.quote(HYNIX, 100_000, at).await;
    let fresh = bench.judged(HYNIX);
    let placed = bench.place(buy(fresh, HYNIX, at)).await;
    assert_eq!(
        placed,
        Placed::RestDay(reason.to_string()),
        "쉬는 날에는 사지 않는다"
    );
    assert_eq!(bench.trade_state(fresh), TradeState::Judged);

    bench.quote(NAVER, 104_000, at).await;
    let placed = bench
        .place(sell(naver, NAVER, Purpose::TakeProfit, 104_000, at))
        .await;
    assert_eq!(
        placed,
        Placed::RestDay(reason.to_string()),
        "익절도 하지 않는다"
    );
    assert_eq!(bench.trade_state(naver), TradeState::Holding);

    let later = at + chrono::Duration::seconds(1);
    bench.quote(HYNIX, 94_000, later).await;
    let placed = bench
        .place(sell(hynix, HYNIX, Purpose::StopLoss, 94_000, later))
        .await;
    assert!(
        matches!(placed, Placed::Ordered(_)),
        "손절은 한다: {placed:?}"
    );
    let placed = bench
        .place(sell(naver, NAVER, Purpose::Liquidation, 104_000, later))
        .await;
    assert!(
        matches!(placed, Placed::Ordered(_)),
        "청산은 한다: {placed:?}"
    );

    let orders = bench.orders().await;
    let buys = orders.iter().filter(|o| o.side == Side::Buy).count();
    let sells = orders.iter().filter(|o| o.side == Side::Sell).count();
    assert_eq!(buys, 2, "사는 주문은 첫날 것뿐이다(쉬는 날 0건)");
    assert_eq!(sells, 2, "손절과 청산만 나갔다");
    bench.close().await;
}
