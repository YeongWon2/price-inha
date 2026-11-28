//! 발판: 시연이 아닌 모드로 월요일 장중에 띄워 스윙(뉴스 구독)을 쓰고, 처음 있던 기사는 "본 것"으로만 적는다
//! (round64 `wake`의 발판을 본떴다).

use std::time::{Duration, Instant};

use pi_store::{Template, TemplateState};
use price_inha::TEMPLATE_PATH;
use serde_json::{Value, json};

use crate::live::{Bench, NEWS_PATH, Plan, STEP, WATCHED, article, kst, monday, post, settle};

/// 처음부터 있던 기사의 제목(깨우지 않는다).
pub const FIRST_TITLE: &str = "백로그U 처음 있던 소식";

/// 시험 기사 주소.
pub fn link(tag: &str, n: u32) -> String {
    format!("https://news.example.test/articles/round68-{tag}-{n}")
}

/// 처음 있던 기사 하나.
pub fn first_article() -> Value {
    article(FIRST_TITLE, "처음 요약", &link("first", 0))
}

/// 뉴스 키 · 1초 간격으로 월요일 장중에 띄워 스윙(뉴스 구독)을 쓴다. 첫 가져오기까지 기다린다.
pub async fn subscribed_bench() -> Bench {
    let bench = Plan::live()
        .watching(&[WATCHED])
        .without_data_keys()
        .with_naver()
        .polling(Duration::from_secs(1))
        .start(kst(monday(), 10, 0))
        .await;
    bench.data.set_news(vec![first_article()]);
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
    bench
}

/// 네이버를 한 번 더 가져갈 때까지 시계를 1초씩 옮긴다(조용한 시간 30초 안에서).
pub async fn next_fetch(bench: &Bench) {
    let asked = bench.data.requests(NEWS_PATH).await.len();
    let deadline = Instant::now() + STEP;
    while bench.data.requests(NEWS_PATH).await.len() <= asked {
        assert!(Instant::now() < deadline, "다음 가져오기가 온다");
        let at = bench.now() + chrono::Duration::seconds(1);
        bench.set(at);
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// 쓰는 템플릿(가장 나중 것).
pub fn latest_template(bench: &Bench) -> Template {
    let store = bench.store();
    TemplateState::ALL
        .iter()
        .flat_map(|state| store.templates_in_state(*state).unwrap())
        .max_by_key(|template| template.id)
        .expect("쓰는 템플릿")
}

// 검증: T11 F2
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 발판이_서고_처음_있던_기사로는_깨우지_않는다() {
    let bench = subscribed_bench().await;
    assert_eq!(latest_template(&bench).state, TemplateState::Simulating);
    assert_eq!(bench.scout_calls(FIRST_TITLE).await, 0);
    bench.close().await;
}
