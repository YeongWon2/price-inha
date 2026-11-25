//! 발판 확인: 이 시험 묶음의 도우미가 지금 구현에서 선다(red 앞에 통과해야 한다).

use pi_store::{TradeSide, TradeState};
use price_inha::{APPROVAL_PATH, TRADE_BASIS_PATH};

use crate::common::{Bench, HYNIX, NAVER, get, kst, monday};

// 검증: C23 C24
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 발판이_서고_판단된_매매와_승인_요청을_만든다() {
    let bench = Bench::start(kst(monday(), 10, 0)).await;
    bench.ready_for_approval().await;
    let trade = bench.judged_with_opinions(HYNIX, TradeSide::Buy);
    let plain = bench.judged_plain(NAVER);
    assert_eq!(bench.trade_state(trade), TradeState::Judged);
    assert_eq!(bench.trade_state(plain), TradeState::Judged);
    let opinions = bench.store().trade_opinions(trade).unwrap();
    assert_eq!(opinions.opinions.len(), 4, "의견 넷이 남는다");
    assert!(opinions.summary.is_some(), "팀장 요약이 남는다");

    for id in [trade, plain] {
        let path = TRADE_BASIS_PATH.replace("{id}", &id.0.to_string());
        let reply = get(bench.addr(), &path).await;
        assert_eq!(reply.status, 200, "{}", reply.body);
        assert_eq!(reply.body["id"], id.0, "{}", reply.body);
        assert!(
            reply.body["summary"]
                .as_str()
                .is_some_and(|s| !s.is_empty())
        );
    }

    let (number, fingerprint) = bench.awaiting(trade, "15").await;
    assert_eq!(bench.trade_state(trade), TradeState::AwaitingApproval);
    let path = APPROVAL_PATH.replace("{id}", number.as_str());
    let reply = get(bench.addr(), &path).await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert_eq!(reply.body["fingerprint"], fingerprint.as_str());
    assert_eq!(reply.body["status"], "pending");
    bench.close().await;
}
