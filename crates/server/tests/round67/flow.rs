//! (결정 36) `price-inha demo`의 짧은 하루가 깨우기 엔진에 닿는다: 켠 뒤 1분이면 그날 장 시작 회차가 돌고(하루 판단),
//! 16분 반이면 장 마감 뒤 회차(복기 · 하루 정리)가 돈다. 시험 시계로 실제 시각만 옮긴다.

use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use pi_market::Clock;
use price_inha::{App, DEMO_DAY_NOTICE, DEMO_HOME_FILE, DemoPlan, DemoRequest, start_demo_plan};

use crate::clock::{TestClock, kst, plus, tuesday};
use crate::common::{Outside, Recorder, STEP, assembly, demo_run_settings};

/// 그 작업의 마지막 회차(밀리초). 없으면 `None`.
fn last_slot(app: &App, job: &str) -> Option<i64> {
    app.store()
        .trigger_jobs()
        .unwrap()
        .into_iter()
        .find(|row| row.job == job)
        .and_then(|row| row.state["last_slot"].as_i64())
}

/// `done`이 참이 될 때까지 기다린다. 기다리는 동안 시험 시계를 `from`부터 조금씩(20ms마다 250ms) 흘린다 —
/// 깨우기 엔진은 끝난 깨우기를 다음 tick(체험 시각으로 60초 안)에 적으므로 시각이 멈춰 있으면 적히지 않는다.
async fn wait_for(
    what: &str,
    clock: &TestClock,
    from: chrono::DateTime<chrono::Utc>,
    mut done: impl FnMut() -> bool,
) {
    let started = Instant::now();
    let mut at = from;
    while !done() {
        assert!(started.elapsed() < STEP, "시간 안에 일어나야 한다: {what}");
        clock.set(at);
        at += chrono::Duration::milliseconds(250);
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
}

async fn start(home: &Path, clock: &Arc<TestClock>) -> (App, String) {
    let outside = Outside::start().await;
    let settings = demo_run_settings(home);
    let mut built = assembly(&settings, &outside);
    built.wiring.clock = Arc::clone(clock) as Arc<dyn Clock>;
    let plan = DemoPlan {
        request: DemoRequest { no_open: true },
        day_minutes: 20,
    };
    let mut printed = Vec::new();
    let app = tokio::time::timeout(
        STEP,
        start_demo_plan(settings, built, &plan, &mut printed, &Recorder::default()),
    )
    .await
    .expect("시간 안에 떠야 한다")
    .expect("체험 서버가 떠야 한다");
    (app, String::from_utf8(printed).unwrap())
}

// 검증: V54, C33, D6
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 짧은_하루에서_장_시작과_장_마감_뒤_회차가_20분_안에_돈다() {
    let dir = tempfile::tempdir().unwrap();
    let real = kst(tuesday(), 10, 0, 0);
    let clock = TestClock::at(real);
    let (app, printed) = start(dir.path(), &clock).await;
    assert!(
        printed.contains(DEMO_DAY_NOTICE),
        "하루 길이를 알린다: {printed}"
    );
    assert!(dir.path().join(DEMO_HOME_FILE).is_file(), "체험 폴더 표시");

    // 켠 뒤 1분 10초: 체험 시각 00:05:10 — 그날 장 시작 회차(00:05)가 돈다.
    let open = kst(tuesday(), 0, 5, 0).timestamp_millis();
    wait_for("장 시작 회차", &clock, plus(real, 70), || {
        last_slot(&app, "team-open") == Some(open)
    })
    .await;

    // 켠 뒤 16분 40초: 체험 시각 23:55:40 — 장 마감 뒤 회차(23:55 + 30초)가 돈다.
    let after = kst(tuesday(), 23, 55, 30).timestamp_millis();
    wait_for(
        "장 마감 뒤 회차",
        &clock,
        plus(real, 16 * 60 + 40),
        || last_slot(&app, "team-after-close") == Some(after),
    )
    .await;
    app.shutdown().await.unwrap();
}
