//! 구독 "실전 멈춤 → 다시 시작"(server 10b-2 · 결정 19): 실전중 템플릿도 그 구독으로 깨우고, 내가 실전 템플릿을
//! 멈추면 구독이 멈추며(네이버를 그만 부른다), 새 템플릿을 고르면(다시 시작) 구독이 다시 붙는다 — 멈춘 사이 쌓인
//! 뉴스로는 깨우지 않고 그 뒤 새 뉴스로 깨운다.
//!
//! 실전중 상태는 저장소 상태로만 만든다(승격대기 → 승인 소비 → 실전중). 실제 증권사 주문 길은 없다.

use std::time::Instant;

use pi_store::{ApprovalStatus, ApprovalUse, NewApprovalRequest, TemplateId, TemplateState};
use price_inha::TEMPLATE_PATH;
use serde_json::json;

use crate::bench::{first_article, latest_template, link, subscribed_bench};
use crate::live::{Bench, NEWS_PATH, STEP, article, ms, post, settle};

const LIVE_TITLE: &str = "백로그U 실전중 소식";
const STOPPED_TITLE: &str = "백로그U 멈춘 사이 소식";
const RESTART_TITLE: &str = "백로그U 다시 시작 뒤 소식";
const FINGERPRINT: &str = "round68-live-promotion";

/// 쓰는 템플릿(모의중)을 저장소 상태로만 실전중으로 올린다.
fn make_live(bench: &Bench) -> TemplateId {
    let store = bench.store();
    let current = latest_template(bench);
    let at = ms(bench.now());
    let pending = store
        .submit_template_for_promotion(current.id, &json!({ "passed": true }), at)
        .unwrap();
    let request = store
        .create_approval(NewApprovalRequest {
            fingerprint: FINGERPRINT.to_string(),
            summary: json!({ "템플릿": pending.id.0, "실전": true }),
            created_at: at,
            expires_at: pi_store::UnixMillis(at.0 + 3_600_000),
        })
        .unwrap();
    store
        .resolve_approval(request.id, ApprovalStatus::Approved, at)
        .unwrap();
    let live = store
        .promote_template_live(
            pending.id,
            ApprovalUse {
                id: request.id,
                fingerprint: FINGERPRINT.to_string(),
            },
            at,
        )
        .unwrap();
    assert_eq!(live.state, TemplateState::Live);
    live.id
}

/// `needle`이 담긴 기사로 담당이 깨어날 때까지 시계를 옮긴다.
async fn until_woken(bench: &Bench, needle: &str, why: &str) {
    let deadline = Instant::now() + STEP;
    while bench.scout_calls(needle).await == 0 {
        assert!(Instant::now() < deadline, "{why}");
        bench.advance(20).await;
    }
}

// 검증: T11 F2 C4 V29
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 실전_템플릿을_멈추면_구독이_멈추고_새_템플릿을_고르면_다시_붙는다() {
    let bench = subscribed_bench().await;
    let live = make_live(&bench);
    bench.data.set_news(vec![
        first_article(),
        article(LIVE_TITLE, "실전 요약", &link("live", 1)),
    ]);
    until_woken(&bench, LIVE_TITLE, "실전중 템플릿도 그 구독으로 깨운다").await;

    bench.store().stop_template(live, ms(bench.now())).unwrap();
    bench.advance(20).await;
    let asked = bench.data.requests(NEWS_PATH).await.len();
    bench.data.set_news(vec![
        first_article(),
        article(LIVE_TITLE, "실전 요약", &link("live", 1)),
        article(STOPPED_TITLE, "멈춘 사이 요약", &link("stopped", 1)),
    ]);
    bench.advance(120).await;
    assert!(
        bench.data.requests(NEWS_PATH).await.len() <= asked + 1,
        "실전 템플릿을 멈추면 다음 고리 안에 네이버를 그만 부른다"
    );
    assert_eq!(bench.scout_calls(STOPPED_TITLE).await, 0);

    let reply = post(bench.addr(), TEMPLATE_PATH, &json!({"style": "swing"})).await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    assert_eq!(latest_template(&bench).state, TemplateState::Simulating);
    let before = bench.data.requests(NEWS_PATH).await.len();
    let deadline = Instant::now() + STEP;
    while bench.data.requests(NEWS_PATH).await.len() <= before {
        assert!(
            Instant::now() < deadline,
            "새 템플릿을 고르면 네이버를 다시 가져온다"
        );
        bench.advance(20).await;
    }
    settle().await;
    bench.advance(120).await;
    settle().await;
    assert_eq!(
        bench.scout_calls(STOPPED_TITLE).await,
        0,
        "멈춘 사이 쌓인 뉴스로 한꺼번에 깨우지 않는다"
    );

    bench.data.set_news(vec![
        first_article(),
        article(LIVE_TITLE, "실전 요약", &link("live", 1)),
        article(STOPPED_TITLE, "멈춘 사이 요약", &link("stopped", 1)),
        article(RESTART_TITLE, "다시 시작 요약", &link("restart", 1)),
    ]);
    until_woken(&bench, RESTART_TITLE, "다시 시작한 뒤 새 뉴스로는 깨운다").await;
    assert_eq!(bench.scout_calls(STOPPED_TITLE).await, 0);
    bench.close().await;
}
