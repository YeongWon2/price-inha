//! 10b-2(결정 19): 쓰는 템플릿의 구독(`TemplateParams.triggers`)이 깨우기를 정한다.
//!
//! - `Quote`가 없으면 시세 조건을 걸지 않는다(손절 · 청산 감시는 그대로 돈다).
//! - `Disclosure`가 없으면 공시 · 재무 · 거시 사건을 넘기지 않고 주기 가져오기도 DART · ECOS · FRED를 부르지 않는다.
//! - `News`가 없으면 뉴스 사건을 넘기지 않고 네이버 주기 가져오기도 부르지 않는다.
//! - 쓰는 템플릿이 바뀌면 다시 건다. 쓰는 템플릿이 없으면 아무것도 걸지 않는다.
//!
//! 시연이 아닌 모드로 띄운다(로컬 가짜 시세 · AI · 자료 서버). 주기 가져오기는 장 시간 안에서만 돌므로(10b 보강
//! 1번) 시계를 장중에 둔다. 장 시작의 공시 보기(DART 공시 목록)는 주기 가져오기가 아니므로, 공시 목록 요청은
//! 장 시작이 지난 뒤의 늘어남만 센다. 모의계좌는 주문 시각 뒤의 시세로만 체결하므로 시계를 앞으로 옮기며 넣는다.

use std::time::{Duration, Instant};

use pi_store::{NewTemplate, TradeState};
use pi_template::{Style, Template, TriggerKind};
use price_inha::{KIND_DAY_CLOSE, KIND_DAY_START, TEMPLATE_PATH};
use serde_json::{Value, json};

use crate::common::{
    Bench, FRED_PATH, HYNIX, NEWS_PATH, Plan, STEP, STOP, WATCHED, WATCHED_PRICE, article, d, kst,
    monday, ms, on_date, plus, post, settle, tuesday, wait_for,
};

const NEWS_TITLE: &str = "시험회사 구독 시험 새 소식";
const NEWS_SUMMARY: &str = "시험회사가 새 제품을 내놓았다";
const NEWS_LINK: &str = "https://news.example.test/articles/subscribe-1";
/// DART 공시 목록 경로.
const LIST_PATH: &str = "/api/list.json";
/// DART 재무 경로.
const FINANCIALS_PATH: &str = "/api/fnlttSinglAcnt.json";
/// ECOS 한국은행 기준금리 통계표(경로에 든다).
const KOREA_RATE: &str = "/722Y001/";
/// ECOS 환율 통계표(경로에 든다).
const EXCHANGE_RATE: &str = "/731Y001/";
/// 바뀐 FRED 값(처음 `4.10`).
const FRED_AFTER: &str = "4.35";
/// 바뀐 ECOS 기준금리 값(처음 `2.25`).
const RATE_AFTER: &str = "2.75";
/// 손절값 아래의 시세.
const BELOW_STOP: i64 = STOP - 1_000;
/// 주기 가져오기가 여러 번 돌 만큼 걷는 시간(초). 자료 간격은 1초, 하루 상한으로 늘어난 간격도 몇 초다.
const WALK_SECONDS: i64 = 10 * 60;

/// 프리셋 템플릿을 고른다(새 템플릿 쓰기).
async fn choose(bench: &Bench, style: &str) {
    let reply = post(bench.addr(), TEMPLATE_PATH, &json!({ "style": style })).await;
    assert_eq!(reply.status, 200, "템플릿을 골라야 한다: {}", reply.body);
}

/// 스윙 값에서 구독만 `triggers`로 바꾼 템플릿을 저장소에 만들고(가장 나중에 만든 템플릿을 쓴다) 오늘
/// 매매하기로 한다.
fn subscribed(bench: &Bench, triggers: Vec<TriggerKind>) {
    let mut template = Template::new(Style::Swing);
    template.params.triggers = triggers;
    template.params.symbol_weight_pct = d("10");
    let value: Value = serde_json::from_str(&template.to_json()).unwrap();
    bench
        .store()
        .create_template(NewTemplate {
            value,
            at: ms(bench.now()),
        })
        .unwrap();
    bench.trade_today();
}

/// 자료 가짜 서버가 받은, 경로에 `part`가 든 요청 수.
async fn fetched(bench: &Bench, part: &str) -> usize {
    bench.data.requests_like(part).await.len()
}

/// 20초씩 가며 뉴스 공시 시세 담당이 `needle`로 깨어날 때까지 기다린다(가장 길게 [`STEP`]).
async fn wait_scout(bench: &Bench, needle: &str, from: usize, what: &str) {
    let deadline = Instant::now() + STEP;
    while bench.scout_calls(needle).await <= from {
        assert!(Instant::now() < deadline, "시간 안에 되지 않았다: {what}");
        bench.advance(20).await;
    }
}

/// 장 시작(공시 보기)이 지나가게 조금 간다.
async fn past_open(bench: &Bench, date: chrono::NaiveDate) {
    let store = bench.store();
    wait_for("장 시작", || on_date(&store, KIND_DAY_START, date) >= 1).await;
    settle().await;
    bench.advance(120).await;
}

/// 월요일 장을 마감하고 화요일 장을 연다(09:01).
async fn next_day(bench: &Bench) {
    settle().await;
    bench.set(kst(monday(), 15, 31));
    let store = bench.store();
    wait_for("월요일 마감", || {
        on_date(&store, KIND_DAY_CLOSE, monday()) == 1
    })
    .await;
    settle().await;
    bench.set(kst(tuesday(), 9, 1));
    past_open(bench, tuesday()).await;
}

// 검증: T11 F2 D9
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 뉴스를_구독하지_않은_템플릿에서는_새_뉴스가_와도_깨어나지_않고_네이버를_주기적으로_부르지_않는다()
 {
    // 중장기 프리셋은 뉴스를 구독하지 않는다(시험의 전제).
    let long_term = Template::new(Style::LongTerm);
    assert!(
        !long_term.params.triggers.contains(&TriggerKind::News),
        "중장기 프리셋의 구독에 뉴스가 없다: {:?}",
        long_term.params.triggers
    );
    let bench = Plan::live()
        .watching(&[WATCHED])
        .without_data_keys()
        .with_naver()
        .polling(Duration::from_secs(1))
        .start(kst(monday(), 10, 0))
        .await;
    choose(&bench, "long_term").await;
    bench.trade_today();
    bench.advance(60).await;

    bench
        .data
        .set_news(vec![article(NEWS_TITLE, NEWS_SUMMARY, NEWS_LINK)]);
    bench.advance(WALK_SECONDS).await;
    settle().await;
    assert_eq!(
        bench.data.requests(NEWS_PATH).await.len(),
        0,
        "뉴스를 구독하지 않으면 네이버를 주기적으로 부르지 않는다"
    );
    assert_eq!(
        bench.scout_calls(NEWS_TITLE).await,
        0,
        "뉴스를 구독하지 않으면 새 뉴스로 깨어나지 않는다"
    );
    bench.close().await;
}

// 검증: T11 F2 D9 V34
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 공시를_구독하지_않으면_dart_fred_ecos를_주기적으로_부르지_않고_거시_값이_바뀌어도_깨어나지_않는다()
 {
    let bench = Plan::live()
        .watching(&[WATCHED])
        .with_fred()
        .polling(Duration::from_secs(1))
        .start(kst(monday(), 9, 5))
        .await;
    subscribed(&bench, vec![TriggerKind::Quote, TriggerKind::News]);
    past_open(&bench, monday()).await;
    let listed = fetched(&bench, LIST_PATH).await;

    bench.advance(WALK_SECONDS).await;
    settle().await;
    assert_eq!(
        fetched(&bench, LIST_PATH).await,
        listed,
        "공시를 구독하지 않으면 DART 공시를 주기적으로 부르지 않는다"
    );

    // 거시 값이 바뀐 채 다음 날 장이 열려도 받지 않고 깨어나지 않는다.
    bench.data.set_fred(FRED_AFTER);
    bench.data.set_korea_rate(RATE_AFTER);
    let listed = fetched(&bench, LIST_PATH).await;
    next_day(&bench).await;
    let listed_after_open = fetched(&bench, LIST_PATH).await;
    bench.advance(WALK_SECONDS).await;
    settle().await;
    assert_eq!(
        fetched(&bench, LIST_PATH).await,
        listed_after_open,
        "다음 날에도 DART 공시를 주기적으로 부르지 않는다(장 시작의 공시 보기 전 {listed}번)"
    );
    for (part, name) in [
        (FRED_PATH, "FRED"),
        (KOREA_RATE, "ECOS 기준금리"),
        (EXCHANGE_RATE, "ECOS 환율"),
        (FINANCIALS_PATH, "DART 재무"),
    ] {
        assert_eq!(
            fetched(&bench, part).await,
            0,
            "공시를 구독하지 않으면 {name}를 주기적으로 부르지 않는다"
        );
    }
    assert!(
        bench.scout_inputs(FRED_AFTER).await.is_empty(),
        "FRED 값이 바뀌어도 깨어나지 않는다"
    );
    assert!(
        bench.scout_inputs(RATE_AFTER).await.is_empty(),
        "ECOS 값이 바뀌어도 깨어나지 않는다"
    );
    bench.close().await;
}

// 검증: T11 F2 F4
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 시세를_구독하지_않으면_시세_조건이_걸리지_않지만_손절은_나간다() {
    let bench = Plan::live()
        .watching(&[WATCHED])
        .without_data_keys()
        .start(kst(monday(), 15, 20))
        .await;
    subscribed(&bench, vec![TriggerKind::Disclosure, TriggerKind::News]);

    // 손절 감시는 구독과 상관없이 돈다: 손절값 아래 시세가 오면 파는 주문이 나가 팔린다.
    let held = bench.holding(HYNIX).await;
    let deadline = Instant::now() + STEP;
    while !matches!(
        bench.trade_state(held),
        TradeState::Sold | TradeState::Reviewed
    ) {
        assert!(Instant::now() < deadline, "시간 안에 손절로 팔리지 않았다");
        bench.quote(HYNIX, BELOW_STOP, plus(bench.now(), 1)).await;
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(
        !bench.sells(HYNIX).await.is_empty(),
        "시세를 구독하지 않아도 손절 주문은 나간다"
    );

    // 월요일 정규장의 마지막 시세가 화요일의 기준가다. 시세를 구독하지 않으면 4% 움직여도 깨우지 않는다.
    next_day(&bench).await;
    let before = bench.scout_calls(WATCHED).await;
    bench.set_price(WATCHED, WATCHED_PRICE * 104 / 100);
    bench.advance(180).await;
    settle().await;
    assert_eq!(
        bench.scout_calls(WATCHED).await,
        before,
        "시세를 구독하지 않으면 지켜보는 종목이 4% 움직여도 깨어나지 않는다"
    );
    bench.close().await;
}

// 검증: T11 F2 C4
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 템플릿을_뉴스_구독이_있는_것으로_바꾸면_그_뒤_새_뉴스로_깨어난다() {
    let bench = Plan::live()
        .watching(&[WATCHED])
        .without_data_keys()
        .with_naver()
        .polling(Duration::from_secs(1))
        .start(kst(monday(), 10, 0))
        .await;
    choose(&bench, "long_term").await;
    bench.trade_today();
    bench.advance(120).await;
    assert_eq!(
        bench.data.requests(NEWS_PATH).await.len(),
        0,
        "뉴스를 구독하지 않는 동안은 네이버를 부르지 않는다"
    );

    // 뉴스를 구독하는 스윙으로 바꾼다: 다음 tick 안에 다시 걸려 네이버를 가져오기 시작한다.
    choose(&bench, "swing").await;
    let deadline = Instant::now() + STEP;
    while bench.data.requests(NEWS_PATH).await.is_empty() {
        assert!(
            Instant::now() < deadline,
            "뉴스를 구독하는 템플릿으로 바꾸면 네이버를 가져온다"
        );
        bench.advance(20).await;
    }
    settle().await;
    bench.advance(20).await;

    bench
        .data
        .set_news(vec![article(NEWS_TITLE, NEWS_SUMMARY, NEWS_LINK)]);
    wait_scout(&bench, NEWS_TITLE, 0, "바꾼 뒤 새 뉴스로 깨어남").await;
    bench.close().await;
}

// 검증: T11 F2 D9
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 쓰는_템플릿이_없으면_자료를_주기적으로_가져오지_않는다() {
    let bench = Plan::live()
        .watching(&[WATCHED])
        .with_fred()
        .with_naver()
        .polling(Duration::from_secs(1))
        .start(kst(monday(), 9, 5))
        .await;
    past_open(&bench, monday()).await;
    let listed = fetched(&bench, LIST_PATH).await;

    bench.advance(WALK_SECONDS).await;
    settle().await;
    assert_eq!(
        fetched(&bench, LIST_PATH).await,
        listed,
        "쓰는 템플릿이 없으면 DART 공시를 주기적으로 부르지 않는다"
    );
    for (part, name) in [
        (NEWS_PATH, "네이버 뉴스"),
        (FRED_PATH, "FRED"),
        (KOREA_RATE, "ECOS 기준금리"),
        (EXCHANGE_RATE, "ECOS 환율"),
        (FINANCIALS_PATH, "DART 재무"),
    ] {
        assert_eq!(
            fetched(&bench, part).await,
            0,
            "쓰는 템플릿이 없으면 {name}를 주기적으로 부르지 않는다"
        );
    }
    bench.close().await;
}
