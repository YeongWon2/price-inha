//! 16차 2번: 이상 감지 멈춤이 풀린 뒤 장이 열려 있을 때만 운용 중으로 간다. 장 밖에서 풀리면 쉬는 중으로 두고,
//! 다음 장 시작에 하루 흐름이 운용 중으로 옮긴다(장 안에서 풀리는 경우는 round17 `health`가 본다).
//!
//! 시연이 아닌 모드로 띄운다: 지켜보는 종목의 시세는 로컬 가짜 시세 서버가 가짜 시계의 지금 시각으로 준다.
//! 시세가 끊기는 것은 그 종목을 가짜 시세 서버에서 빼서 만든다.

use std::time::Instant;

use pi_store::{TeamCause, TeamState};
use price_inha::KIND_DAY_CLOSE;

use crate::common::{
    Plan, STEP, WATCHED, WATCHED_PRICE, kst, monday, on_date, settle, tuesday, wait_for,
};

// 검증: V11 N6 P11 E13 C8
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 장_밖에서_이상이_풀리면_쉬는_중으로_두고_다음_장_시작에_운용_중이_된다() {
    let bench = Plan::live().start(kst(monday(), 15, 20)).await;
    bench.ready().await;
    settle().await;
    assert_eq!(
        bench.team(),
        TeamState::Operating,
        "장 안에서는 운용 중이다"
    );

    // 시세가 30초 넘게 끊겨 멈춘다(장 안).
    bench.drop_price(WATCHED);
    settle().await;
    bench.set(kst(monday(), 15, 20) + chrono::Duration::seconds(31));
    wait_for("이상 감지 멈춤", || {
        bench.team() == TeamState::AnomalyHalt
    })
    .await;

    // 멈춘 채로 장이 닫힌다. 장이 닫힌 뒤에 시세가 돌아온다.
    bench.set(kst(monday(), 15, 31));
    let store = bench.store();
    wait_for("월요일 마감", || {
        on_date(&store, KIND_DAY_CLOSE, monday()) == 1
    })
    .await;
    assert_eq!(
        bench.team(),
        TeamState::AnomalyHalt,
        "마감이 멈춤을 풀지 않는다"
    );
    bench.set_price(WATCHED, WATCHED_PRICE);
    settle().await;
    let deadline = Instant::now() + STEP;
    while bench.team() == TeamState::AnomalyHalt {
        assert!(Instant::now() < deadline, "정상 60초 뒤 스스로 풀려야 한다");
        bench.advance(20).await;
    }
    let cleared = bench
        .store()
        .team_history()
        .unwrap()
        .into_iter()
        .rfind(|record| record.from == TeamState::AnomalyHalt)
        .expect("이상 감지 멈춤에서 나간 기록");
    assert_eq!(
        (cleared.to, cleared.cause),
        (TeamState::Resting, TeamCause::AnomalyCleared)
    );
    // 장이 닫혀 있으니 그날 계획이 매매여도 운용 중으로 가지 않는다.
    assert_eq!(
        bench.team(),
        TeamState::Resting,
        "장 밖에서 풀리면 쉬는 중으로 둔다"
    );
    for _ in 0..6 {
        bench.advance(20).await;
    }
    assert_eq!(
        bench.team(),
        TeamState::Resting,
        "장이 열리기 전에는 운용 중이 되지 않는다"
    );

    // 다음 장 시작에 하루 흐름이 운용 중으로 옮긴다.
    bench.set(kst(tuesday(), 9, 0));
    let deadline = Instant::now() + STEP;
    while bench.team() != TeamState::Operating {
        assert!(
            Instant::now() < deadline,
            "다음 장 시작에 운용 중이 되어야 한다: {:?}",
            bench.team()
        );
        bench.advance(20).await;
    }
    bench.close().await;
}
