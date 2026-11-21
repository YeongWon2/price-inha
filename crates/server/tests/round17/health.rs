//! 2번 이상 감지(결정 2026-11-21 8 · 13번, 상태표 · P11 · N6): 시세가 30초 넘게 끊기면 이상 감지 멈춤이 되고,
//! 모든 것이 정상인 채로 60초가 지나면 스스로 풀려 쉬는 중을 거쳐 그날 계획대로 간다. 시세를 받은 뒤에 보므로
//! 장 시작 순간에 낡은 시세로 멈추지 않는다.
//!
//! 시연이 아닌 모드로 띄운다: 지켜보는 종목의 시세는 로컬 가짜 시세 서버가 가짜 시계의 지금 시각으로 준다.

use pi_store::{TeamCause, TeamState};
use price_inha::{KIND_DAY_CLOSE, KIND_DAY_START};

use crate::common::{Bench, kst, monday, on_date, plus, settle, tuesday, wait_for};

/// 20초씩 나눠 `seconds`초 뒤로 간다(지켜보는 종목의 시세가 따라오게 걸음마다 쉰다).
async fn advance(bench: &Bench, seconds: i64) {
    let from = bench.now();
    let mut gone = 0;
    while gone < seconds {
        gone = (gone + 20).min(seconds);
        bench.set(plus(from, gone));
        settle().await;
    }
}

/// 시세를 끊고 31초 뒤로 가서 비정상을 만든 뒤 시세를 되살린다. 되살아난 시각(정상이 된 첫 시각)에 선다.
async fn outage(bench: &Bench) {
    bench.fail_quotes();
    settle().await;
    bench.set(plus(bench.now(), 31));
    settle().await;
    bench.resume_quotes().await;
}

// 검증: V11 N6 P11 E13
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 시세가_30초_넘게_끊기면_멈추고_정상_60초_뒤_스스로_풀려_그날_계획대로_간다() {
    let bench = Bench::live(kst(monday(), 10, 0)).await;
    bench.ready().await;
    settle().await;
    assert_eq!(
        bench.team(),
        TeamState::Operating,
        "시세가 오는 동안은 운용 중이다"
    );

    bench.fail_quotes();
    settle().await;
    bench.set(plus(bench.now(), 31));
    wait_for("이상 감지 멈춤", || {
        bench.team() == TeamState::AnomalyHalt
    })
    .await;
    let entered = bench.store().team_history().unwrap().pop().unwrap();
    assert_eq!(
        (entered.from, entered.cause),
        (TeamState::Operating, TeamCause::AnomalyDetected)
    );

    // 시세가 돌아왔다. 정상 59초로는 풀리지 않는다.
    bench.resume_quotes().await;
    advance(&bench, 59).await;
    assert_eq!(
        bench.team(),
        TeamState::AnomalyHalt,
        "정상 59초로는 풀리지 않는다"
    );

    // 그 사이 다시 비정상이 끼면 처음부터 다시 센다(처음 정상이 된 때부터는 60초가 훨씬 넘었다).
    outage(&bench).await;
    advance(&bench, 59).await;
    assert_eq!(
        bench.team(),
        TeamState::AnomalyHalt,
        "비정상이 한 번이라도 끼면 정상 시간을 다시 센다"
    );

    // 딱 60초면 사람이 누르지 않아도 풀리고, 오늘 매매하기로 했으니 다시 운용 중이다.
    advance(&bench, 1).await;
    wait_for("스스로 풀려 운용 중", || {
        bench.team() == TeamState::Operating
    })
    .await;
    let history = bench.store().team_history().unwrap();
    let cleared = history
        .iter()
        .rposition(|record| record.from == TeamState::AnomalyHalt)
        .expect("이상 감지 멈춤에서 나간 기록");
    assert_eq!(
        (
            history[cleared].to,
            history[cleared].cause,
            history[cleared].owner_approved
        ),
        (TeamState::Resting, TeamCause::AnomalyCleared, false),
        "이상이 풀리면 쉬는 중으로 옮긴다(사람의 승인이 아니다)"
    );
    assert_eq!(
        history.last().map(|record| (record.from, record.to)),
        Some((TeamState::Resting, TeamState::Operating)),
        "쉬는 중으로 옮긴 뒤 그날 계획(매매)을 적용한다"
    );
    bench.close().await;
}

// 검증: V5 N6 P11
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 장_시작_순간에_밤사이_낡은_시세로_멈추지_않는다() {
    let bench = Bench::live(kst(monday(), 15, 25)).await;
    bench.ready().await;
    settle().await;
    bench.set(kst(monday(), 15, 31));
    let store = bench.store();
    wait_for("월요일 마감", || {
        on_date(&store, KIND_DAY_CLOSE, monday()) == 1
    })
    .await;
    settle().await;

    // 밤을 건너 장 시작 순간으로 간다: 이 순간 가진 시세는 전날 것이다.
    bench.set(kst(tuesday(), 9, 0));
    wait_for("화요일 시작", || {
        on_date(&store, KIND_DAY_START, tuesday()) >= 1
    })
    .await;
    settle().await;
    advance(&bench, 60).await;

    let halted = store
        .team_history()
        .unwrap()
        .iter()
        .any(|record| record.cause == TeamCause::AnomalyDetected);
    assert!(!halted, "시세를 받은 뒤에 보므로 장 시작에 멈추지 않는다");
    assert_ne!(bench.team(), TeamState::AnomalyHalt);
    bench.close().await;
}
