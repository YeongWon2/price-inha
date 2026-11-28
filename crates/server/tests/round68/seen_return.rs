//! 본 것 기억 기록 시점(server 지시서 10b 다음 차수 후보 "넘긴 뒤 버려진 뉴스는 다시 오지 않음"): 깨우기 엔진이 한
//! 깨우기 상한(20)으로 버린 뉴스는 "본 것"에서 빠져, 다음 가져오기에서 다시 넘어가 담당을 깨운다.
//!
//! 같은 종목에 조용한 시간(30초) 안에 두 번 15개씩 새 기사가 오면 한 묶음에 30개가 들어 가장 이른 10개가 버려진다.
//! 예전에는 그 10개가 "본 것"으로 남아 다시 오지 않았다.

use std::time::Instant;

use serde_json::Value;

use crate::bench::{first_article, link, next_fetch, subscribed_bench};
use crate::live::{STEP, article, settle};

fn title(tag: &str, n: u32) -> String {
    format!("백로그U {tag}묶음 소식 {n:02}")
}

fn articles(tag: &str) -> Vec<Value> {
    (1..=15)
        .map(|n| article(&title(tag, n), "요약", &link(tag, n)))
        .collect()
}

// 검증: T11 F2 C4
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 깨우기_상한으로_버려진_뉴스는_본_것에서_빠져_다음_가져오기에서_다시_넘어가_깨운다() {
    let bench = subscribed_bench().await;
    let mut news = vec![first_article()];
    news.extend(articles("가"));
    bench.data.set_news(news.clone());
    next_fetch(&bench).await;
    news.extend(articles("나"));
    bench.data.set_news(news);
    next_fetch(&bench).await;
    settle().await;

    let earliest = title("가", 1);
    let deadline = Instant::now() + STEP * 2;
    while bench.scout_calls(&earliest).await == 0 {
        assert!(
            Instant::now() < deadline,
            "상한으로 버려진 가장 이른 기사도 결국 담당을 깨운다"
        );
        bench.advance(20).await;
    }
    for n in [10, 11, 15] {
        assert!(bench.scout_calls(&title("가", n)).await > 0, "가묶음 {n}");
    }
    assert!(bench.scout_calls(&title("나", 15)).await > 0);
    bench.close().await;
}
