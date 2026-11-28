//! 발판 확인: 이 시험 묶음의 도우미가 지금 구현에서 선다(red 앞에 통과해야 한다).

use pi_store::TradeState;
use price_inha::{TRADE_BASIS_PATH, TRADE_PATH, TRADES_PATH};

use crate::common::{Bench, get};

// 검증: V83 C30
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 발판이_서고_매매와_방_로그_줄을_만들고_읽는다() {
    let bench = Bench::start().await;
    let trade = bench.discussing(1_000);
    bench.member_said(trade, 1_100);
    bench.watched(trade, "missing_data", 1_200);
    bench.failed(trade, "discussion_error", 1_300);
    bench.server_note(trade, 1_400);
    let judged = bench.discussing(2_000);
    bench.judge(judged, 2_100);
    assert_eq!(
        bench.store().get_trade(judged).unwrap().state,
        TradeState::Judged
    );
    let room = format!("trade-{}", trade.0);
    assert_eq!(bench.store().room_last_seq(&room).unwrap(), 4);

    let (status, list) = get(bench.addr(), TRADES_PATH).await;
    assert_eq!(status, 200, "{list}");
    assert_eq!(
        list["trades"].as_array().unwrap().len(),
        2,
        "팀이 스스로 매매를 만들지 않는다: {list}"
    );
    let one = TRADE_PATH.replace("{id}", &trade.0.to_string());
    let (status, body) = get(bench.addr(), &one).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["state"], "discussing");
    let basis = TRADE_BASIS_PATH.replace("{id}", &trade.0.to_string());
    let (status, body) = get(bench.addr(), &basis).await;
    assert_eq!(status, 200, "근거 경로가 열린다: {body}");
    bench.close().await;
}
