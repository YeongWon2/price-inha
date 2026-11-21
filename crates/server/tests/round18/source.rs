//! 10b 보강 6번(검토 B 참고): 뉴스 공시 시세 담당(scout)을 깨우는 글의 "새 뉴스: …" · "새 공시: …" 줄에 출처
//! (naver · dart)와 "바깥 글" 표시를 붙인다(논의 자료와 같은 방식). 거시(fred · ecos)의 깨우기 글은 3번 시험
//! (`macro_wake`)이 본다.
//!
//! 뉴스 · 공시를 모두 구독하는 단타 템플릿으로 깨운다(10b 보강 4번의 구독 트리거와 부딪히지 않게).

use std::time::{Duration, Instant};

use price_inha::TEMPLATE_PATH;
use serde_json::json;

use crate::common::{
    Bench, DISCLOSURE_NAME, Plan, STEP, WATCHED, article, kst, monday, post, settle,
};

const NEWS_TITLE: &str = "시험회사 신제품 출시 소식";
/// 외부에서 온 글이라는 표시(지시서 글자 그대로).
const OUTSIDE: &str = "바깥 글";

/// 20초씩 가며 scout 턴 가운데 `needle`이 든 것이 생길 때까지 기다리고 그 글들을 돌려준다.
async fn scout_woke(bench: &Bench, needle: &str, what: &str) -> Vec<String> {
    let deadline = Instant::now() + STEP;
    loop {
        let inputs = bench.scout_inputs(needle).await;
        if !inputs.is_empty() {
            return inputs;
        }
        assert!(Instant::now() < deadline, "시간 안에 되지 않았다: {what}");
        bench.advance(20).await;
    }
}

// 검증: T11 F2 V35 D9
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn scout_깨우기_글의_새_공시와_새_뉴스에_출처와_바깥_글_표시가_있다() {
    let bench = Plan::live()
        .watching(&[WATCHED])
        .with_naver()
        .polling(Duration::from_secs(1))
        .start(kst(monday(), 10, 0))
        .await;
    let reply = post(bench.addr(), TEMPLATE_PATH, &json!({"style": "day_trade"})).await;
    assert_eq!(reply.status, 200, "템플릿을 골라야 한다: {}", reply.body);
    bench.trade_today();
    bench.advance(40).await;

    bench.data.disclose(WATCHED, "20261116000888");
    let disclosure = scout_woke(&bench, DISCLOSURE_NAME, "새 공시로 깨어남").await;
    assert!(
        disclosure
            .iter()
            .any(|input| input.to_lowercase().contains("dart") && input.contains(OUTSIDE)),
        "새 공시 깨우기 글에 출처(dart)와 바깥 글 표시가 있다: {disclosure:?}"
    );
    settle().await;

    bench.data.set_news(vec![article(
        NEWS_TITLE,
        "신제품을 내놓았다",
        "https://news.example.test/a/9",
    )]);
    let news = scout_woke(&bench, NEWS_TITLE, "새 뉴스로 깨어남").await;
    assert!(
        news.iter()
            .any(|input| input.to_lowercase().contains("naver") && input.contains(OUTSIDE)),
        "새 뉴스 깨우기 글에 출처(naver)와 바깥 글 표시가 있다: {news:?}"
    );
    bench.close().await;
}
