//! 39차 시험 발판 고침: 발판이 시세를 받아 팀이 운용 중으로 남는지 본다(메신저와 무관 — 메신저 없이 같은 조립으로
//! 띄운다).

use std::time::Duration;

use pi_store::TeamState;

use crate::common::Bench;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 발판은_운용_중이_되고_그대로_남는다() {
    let bench = Bench::start_plain().await;
    bench.ready().await;
    let store = bench.store();
    assert_eq!(store.team_state().unwrap(), TeamState::Operating);
    tokio::time::sleep(Duration::from_secs(2)).await;
    assert_eq!(
        store.team_state().unwrap(),
        TeamState::Operating,
        "시세가 흐르므로 이상 감시가 팀을 멈추지 않는다"
    );
    bench.close().await;
}
