//! 21차 5번: 하루 이벤트는 한 번만. 이벤트(`day_start` · `day_close`)를 남긴 뒤 다음 단계가 실패해도 같은 날
//! 이벤트가 두 번 남지 않는다(이미 남았으면 남기지 않고 다음 단계만 다시 한다).
//!
//! 다음 단계의 실패는 저장소 파일에 그 종류의 쓰기를 막는 트리거를 달아 만든다(공개 길에는 실패를 내는 곳이 없다).
//! 트리거를 단 채 뒤에서 도는 하루 흐름이 여러 번 다시 해 볼 시간을 주고, 트리거를 뗀 뒤 다음 단계가 끝나기를
//! 조건으로 기다린다. 시계는 시험이 정한다.

use pi_store::{TeamCause, TeamChange, TeamState};
use price_inha::{KIND_DAY_CLOSE, KIND_DAY_START};

use crate::common::{
    Plan, WATCHED, WATCHED_PRICE, kst, monday, ms, on_date, plus, settle, sql, tuesday, wait_for,
};

/// 마감의 두 번째 단계(그날 마지막 시세 남기기)를 막는다.
const BLOCK_REFERENCE: &str = "CREATE TRIGGER block_reference BEFORE INSERT ON events \
    WHEN NEW.kind = 'reference_prices' BEGIN SELECT RAISE(ABORT, '시험: 저장 실패'); END;";

/// 시작의 두 번째 단계(하루 한도 멈춤을 쉬는 중으로 풀기)를 막는다.
const BLOCK_TEAM: &str = "CREATE TRIGGER block_team BEFORE INSERT ON team_history \
    BEGIN SELECT RAISE(ABORT, '시험: 저장 실패'); END;";

// 검증: V5
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 마감_단계가_실패한_뒤_다시_해도_day_close는_한_번만_남는다() {
    let bench = Plan::demo().start(kst(monday(), 15, 20)).await;
    let store = bench.store();
    wait_for("월요일 시작", || {
        on_date(&store, KIND_DAY_START, monday()) == 1
    })
    .await;
    // 장 안에서 본 시세가 있어야 마감이 그날 마지막 시세를 남긴다.
    bench
        .quote(WATCHED, WATCHED_PRICE, kst(monday(), 15, 21))
        .await;
    sql(bench.home(), BLOCK_REFERENCE);

    bench.set(kst(monday(), 15, 31));
    wait_for("월요일 마감 이벤트", || {
        on_date(&store, KIND_DAY_CLOSE, monday()) >= 1
    })
    .await;
    // 다음 단계가 실패하는 채로 하루 흐름이 몇 번 더 돌게 둔다.
    settle().await;
    settle().await;
    sql(bench.home(), "DROP TRIGGER block_reference;");

    wait_for(
        "그날 마지막 시세가 남는다(다음 단계가 끝났다)",
        || on_date(&store, "reference_prices", monday()) == 1,
    )
    .await;
    settle().await;
    assert_eq!(
        on_date(&store, KIND_DAY_CLOSE, monday()),
        1,
        "단계가 실패해 다시 해도 day_close는 한 번만 남는다"
    );
    bench.close().await;
}

// 검증: V5 V8
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 시작_단계가_실패한_뒤_다시_해도_day_start는_한_번만_남는다() {
    let bench = Plan::demo().start(kst(monday(), 10, 0)).await;
    let store = bench.store();
    wait_for("월요일 시작", || {
        on_date(&store, KIND_DAY_START, monday()) == 1
    })
    .await;
    // 하루 한도 멈춤으로 다음 날을 맞는다: 다음 거래일 시작이 쉬는 중으로 풀어야 한다.
    bench.trade_today();
    store
        .change_team_state(TeamChange {
            to: TeamState::DailyLimitHalt,
            at: ms(plus(bench.now(), 1)),
            cause: TeamCause::DailyLossLimit,
            detail: "시험: 하루 한도".to_string(),
            owner_approved: false,
        })
        .expect("운용 중에서 하루 한도 멈춤이 된다");
    sql(bench.home(), BLOCK_TEAM);

    bench.set(kst(tuesday(), 9, 1));
    wait_for("화요일 시작 이벤트", || {
        on_date(&store, KIND_DAY_START, tuesday()) >= 1
    })
    .await;
    settle().await;
    settle().await;
    sql(bench.home(), "DROP TRIGGER block_team;");

    wait_for(
        "다음 단계(쉬는 중으로 풀기)가 끝난다",
        || {
            store.team_history().unwrap().iter().any(|record| {
                record.from == TeamState::DailyLimitHalt
                    && record.cause == TeamCause::NextTradingDayStart
            })
        },
    )
    .await;
    settle().await;
    assert_eq!(
        on_date(&store, KIND_DAY_START, tuesday()),
        1,
        "단계가 실패해 다시 해도 day_start는 한 번만 남는다"
    );
    bench.close().await;
}
