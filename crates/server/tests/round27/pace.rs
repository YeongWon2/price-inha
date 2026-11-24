//! 3번: 시연의 속도. 실행 파일의 `--demo` 조립에서만 시연용 가짜 AI가 답마다 뜸(기본 2.5초, 조립 값
//! [`price_inha::RunAssembly::demo_ai_pause`])을 둔다. 시험은 뜸을 줄여 논의 글의 `office.talk`가 한꺼번에가 아니라
//! 차례로 간격을 두고 나오는 것을 본다. 시험이 쓰는 가짜 AI의 기본([`DemoAi::new`])은 뜸이 없다.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use pi_ai::{AiProvider, CompletionRequest, Message, Role as Speaker};
use price_inha::{DEMO_AI_PAUSE, DemoAi, KIND_OFFICE_TALK, TRADE_ROOM_PREFIX, run_assembly};
use serde_json::Value;

use crate::common::{Plan, STEP, WATCHED, events, kst, members, saturday, settings, wait_for};

/// 시험이 줄인 뜸.
const PAUSE: Duration = Duration::from_millis(150);

/// 가짜 AI에 보내는 하루 판단 요청 하나.
fn day_plan_request() -> CompletionRequest {
    CompletionRequest {
        model: "demo".to_string(),
        system: Some("담당 id: leader".to_string()),
        messages: vec![Message {
            role: Speaker::User,
            content: "[하루 판단] 2026-11-21 오늘 매매할지 쉴지 정한다.".to_string(),
        }],
        max_tokens: 64,
    }
}

// 검증: V54 R2-3 D14
#[test]
fn 시연_조립의_가짜_ai_뜸은_기본_2_5초이고_실제_조립에는_뜸이_없다() {
    assert_eq!(
        DEMO_AI_PAUSE,
        Duration::from_millis(2_500),
        "기본 뜸은 2.5초다"
    );
    let home = tempfile::tempdir().unwrap();
    let demo = run_assembly(&settings(home.path(), true), None);
    assert_eq!(
        demo.demo_ai_pause, DEMO_AI_PAUSE,
        "--demo 조립은 가짜 AI 답마다 2.5초 뜸을 둔다"
    );
    let live = run_assembly(&settings(home.path(), false), None);
    assert_eq!(
        live.demo_ai_pause,
        Duration::ZERO,
        "실제 조립에는 뜸이 없다"
    );
}

// 검증: V54 R2-3
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 기본_가짜_ai는_뜸이_없다() {
    let ai = DemoAi::new();
    assert_eq!(ai.pause(), Duration::ZERO, "기본 가짜 AI의 뜸은 0이다");
    let started = Instant::now();
    for _ in 0..5 {
        let reply = ai.complete(day_plan_request()).await.expect("답한다");
        assert!(!reply.text.is_empty());
    }
    assert!(
        started.elapsed() < DEMO_AI_PAUSE,
        "기본 가짜 AI는 기다리지 않고 곧바로 답한다: {:?}",
        started.elapsed()
    );
}

// 검증: V54 R2-3
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 뜸을_둔_가짜_ai는_답마다_그만큼_기다리고_답은_같다() {
    let paced = DemoAi::with_pause(PAUSE);
    assert_eq!(paced.pause(), PAUSE);
    let plain = DemoAi::new();
    for _ in 0..2 {
        let started = Instant::now();
        let reply = paced.complete(day_plan_request()).await.expect("답한다");
        assert!(
            started.elapsed() >= PAUSE,
            "답마다 뜸만큼 기다린다: {:?}",
            started.elapsed()
        );
        let same = plain.complete(day_plan_request()).await.expect("답한다");
        assert_eq!(reply.text, same.text, "뜸은 답을 바꾸지 않는다");
    }
}

// 검증: V54 R2-3 D14 V14
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 시연_조립의_뜸을_줄이면_논의_글의_office_talk가_차례로_간격을_두고_나온다() {
    let bench = Plan::demo()
        .pause(PAUSE)
        .start(kst(saturday(), 14, 0))
        .await;
    bench.ready().await;
    assert_eq!(
        bench.demo_ai().pause(),
        PAUSE,
        "조립 값의 뜸이 팀의 가짜 AI에 닿는다"
    );

    // office.talk가 나오는 때(벽시계)를 따로 적는다.
    let store = bench.store();
    let seen = events(&store, KIND_OFFICE_TALK).len();
    let stop = Arc::new(AtomicBool::new(false));
    let watcher = {
        let (store, stop) = (store.clone(), Arc::clone(&stop));
        tokio::spawn(async move {
            let mut arrived: Vec<(Value, Instant)> = Vec::new();
            let mut count = seen;
            while !stop.load(Ordering::SeqCst) {
                let all = events(&store, KIND_OFFICE_TALK);
                let now = Instant::now();
                arrived.extend(all.into_iter().skip(count).map(|talk| (talk, now)));
                count = seen + arrived.len();
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
            arrived
        })
    };

    let started = Instant::now();
    let discussed = bench
        .crew()
        .discuss(WATCHED, "뜸 시험")
        .await
        .expect("논의가 돈다");
    let took = started.elapsed();
    let room = format!("{TRADE_ROOM_PREFIX}{}", discussed.trade.0);
    let said = members(&store, &room).len();
    assert!(
        said >= 3,
        "담당 셋 이상(뉴스 공시 시세 · 넘겨받은 담당 · 팀장)이 말했다: {said}"
    );
    wait_for("담당 글마다 office.talk", || {
        events(&store, KIND_OFFICE_TALK)
            .iter()
            .filter(|talk| talk["room"] == room.as_str())
            .count()
            >= said
    })
    .await;
    stop.store(true, Ordering::SeqCst);
    let arrived: Vec<Instant> = tokio::time::timeout(STEP, watcher)
        .await
        .expect("시간 안에 멈춘다")
        .expect("지켜보기가 끝난다")
        .into_iter()
        .filter(|(talk, _)| talk["room"] == room.as_str())
        .map(|(_, at)| at)
        .collect();

    assert_eq!(arrived.len(), said, "담당 글 하나마다 office.talk 하나");
    assert!(
        took >= PAUSE * 2,
        "첫 담당 글과 팀장 판단 사이에 적어도 뜸 두 번(넘겨받은 담당 · 팀장): {took:?}"
    );
    let spread = arrived[arrived.len() - 1].duration_since(arrived[0]);
    assert!(
        spread >= PAUSE,
        "office.talk가 한꺼번에 나오지 않고 차례로 간격을 두고 나온다: 처음과 끝 사이 {spread:?}"
    );
    bench.close().await;
}
