//! 4번 · E56: 시연 모드는 모의계좌와 시연용 가짜 AI만 쓴다. 키가 모두 등록돼 있어도 "실제" 증권사 · AI · DART ·
//! 네이버 주소(로컬 가짜 서버)에 요청이 0건이다. 실제(시연 아님) 조립은 시연 달력을 쓰지 않는다: 증권사 달력이
//! 503이면 주말 낮이어도 첫 tick이 실패하고 하루를 시작하지 않는다(그 증권사 주소에는 요청이 간다).

use std::time::{Duration, Instant};

use price_inha::{KIND_DAY_START, KIND_OFFICE_TALK};

use crate::common::{
    Bench, OPEN_JOB, Outside, Plan, STEP, events, kst, on_date, saturday, stays_false, sunday,
    wait_for,
};
use crate::disclosure::until_trade_talk;

/// 증권사 주소 자리에 요청이 하나라도 올 때까지 기다린다(가장 길게 [`STEP`]).
async fn until_broker_asked(bench: &Bench) {
    let deadline = Instant::now() + STEP;
    while Outside::received(&bench.outside.broker).await == 0 {
        assert!(
            Instant::now() < deadline,
            "실제 모드의 시세 흐름은 증권사 주소에 묻는다"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

// 검증: V54 E56 R2-3
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 시연_조립은_키가_등록돼_있어도_실제_증권사_ai_dart_네이버로_요청하지_않는다() {
    let bench = Plan::demo()
        .every(Duration::from_secs(1))
        .start(kst(sunday(), 14, 0))
        .await;
    let store = bench.store();
    wait_for("하루 판단 논의", || {
        events(&store, KIND_OFFICE_TALK)
            .iter()
            .any(|talk| talk["topic"] == "day")
    })
    .await;
    bench.ready().await;
    // 하루 판단 · 시세 흐름 · 가짜 공시 · 종목 논의(자료 받기 포함)까지 한 바퀴 돈 뒤에 센다.
    until_trade_talk(&bench).await;
    for (name, count) in bench.outside.counts().await {
        assert_eq!(count, 0, "시연 모드는 실제 {name} 주소로 요청하지 않는다");
    }
    bench.close().await;
}

// 검증: V54 E56 D14
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 실제_조립은_시연_달력을_쓰지_않아_증권사_달력이_없으면_주말_낮에도_하루를_시작하지_않는다()
{
    let bench = Plan::live().start(kst(saturday(), 14, 0)).await;
    assert!(
        !bench.trigger_jobs().iter().any(|job| job == OPEN_JOB),
        "실제 모드는 증권사 달력만 쓴다: 달력이 없으면(503) 첫 tick이 성공하지 않는다"
    );
    until_broker_asked(&bench).await;
    let store = bench.store();
    stays_false("시연 달력으로 하루 시작", || {
        on_date(&store, KIND_DAY_START, saturday()) > 0
    })
    .await;
    assert!(
        !bench.trigger_jobs().iter().any(|job| job == OPEN_JOB),
        "증권사 달력이 없는 동안 깨우기 엔진은 장 시작 작업을 남기지 않는다"
    );
    assert!(
        events(&store, KIND_OFFICE_TALK).is_empty(),
        "하루 판단 논의도 열리지 않는다"
    );
    bench.close().await;
}
