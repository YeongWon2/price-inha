//! 10b 보강 3번(검토 B-3, 8번 그대로, 결정 13): 주기 가져오기가 장 시간 안에서 하루 한 번(장 시작 뒤) 재무(지켜보는
//! 종목) · 거시(ECOS 금리 · 환율, FRED 금리)를 받아 지난번 값과 다르면 자료 사건으로 넘긴다. 종목이 없는 거시
//! 사건은 뉴스 공시 시세 담당(scout) 턴 하나로 "출처 · 이전 → 지금"을 알리고, 담당이 답에 지켜보는 종목을 고르면
//! 그 종목들(많아야 3개)을 논의한다. 고르지 않으면(`(pass)`) 논의하지 않는다.
//!
//! 시연이 아닌 모드로 띄운다(가짜 시세 · AI · 자료 서버). 월요일 장 시작 뒤에 처음 받은 값이 "지난번 값"이 되고,
//! 화요일 장 시작 뒤에 다시 받은 값과 비교한다. 10b 보강 6번에 따라 깨우기 글에는 출처와 "바깥 글" 표시가 있다.

use std::time::Instant;

use price_inha::{KIND_DAY_CLOSE, KIND_DAY_START};

use crate::common::{
    Bench, FRED_PATH, HYNIX, Plan, STEP, WATCHED, WATCHED_PRICE, kst, monday, on_date, settle,
    tuesday, wait_for,
};

/// ECOS 한국은행 기준금리 통계표(경로에 든다).
const KOREA_RATE: &str = "/722Y001/";
/// DART 재무 경로.
const FINANCIALS_PATH: &str = "/api/fnlttSinglAcnt.json";
/// 바뀐 FRED 값(처음 `4.10`).
const FRED_AFTER: &str = "4.35";
/// 바뀐 ECOS 기준금리 값(처음 `2.25`).
const RATE_AFTER: &str = "2.75";
/// scout가 고를 종목 다섯(지켜보는 종목).
const PICKS: [&str; 5] = [WATCHED, HYNIX, "100010", "100020", "100030"];
/// 외부에서 온 글이라는 표시(지시서 글자 그대로).
const OUTSIDE: &str = "바깥 글";

/// 자료 가짜 서버가 받은, 경로에 `part`가 든 요청 수.
async fn fetched(bench: &Bench, part: &str) -> usize {
    bench.data.requests_like(part).await.len()
}

/// 20초씩 가며 경로에 `part`가 든 요청이 `from`보다 많아질 때까지 기다린다.
async fn wait_fetch(bench: &Bench, part: &str, from: usize, what: &str) {
    let deadline = Instant::now() + STEP;
    while fetched(bench, part).await <= from {
        assert!(Instant::now() < deadline, "시간 안에 되지 않았다: {what}");
        bench.advance(20).await;
    }
}

/// 20초씩 가며 scout 턴 가운데 `needle`이 든 것이 `from`보다 많아질 때까지 기다린다.
async fn wait_scout(bench: &Bench, needle: &str, from: usize, what: &str) {
    let deadline = Instant::now() + STEP;
    while bench.scout_inputs(needle).await.len() <= from {
        assert!(Instant::now() < deadline, "시간 안에 되지 않았다: {what}");
        bench.advance(20).await;
    }
}

/// 월요일 장 중에 띄워 스윙 템플릿을 고르고, 그날 처음 받은 값(`part`)이 올 때까지 간다.
async fn monday_baseline(plan: Plan, part: &str) -> Bench {
    let bench = plan.start(kst(monday(), 9, 5)).await;
    bench.ready().await;
    wait_fetch(&bench, part, 0, "월요일 장 시작 뒤 처음 받음").await;
    settle().await;
    bench.advance(60).await;
    bench
}

/// 월요일 장을 마감하고 화요일 장을 연 뒤, 화요일에 다시 받은 값(`part`)이 올 때까지 간다.
async fn tuesday_fetch(bench: &Bench, part: &str) {
    let before = fetched(bench, part).await;
    settle().await;
    bench.set(kst(monday(), 15, 31));
    let store = bench.store();
    wait_for("월요일 마감", || {
        on_date(&store, KIND_DAY_CLOSE, monday()) == 1
    })
    .await;
    settle().await;
    bench.set(kst(tuesday(), 9, 1));
    wait_for("화요일 시작", || {
        on_date(&store, KIND_DAY_START, tuesday()) >= 1
    })
    .await;
    wait_fetch(bench, part, before, "화요일 장 시작 뒤 다시 받음").await;
}

// 검증: T11 F2 V34 D9
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn fred와_ecos_값이_바뀌면_scout가_깨어나고_출처와_이전_지금_값이_실린다() {
    let plan = Plan::live().watching(&[WATCHED]).with_fred();
    let bench = monday_baseline(plan, FRED_PATH).await;
    assert!(
        fetched(&bench, KOREA_RATE).await > 0,
        "ECOS 기준금리도 받는다"
    );
    let fred_before = bench.scout_inputs(FRED_AFTER).await.len();
    let rate_before = bench.scout_inputs(RATE_AFTER).await.len();

    bench.data.set_fred(FRED_AFTER);
    bench.data.set_korea_rate(RATE_AFTER);
    tuesday_fetch(&bench, FRED_PATH).await;
    wait_scout(&bench, FRED_AFTER, fred_before, "FRED 값이 바뀌어 깨어남").await;
    wait_scout(&bench, RATE_AFTER, rate_before, "ECOS 값이 바뀌어 깨어남").await;

    let fred = bench.scout_inputs(FRED_AFTER).await;
    assert!(
        fred.iter().any(|input| {
            let lower = input.to_lowercase();
            lower.contains("fred") && input.contains("4.1") && input.contains(OUTSIDE)
        }),
        "깨우기 글에 출처(fred) · 이전 값 · 바깥 글 표시가 있다: {fred:?}"
    );
    let rate = bench.scout_inputs(RATE_AFTER).await;
    assert!(
        rate.iter().any(|input| {
            let lower = input.to_lowercase();
            lower.contains("ecos") && input.contains("2.25") && input.contains(OUTSIDE)
        }),
        "깨우기 글에 출처(ecos) · 이전 값 · 바깥 글 표시가 있다: {rate:?}"
    );
    bench.close().await;
}

// 검증: T11 F2 V34
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn fred와_ecos_값이_같으면_scout를_깨우지_않는다() {
    let plan = Plan::live().watching(&[WATCHED]).with_fred();
    let bench = monday_baseline(plan, FRED_PATH).await;
    let fred_before = bench.scout_inputs("fred").await.len();
    let ecos_before = bench.scout_inputs("ecos").await.len();

    tuesday_fetch(&bench, FRED_PATH).await;
    settle().await;
    bench.advance(60).await;
    settle().await;
    assert_eq!(
        bench.scout_inputs("fred").await.len(),
        fred_before,
        "FRED 값이 같으면 깨우지 않는다"
    );
    assert_eq!(
        bench.scout_inputs("ecos").await.len(),
        ecos_before,
        "ECOS 값이 같으면 깨우지 않는다"
    );
    bench.close().await;
}

// 검증: T11 F2 D9
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 재무_값이_바뀌면_그_종목으로_scout가_깨어난다() {
    let plan = Plan::live().watching(&[WATCHED]);
    let bench = monday_baseline(plan, FINANCIALS_PATH).await;
    // 화요일 시세는 월요일 마지막 시세와 같다(시세 조건으로는 깨우지 않는다).
    bench.set_price(WATCHED, WATCHED_PRICE);
    let before = bench.scout_calls(WATCHED).await;

    bench.data.set_revenue("310,000,000,000,000");
    tuesday_fetch(&bench, FINANCIALS_PATH).await;
    let deadline = Instant::now() + STEP;
    while bench.scout_calls(WATCHED).await <= before {
        assert!(
            Instant::now() < deadline,
            "재무 값이 바뀌어 그 종목으로 깨어나야 한다"
        );
        bench.advance(20).await;
    }
    bench.close().await;
}

/// [`PICKS`]를 지켜보며(시세는 모두 있다) 월요일 FRED 값을 받아 둔다.
async fn picks_baseline() -> Bench {
    let plan = Plan::live().watching(&PICKS).with_fred();
    let bench = plan.start(kst(monday(), 9, 5)).await;
    for symbol in PICKS {
        bench.set_price(symbol, WATCHED_PRICE);
    }
    bench.ready().await;
    wait_fetch(&bench, FRED_PATH, 0, "월요일 장 시작 뒤 처음 받음").await;
    settle().await;
    bench.advance(60).await;
    bench
}

// 검증: T11 F2 E16
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 거시_깨우기에서_scout가_종목을_고르면_그_종목들을_많아야_세_개_논의한다() {
    let bench = picks_baseline().await;
    assert!(
        bench.discussed_symbols().await.is_empty(),
        "아직 논의한 종목이 없다"
    );
    bench.scout_says(&format!("살펴볼 종목: {}", PICKS.join(", ")));
    let fred_before = bench.scout_inputs(FRED_AFTER).await.len();

    bench.data.set_fred(FRED_AFTER);
    tuesday_fetch(&bench, FRED_PATH).await;
    wait_scout(&bench, FRED_AFTER, fred_before, "FRED 값이 바뀌어 깨어남").await;
    let deadline = Instant::now() + STEP;
    while bench.discussed_symbols().await.is_empty() {
        assert!(Instant::now() < deadline, "고른 종목을 논의해야 한다");
        bench.advance(20).await;
    }
    settle().await;
    bench.advance(60).await;
    settle().await;
    let discussed = bench.discussed_symbols().await;
    assert!(
        discussed.len() <= 3,
        "논의하는 종목은 많아야 3개다: {discussed:?}"
    );
    assert!(
        discussed
            .iter()
            .all(|symbol| PICKS.contains(&symbol.as_str())),
        "고른 종목만 논의한다: {discussed:?}"
    );
    bench.close().await;
}

// 검증: T11 F2 E16
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 거시_깨우기에서_scout가_pass하면_논의하지_않는다() {
    let bench = picks_baseline().await;
    bench.scout_says("(pass)");
    let fred_before = bench.scout_inputs(FRED_AFTER).await.len();

    bench.data.set_fred(FRED_AFTER);
    tuesday_fetch(&bench, FRED_PATH).await;
    wait_scout(&bench, FRED_AFTER, fred_before, "FRED 값이 바뀌어 깨어남").await;
    settle().await;
    bench.advance(60).await;
    settle().await;
    let discussed = bench.discussed_symbols().await;
    assert!(
        discussed.is_empty(),
        "(pass)면 논의하지 않는다: {discussed:?}"
    );
    bench.close().await;
}
