//! 사는 주문의 수량: 규칙 엔진이 정한 수량을 매매에 남기고(점검 표시는 그 매매에 묶는다),
//! 매매 담당이 적은 수량이 더 작으면 작은 쪽으로 산다(P7).

use price_inha::{DemoTurn, Placed};
use serde_json::json;

use crate::common::{Bench, HYNIX, kst, monday};

// 검증: P7 E33 C10
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 산_주문의_수량이_그_매매에_남는다() {
    let bench = Bench::start(kst(monday(), 10, 0)).await;
    bench.ready().await;
    let discussed = bench.discuss_buy(HYNIX).await;
    assert!(
        matches!(discussed.placed, Some(Placed::Ordered(_))),
        "산다: {:?}",
        discussed.placed
    );
    let quantity = bench.store().trade_quantity(discussed.trade).unwrap();
    assert!(
        quantity.is_some_and(|q| q > 0),
        "산 수량이 매매에 남는다: {quantity:?}"
    );
    bench.close().await;
}

// 검증: P7 C10
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 매매_담당이_적은_수량이_작으면_그_수량으로_산다() {
    let bench = Bench::start(kst(monday(), 10, 0)).await;
    bench.ready().await;
    let draft = json!({"side": "buy", "note": "한 주만 산다", "quantity": 1});
    bench.script(DemoTurn::Draft, &draft.to_string());
    let discussed = bench.discuss_buy(HYNIX).await;
    assert!(
        matches!(discussed.placed, Some(Placed::Ordered(_))),
        "산다: {:?}",
        discussed.placed
    );
    let quantity = bench.store().trade_quantity(discussed.trade).unwrap();
    assert_eq!(quantity, Some(1), "작은 쪽(매매 담당의 1주)으로 산다");
    bench.close().await;
}
