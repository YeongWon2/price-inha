//! 묶음 C "구독"(server.md 10b-2 5번): 구독을 다시 켤 때 꺼진 사이 쌓인 뉴스로 한꺼번에 깨우지 않기, 구독을 끄기
//! 직전 대기열에 든 사건 버리기, 템플릿 값 바뀜 → 다시 걸기. "공시"(10b 보강 2 2번): scout 답에서 부정 문맥의 종목
//! 코드는 고르지 않는다.
//!
//! 시연이 아닌 모드(로컬 가짜 시세 · AI · 자료 서버)로 띄운다. 뉴스 간격은 1초, 시계는 시험이 옮긴다.

use std::time::{Duration, Instant};

use pi_store::TemplateState;
use pi_template::{Template, TriggerKind};
use price_inha::{KIND_DAY_CLOSE, KIND_DAY_START, TEMPLATE_PATH};
use serde_json::json;

use crate::live::{
    Bench, FRED_PATH, HYNIX, NEWS_PATH, Plan, STEP, WATCHED, WATCHED_PRICE, article, kst, monday,
    ms, on_date, post, settle, tuesday, wait_for,
};

const FIRST_TITLE: &str = "구독시험 처음 있던 소식";
const OFF_TITLE: &str = "구독시험 꺼진 사이 소식";
const NEW_TITLE: &str = "구독시험 다시 켠 뒤 소식";
const QUEUED_TITLE: &str = "구독시험 끄기 직전 소식";

fn link(n: u32) -> String {
    format!("https://news.example.test/articles/round64-{n}")
}

/// 뉴스 키 · 1초 간격으로 월요일 장중에 띄워 스윙(뉴스 구독)을 쓰고, 처음 있던 기사를 "본 것"으로 적게 한다.
async fn subscribed_bench() -> Bench {
    let bench = Plan::live()
        .watching(&[WATCHED])
        .without_data_keys()
        .with_naver()
        .polling(Duration::from_secs(1))
        .start(kst(monday(), 10, 0))
        .await;
    bench
        .data
        .set_news(vec![article(FIRST_TITLE, "처음 요약", &link(1))]);
    let reply = post(bench.addr(), TEMPLATE_PATH, &json!({"style": "swing"})).await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    bench.trade_today();
    let deadline = Instant::now() + STEP;
    while bench.data.requests(NEWS_PATH).await.is_empty() {
        assert!(
            Instant::now() < deadline,
            "뉴스를 구독하면 네이버를 가져온다"
        );
        bench.advance(20).await;
    }
    settle().await;
    bench.advance(40).await;
    assert_eq!(
        bench.scout_calls(FIRST_TITLE).await,
        0,
        "처음 있던 기사로는 깨우지 않는다"
    );
    bench
}

/// 쓰는 템플릿(가장 나중 것, 모의중)의 구독에 뉴스를 넣거나 뺀다(값 바뀜 — 개선안 반영과 같은 새 판).
fn set_news_subscription(bench: &Bench, on: bool) {
    let store = bench.store();
    let current = store
        .templates_in_state(TemplateState::Simulating)
        .unwrap()
        .into_iter()
        .max_by_key(|t| t.id)
        .expect("쓰는 템플릿");
    let mut template = Template::from_json(&current.value.to_string()).unwrap();
    template
        .params
        .triggers
        .retain(|kind| *kind != TriggerKind::News);
    if on {
        template.params.triggers.push(TriggerKind::News);
    }
    let value: serde_json::Value = serde_json::from_str(&template.to_json()).unwrap();
    store
        .revise_template(current.id, &value, ms(bench.now()))
        .unwrap();
}

// 검증: T11 F2 C4
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 구독을_다시_켜면_꺼진_사이_쌓인_뉴스로_한꺼번에_깨우지_않고_그_뒤_새_뉴스로_깨어난다() {
    let bench = subscribed_bench().await;
    set_news_subscription(&bench, false);
    bench.advance(20).await;
    let asked = bench.data.requests(NEWS_PATH).await.len();
    bench.data.set_news(vec![
        article(FIRST_TITLE, "처음 요약", &link(1)),
        article(OFF_TITLE, "꺼진 사이 요약", &link(2)),
    ]);
    bench.advance(120).await;
    assert!(
        bench.data.requests(NEWS_PATH).await.len() <= asked + 1,
        "구독을 끄면(값 바뀜) 다음 tick 안에 네이버를 그만 부른다"
    );

    set_news_subscription(&bench, true);
    let deadline = Instant::now() + STEP;
    let before = bench.data.requests(NEWS_PATH).await.len();
    while bench.data.requests(NEWS_PATH).await.len() <= before {
        assert!(
            Instant::now() < deadline,
            "다시 켜면 네이버를 다시 가져온다"
        );
        bench.advance(20).await;
    }
    settle().await;
    bench.advance(120).await;
    settle().await;
    assert_eq!(
        bench.scout_calls(OFF_TITLE).await,
        0,
        "꺼진 사이 쌓인 뉴스로 한꺼번에 깨우지 않는다"
    );

    bench.data.set_news(vec![
        article(FIRST_TITLE, "처음 요약", &link(1)),
        article(OFF_TITLE, "꺼진 사이 요약", &link(2)),
        article(NEW_TITLE, "다시 켠 뒤 요약", &link(3)),
    ]);
    let deadline = Instant::now() + STEP;
    while bench.scout_calls(NEW_TITLE).await == 0 {
        assert!(
            Instant::now() < deadline,
            "다시 켠 뒤의 새 뉴스로는 깨어난다"
        );
        bench.advance(20).await;
    }
    assert_eq!(bench.scout_calls(OFF_TITLE).await, 0);
    bench.close().await;
}

// 검증: T11 F2 C4
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 구독을_끄기_직전_대기열에_든_뉴스_사건으로는_깨우지_않는다() {
    let bench = subscribed_bench().await;
    let asked = bench.data.requests(NEWS_PATH).await.len();
    bench.data.set_news(vec![
        article(FIRST_TITLE, "처음 요약", &link(1)),
        article(QUEUED_TITLE, "끄기 직전 요약", &link(4)),
    ]);
    // 새 기사를 가져온 것을 본 바로 뒤(깨우기는 조용한 시간 30초 뒤)에 구독을 끈다.
    let deadline = Instant::now() + STEP;
    while bench.data.requests(NEWS_PATH).await.len() <= asked {
        assert!(Instant::now() < deadline, "새 기사를 가져온다");
        let at = bench.now() + chrono::Duration::seconds(1);
        bench.set(at);
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    set_news_subscription(&bench, false);
    for _ in 0..5 {
        let at = bench.now() + chrono::Duration::seconds(1);
        bench.set(at);
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    bench.advance(120).await;
    settle().await;
    assert_eq!(
        bench.scout_calls(QUEUED_TITLE).await,
        0,
        "끄기 직전 대기열에 든 사건은 버린다"
    );
    bench.close().await;
}

/// 다섯 종목을 지켜보며(시세는 모두 있다) 월요일 FRED 값을 받아 둔다.
async fn macro_baseline(picks: &[&'static str]) -> Bench {
    let bench = Plan::live()
        .watching(picks)
        .with_fred()
        .start(kst(monday(), 9, 5))
        .await;
    for symbol in picks {
        bench.set_price(symbol, WATCHED_PRICE);
    }
    bench.ready().await;
    let deadline = Instant::now() + STEP;
    while bench.data.requests(FRED_PATH).await.is_empty() {
        assert!(
            Instant::now() < deadline,
            "월요일 장 시작 뒤 FRED를 처음 받는다"
        );
        bench.advance(20).await;
    }
    settle().await;
    bench.advance(60).await;
    bench
}

// 검증: T11 F2 E16
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 거시_깨우기에서_scout가_볼_것_없다고_한_종목은_논의하지_않는다() {
    let picks = [WATCHED, HYNIX, "100010"];
    let bench = macro_baseline(&picks).await;
    bench.scout_says("005930은 볼 것 없음, 000660은 관련 없음. 100010 살펴볼 만함");
    let before = bench.data.requests(FRED_PATH).await.len();
    bench.data.set_fred("4.35");
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
    let deadline = Instant::now() + STEP;
    while bench.discussed_symbols().await.is_empty() {
        assert!(Instant::now() < deadline, "고른 종목을 논의한다");
        bench.advance(20).await;
    }
    assert!(bench.data.requests(FRED_PATH).await.len() > before);
    settle().await;
    bench.advance(60).await;
    settle().await;
    let discussed = bench.discussed_symbols().await;
    assert_eq!(
        discussed.into_iter().collect::<Vec<_>>(),
        vec!["100010".to_string()],
        "볼 것 없다 · 관련 없다고 한 종목은 고르지 않는다"
    );
    bench.close().await;
}
