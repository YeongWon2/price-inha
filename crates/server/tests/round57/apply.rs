//! 바꾼 모델은 다시 켜지 않아도 다음 AI 턴부터 쓰인다: 그 담당의 AI 요청에 새 모델 이름이 실리고 다른 담당은 그대로다.
//! 논의가 도는 중에 바꾸면 그 논의는 시작할 때의 모델로 끝나고, 다음 논의부터 새 모델이다.

use std::time::Duration;

use price_inha::SETUP_MODELS_PATH;

use crate::common::{Bench, default_model, models_body, models_of, post, wait_for};

/// 고른 모델 둘(진짜 모델 이름처럼 보이지 않는다).
const SCOUT_MODEL: &str = "zz-scout-model-57";
const RISK_MODEL: &str = "zz-risk-model-57.b";

// 검증: R18-4 C32 V78
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn 바꾼_뒤_그_담당의_ai_요청에_새_모델이_실리고_다른_담당은_그대로다() {
    let bench = Bench::start().await;
    let from = bench.ai().settled().await;
    let body = models_body(
        &[("scout", SCOUT_MODEL), ("risk", RISK_MODEL)],
        &default_model(),
    );
    let reply = post(bench.addr(), SETUP_MODELS_PATH, &body).await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    bench.discuss("모델을 바꾼 뒤 논의").await;
    let asked = bench.ai().asked_from(from);
    let scout = models_of(&asked, "scout");
    let risk = models_of(&asked, "risk");
    assert!(!scout.is_empty() && !risk.is_empty(), "{asked:?}");
    assert!(
        scout.iter().all(|m| m == SCOUT_MODEL),
        "다시 켜지 않아도 scout 요청에 새 모델: {scout:?}"
    );
    assert!(
        risk.iter().all(|m| m == RISK_MODEL),
        "risk 요청에 새 모델: {risk:?}"
    );
    for other in ["strategy", "leader"] {
        let models = models_of(&asked, other);
        assert!(!models.is_empty(), "{other} 턴이 돌았다: {asked:?}");
        assert!(
            models.iter().all(|m| *m == default_model()),
            "{other}은 그대로 기본 모델: {models:?}"
        );
    }
    bench.close().await;
}

// 검증: R18-4 C32 V78
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn 논의가_도는_중에_바꿔도_그_논의는_시작할_때의_모델로_끝난다() {
    let bench = Bench::start().await;
    let from = bench.ai().settled().await;
    bench.ai().set_delay(Duration::from_millis(400));
    let crew = bench.crew();
    let running = tokio::spawn(async move {
        let _ = crew
            .discuss(crate::common::HYNIX, "도는 중에 바꾸는 논의")
            .await;
    });
    wait_for("논의의 첫 AI 요청", || {
        !models_of(&bench.ai().asked_from(from), "scout").is_empty()
    })
    .await;
    let body = models_body(&[("strategy", SCOUT_MODEL)], &default_model());
    let reply = post(bench.addr(), SETUP_MODELS_PATH, &body).await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    tokio::time::timeout(crate::common::STEP, running)
        .await
        .expect("논의가 시간 안에 끝나야 한다")
        .expect("논의 일이 끝나야 한다");
    let during = bench.ai().asked_from(from);
    assert!(
        !models_of(&during, "strategy").is_empty(),
        "바꾼 뒤에도 그 논의의 strategy 턴이 돌았다: {during:?}"
    );
    assert!(
        during.iter().all(|a| a.model == default_model()),
        "도는 논의는 시작할 때의 모델로 끝난다: {during:?}"
    );
    bench.ai().set_delay(Duration::ZERO);
    let next = bench.ai().settled().await;
    bench.discuss("다음 논의").await;
    let after = bench.ai().asked_from(next);
    let strategy = models_of(&after, "strategy");
    assert!(
        !strategy.is_empty() && strategy.iter().all(|m| m == SCOUT_MODEL),
        "다음 논의부터 새 모델: {after:?}"
    );
    bench.close().await;
}
