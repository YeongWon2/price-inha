//! 8번 트리거(trigger 6차, 결정 2026-11-21 2 · 3 · 13번): 지켜보는 종목 · 기준가(지난 정규장 마지막 시세)로 시세
//! 조건(기본 3%)을 걸고, 자료를 주기적으로 가져와 새 공시 · 새 뉴스를 자료 사건으로 넘겨 뉴스 공시 시세
//! 담당(scout)을 깨운다. 다시 켠 뒤에도 같은 것으로 두 번 깨우지 않고, 네이버 하루 한도를 넘지 않는다.
//!
//! 시연이 아닌 모드로 띄운다: 시세는 로컬 가짜 시세 서버가, AI는 로컬 가짜 AI 서버가 준다. "깨어났다"는
//! 가짜 AI 서버가 뉴스 공시 시세 담당의 요청을 받았고 그 본문에 그 종목(또는 그 자료의 제목)이 있다는 뜻이다.

use std::time::{Duration, Instant};

use price_inha::{KIND_DAY_CLOSE, KIND_DAY_START, SETUP_SYMBOLS_PATH};
use serde_json::json;

use crate::common::{
    Bench, DISCLOSURE_NAME, ENTRY, HYNIX, NAVER_SECRET, NEWS_PATH, Plan, STEP, WATCHED,
    WATCHED_PRICE, article, kst, monday, on_date, plus, post, settle, tuesday, wait_for,
};

const NEWS_TITLE: &str = "시험회사 새 공장 착공 소식";
const NEWS_SUMMARY: &str = "시험회사가 새 공장을 짓기 시작했다";
const NEWS_LINK: &str = "https://news.example.test/articles/1";

/// 월요일 장을 마감한다.
async fn close_monday(bench: &Bench) {
    settle().await;
    bench.set(kst(monday(), 15, 31));
    let store = bench.store();
    wait_for("월요일 마감", || {
        on_date(&store, KIND_DAY_CLOSE, monday()) == 1
    })
    .await;
    settle().await;
}

/// 화요일 장을 열고(09:01) 조건이 걸릴 만큼 조금 간다.
async fn open_tuesday(bench: &Bench) {
    bench.set(kst(tuesday(), 9, 1));
    let store = bench.store();
    wait_for("화요일 시작", || {
        on_date(&store, KIND_DAY_START, tuesday()) >= 1
    })
    .await;
    settle().await;
    bench.advance(40).await;
}

/// 20초씩 가며 뉴스 공시 시세 담당이 `needle`로 깨어날 때까지 기다린다(가장 길게 [`STEP`]).
async fn wait_scout(bench: &Bench, needle: &str, from: usize, what: &str) {
    let deadline = Instant::now() + STEP;
    while bench.scout_calls(needle).await <= from {
        assert!(Instant::now() < deadline, "시간 안에 되지 않았다: {what}");
        bench.advance(20).await;
    }
}

/// 지켜볼 종목을 바꾼다(저장된 목록).
async fn watch(bench: &Bench, symbols: &[&str]) {
    let reply = post(
        bench.addr(),
        SETUP_SYMBOLS_PATH,
        &json!({ "symbols": symbols }),
    )
    .await;
    assert_eq!(reply.status, 200, "종목을 저장한다: {}", reply.text);
}

// 검증: T11 F2 V32
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 지켜보는_종목이_지난_마감가보다_3퍼센트_움직이면_뉴스_공시_시세_담당이_깨어난다() {
    let bench = Plan::live()
        .watching(&[WATCHED])
        .without_data_keys()
        .start(kst(monday(), 15, 25))
        .await;
    bench.ready().await;
    // 월요일 정규장의 마지막 시세가 화요일의 기준가다.
    close_monday(&bench).await;
    open_tuesday(&bench).await;
    let before = bench.scout_calls(WATCHED).await;

    // 2% 움직임으로는 깨우지 않는다(기본 3%).
    bench.set_price(WATCHED, WATCHED_PRICE * 102 / 100);
    bench.advance(60).await;
    settle().await;
    assert_eq!(
        bench.scout_calls(WATCHED).await,
        before,
        "기준가보다 2% 움직임으로는 깨우지 않는다"
    );

    bench.set_price(WATCHED, WATCHED_PRICE * 104 / 100);
    wait_scout(&bench, WATCHED, before, "4% 움직이면 깨어남").await;
    bench.close().await;
}

// 검증: T11 F2 C3
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 지켜보는_종목을_바꾸면_새로_넣은_종목의_시세_조건이_걸린다() {
    let bench = Plan::live()
        .watching(&[WATCHED, HYNIX])
        .without_data_keys()
        .start(kst(monday(), 15, 25))
        .await;
    bench.set_price(HYNIX, ENTRY);
    bench.ready().await;
    close_monday(&bench).await;
    // 장이 끝난 뒤 하이닉스를 뺀다: 화요일은 삼성전자만 지켜보며 시작한다.
    watch(&bench, &[WATCHED]).await;
    open_tuesday(&bench).await;

    // 화요일 장 중에 하이닉스를 다시 넣는다: 기준가(월요일 마지막 시세)로 조건이 새로 걸려야 한다.
    watch(&bench, &[WATCHED, HYNIX]).await;
    bench.advance(40).await;
    let before = bench.scout_calls(HYNIX).await;
    bench.set_price(HYNIX, ENTRY * 104 / 100);
    wait_scout(&bench, HYNIX, before, "새로 넣은 종목이 4% 움직이면 깨어남").await;
    bench.close().await;
}

// 검증: V2 T11 F2 E16 D9
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 새_공시와_새_뉴스가_뉴스_공시_시세_담당을_깨우고_다시_켜도_같은_것으로_두_번_깨우지_않는다()
 {
    let bench = Plan::live()
        .watching(&[WATCHED])
        .with_naver()
        .polling(Duration::from_secs(1))
        .start(kst(monday(), 10, 0))
        .await;
    bench.ready().await;
    bench.advance(40).await;

    bench.data.disclose(WATCHED, "20261116000777");
    wait_scout(&bench, DISCLOSURE_NAME, 0, "새 공시로 깨어남").await;
    settle().await;
    bench
        .data
        .set_news(vec![article(NEWS_TITLE, NEWS_SUMMARY, NEWS_LINK)]);
    wait_scout(&bench, NEWS_TITLE, 0, "새 뉴스로 깨어남").await;
    settle().await;
    let woke_disclosure = bench.scout_calls(DISCLOSURE_NAME).await;
    let woke_news = bench.scout_calls(NEWS_TITLE).await;
    let listed = bench.data.requests("/api/list.json").await.len();
    let searched = bench.data.requests(NEWS_PATH).await.len();

    // 다시 켠다: 공시 · 뉴스는 그대로 있다. 새로 가져와도 이미 본 것이라 깨우지 않는다.
    let later = plus(bench.now(), 60);
    let bench = bench.reopen(later).await;
    let deadline = Instant::now() + STEP;
    while bench.data.requests("/api/list.json").await.len() <= listed
        || bench.data.requests(NEWS_PATH).await.len() <= searched
    {
        assert!(Instant::now() < deadline, "다시 켠 뒤에도 자료를 가져온다");
        bench.advance(20).await;
    }
    bench.advance(60).await;
    settle().await;
    assert_eq!(
        bench.scout_calls(DISCLOSURE_NAME).await,
        woke_disclosure,
        "다시 켠 뒤 같은 공시로 다시 깨우지 않는다"
    );
    assert_eq!(
        bench.scout_calls(NEWS_TITLE).await,
        woke_news,
        "다시 켠 뒤 같은 뉴스로 다시 깨우지 않는다"
    );
    bench.close().await;
}

// 검증: D9 T11 N8
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 네이버_뉴스_검색은_하루_상한을_넘지_않는다() {
    let bench = Plan::live()
        .watching(&[WATCHED, HYNIX])
        .without_data_keys()
        .with_naver()
        .polling(Duration::from_secs(1))
        .news_limit(4)
        .start(kst(monday(), 10, 0))
        .await;
    // 세 시간을 15분씩 간다. 상한이 없으면 종목 둘 × 12걸음으로 4번을 넘는다.
    for _ in 0..12 {
        bench.set(plus(bench.now(), 15 * 60));
        settle().await;
    }
    let searched = bench.data.requests(NEWS_PATH).await;
    assert!(!searched.is_empty(), "뉴스를 주기적으로 가져온다");
    assert!(
        searched.len() <= 4,
        "하루 상한(4번)을 넘지 않는다: {}번",
        searched.len()
    );
    for request in &searched {
        assert!(
            !request.url.as_str().contains(NAVER_SECRET),
            "키는 주소에 실리지 않는다"
        );
    }
    bench.close().await;
}
