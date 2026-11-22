//! 10b 보강 2 1번(F4 · E30 · E34): 손절 · 청산의 물러서기가 손절을 늦추지 않는다.
//!
//! - (가) 장 밖(`MarketClosed`) · 달력 없음으로 막힌 손절 대기는 다음 세션 시작 시각보다 늦게 다시 보지 않는다.
//! - (나) 그 밖의 까닭(낡은 시세 등)으로 막혀도 손절 · 청산의 물러서기 상한은 5초다(익절 · 팀장 팔기는 10분).
//! - (다) 세션이 시작되면 손절 · 청산의 물러서기 기억을 비운다.
//!
//! 모의계좌는 주문 시각 뒤에 받은 시세로만 체결하므로, 시계를 1초씩 앞으로 옮기며 그 시각의 시세를 넣는다.
//!
//! (나)의 "장중에 낡은 시세로 막혔다가 새 시세가 오면 5초 안에" 시험은 쓰지 않았다: 지금 규칙에서 낡은 시세(P11)는
//! 사는 주문만 막고, 손절은 시장가라 가격제한도 보지 않으며, 모의계좌의 API 한도는 없음으로 고정이라 장중에
//! 손절 다시 내기를 공개 길로 막을 까닭이 없다(조정자에게 보고).

use std::time::{Duration, Instant};

use pi_paper::Fault;
use pi_store::TradeState;
use price_inha::KIND_DAY_START;
use serde_json::json;

use crate::common::{
    Bench, HYNIX, Plan, STEP, STOP, kst, monday, on_date, plus, settle, tuesday, wait_for,
};

/// 장 시작 뒤 손절이 팔려야 하는 가짜 시계의 한도(초). 물러서기 상한 10분보다 훨씬 짧다.
const OPEN_LIMIT_SECONDS: i64 = 30;
/// 손절값 아래의 시세.
const BELOW_STOP: i64 = STOP - 1_000;

/// 모의계좌가 주문을 받지 않는 동안 손절값에 닿게 해 손절이 다시 내기를 기다리게 한다.
async fn failed_stop(bench: &Bench, at: chrono::DateTime<chrono::Utc>) -> pi_store::TradeId {
    let held = bench.holding(HYNIX).await;
    bench.trading().paper().set_fault(Some(Fault::RejectOrders));
    bench.quote(HYNIX, BELOW_STOP, at).await;
    let pending = bench.pending_sells(held);
    assert_eq!(pending.len(), 1, "실패한 손절이 기다린다: {pending:?}");
    assert_eq!(pending[0]["purpose"], json!("stop_loss"));
    held
}

/// 시세를 1초씩 넣으며 팔릴 때까지 간다. 가짜 시계가 `limit`을 넘으면 실패한다.
async fn sold_by(bench: &Bench, held: pi_store::TradeId, limit: chrono::DateTime<chrono::Utc>) {
    let deadline = Instant::now() + STEP;
    // 복기(Reviewed)는 팔린 뒤의 상태다: 밤을 넘기면 놓친 장 마감 복기가 장 시작 직후 돌아 곧바로 옮길 수 있다.
    while !matches!(
        bench.trade_state(held),
        TradeState::Sold | TradeState::Reviewed
    ) {
        assert!(
            bench.now() <= limit,
            "손절이 {limit} 안에 팔려야 한다(물러서기가 손절을 늦추면 안 된다): 지금 {}",
            bench.now()
        );
        assert!(Instant::now() < deadline, "시간 안에 팔리지 않았다");
        bench.quote(HYNIX, BELOW_STOP, plus(bench.now(), 1)).await;
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

// 검증: F4 E30 E34
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 마감_직전_실패한_손절은_밤을_넘겨도_다음_장_시작_몇_초_안에_다시_나가_팔린다() {
    let bench = Plan::demo().start(kst(monday(), 15, 10)).await;
    bench.ready().await;
    let held = failed_stop(&bench, kst(monday(), 15, 29)).await;

    // 장이 닫혀 다시 내기는 장 밖으로 막힌다. 계좌는 돌아왔다. 막힌 채 20분을 걸어 물러서기가 상한까지 큰다.
    bench.set(kst(monday(), 15, 31));
    bench.trading().paper().set_fault(None);
    bench.advance(20 * 60).await;
    assert!(
        bench.sells(HYNIX).await.is_empty(),
        "장 밖에서는 나가지 않는다"
    );

    // 밤을 넘겨 다음 장 시작 직전까지 간다(시작 직전에도 막힌 대기를 다시 본다).
    bench.set(kst(tuesday(), 8, 59));
    settle().await;
    bench.advance(50).await;
    assert!(
        bench.sells(HYNIX).await.is_empty(),
        "장 시작 전에는 나가지 않는다"
    );

    // 장이 열린다. 몇 초 안에 새 시세가 오면 손절이 다시 나가 팔린다(10분 물러서기를 기다리지 않는다).
    let open = kst(tuesday(), 9, 0);
    bench.set(open);
    let store = bench.store();
    wait_for("화요일 시작", || {
        on_date(&store, KIND_DAY_START, tuesday()) >= 1
    })
    .await;
    sold_by(&bench, held, plus(open, OPEN_LIMIT_SECONDS)).await;

    assert!(
        bench.pending_sells(held).is_empty(),
        "다시 낸 뒤에는 기다리는 것이 없다"
    );
    assert_eq!(
        bench.sells(HYNIX).await.len(),
        1,
        "파는 주문은 한 번 나갔다"
    );
    bench.close().await;
}
