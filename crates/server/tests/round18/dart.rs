//! 10b 보강 1번(검토 B-1): DART 공시 요청에 하루 상한을 두고, 주기 가져오기는 그 시장의 정규장과 그 앞뒤
//! 30분 안에서만 돈다(네이버도). 장 밖 · 밤 · 주말에 나온 공시는 다음 장 시작의 공시 보기가 맡는다.
//!
//! 가짜 시계를 움직여 하루를 넘긴다. 공시 목록 요청(`/api/list.json`)은 주기 가져오기 · 장 시작의 공시 보기 ·
//! 논의의 자료 받기를 모두 센다(지시서: 논의의 자료 받기도 같은 수에 든다).

use std::time::{Duration, Instant};

use chrono::{DateTime, NaiveDate, Utc};
use price_inha::TEMPLATE_PATH;
use serde_json::json;

use crate::common::{Bench, NEWS_PATH, Plan, STEP, kst, monday, plus, post, settle, tuesday};

/// 공시 목록 경로.
const LIST_PATH: &str = "/api/list.json";
/// 지켜볼 종목 30개(앞 둘은 고유번호를 아는 종목, 나머지는 가짜 서버가 공시가 없다고 답하는 종목).
const MANY: [&str; 30] = [
    "005930", "000660", "100010", "100020", "100030", "100040", "100050", "100060", "100070",
    "100080", "100090", "100100", "100110", "100120", "100130", "100140", "100150", "100160",
    "100170", "100180", "100190", "100200", "100210", "100220", "100230", "100240", "100250",
    "100260", "100270", "100280",
];
/// 시험의 DART 하루 상한(종목 30개를 몇 번만 물어도 닿는 값).
const DART_LIMIT: u32 = 100;

/// 2026-11-21 토요일.
fn saturday() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 11, 21).unwrap()
}

/// 2026-11-22 일요일.
fn sunday() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 11, 22).unwrap()
}

/// 스윙 템플릿을 고른다(공시를 구독한다). 오늘 매매하기는 고르지 않는다(장 밖에서도 고를 수 있게).
async fn choose_swing(bench: &Bench) {
    let reply = post(bench.addr(), TEMPLATE_PATH, &json!({"style": "swing"})).await;
    assert_eq!(reply.status, 200, "템플릿을 골라야 한다: {}", reply.body);
}

/// `to`까지 `step`초씩 간다(걸음마다 뒤에서 도는 일이 따라오게 잠깐 쉰다).
async fn walk(bench: &Bench, to: DateTime<Utc>, step: i64) {
    while bench.now() < to {
        let next = plus(bench.now(), step).min(to);
        bench.set(next);
        settle().await;
    }
}

// 검증: D9 T11 N8
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 종목_30개를_하루_넘게_지켜봐도_dart_요청은_하루_상한을_넘지_않는다() {
    let bench = Plan::live()
        .watching(&MANY)
        .polling(Duration::from_secs(1))
        .dart_limit(DART_LIMIT)
        .start(kst(monday(), 9, 5))
        .await;
    for symbol in MANY {
        bench.set_price(symbol, 50_000);
    }
    bench.ready().await;
    // 월요일 09:05부터 화요일 08:00까지(월요일 하루치) 30분씩 간다. 상한이 없으면 걸음마다 30종목을 물어
    // 1,000번을 넘는다.
    walk(&bench, kst(tuesday(), 8, 0), 30 * 60).await;
    let listed = bench.data.requests(LIST_PATH).await.len();
    assert!(listed > 0, "공시를 가져온다");
    assert!(
        listed <= DART_LIMIT as usize,
        "DART 요청은 하루 상한({DART_LIMIT})을 넘지 않는다: {listed}번"
    );
    bench.close().await;
}

// 검증: D9 T11 N8
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 장_밖의_밤에는_dart와_네이버를_주기적으로_부르지_않는다() {
    let bench = Plan::demo()
        .watching(&MANY[..2])
        .with_naver()
        .polling(Duration::from_secs(1))
        .start(kst(monday(), 20, 0))
        .await;
    choose_swing(&bench).await;
    // 월요일 20:00부터 화요일 08:20까지(정규장 앞 30분 전) 한 시간씩 간다.
    walk(&bench, kst(tuesday(), 8, 20), 3600).await;
    assert_eq!(
        bench.data.requests(LIST_PATH).await.len(),
        0,
        "장 밖(밤)에는 공시를 묻지 않는다"
    );
    assert_eq!(
        bench.data.requests(NEWS_PATH).await.len(),
        0,
        "장 밖(밤)에는 네이버 뉴스를 묻지 않는다"
    );

    // 장 시간이 되면 다시 가져온다(멈춘 것이 아니라 시간을 본 것이다).
    bench.set(kst(tuesday(), 9, 5));
    let deadline = Instant::now() + STEP;
    while bench.data.requests(LIST_PATH).await.is_empty() {
        assert!(Instant::now() < deadline, "장 시간에는 공시를 가져온다");
        bench.advance(20).await;
    }
    bench.close().await;
}

// 검증: D9 T11 N8
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 주말에는_dart와_네이버를_주기적으로_부르지_않는다() {
    let bench = Plan::demo()
        .watching(&MANY[..2])
        .with_naver()
        .polling(Duration::from_secs(1))
        .start(kst(saturday(), 9, 0))
        .await;
    choose_swing(&bench).await;
    // 토요일 09:00부터 일요일 18:00까지 한 시간씩 간다(정규장 시간대를 두 번 지나지만 휴장이다).
    walk(&bench, kst(sunday(), 18, 0), 3600).await;
    assert_eq!(
        bench.data.requests(LIST_PATH).await.len(),
        0,
        "주말에는 공시를 묻지 않는다"
    );
    assert_eq!(
        bench.data.requests(NEWS_PATH).await.len(),
        0,
        "주말에는 네이버 뉴스를 묻지 않는다"
    );
    bench.close().await;
}
