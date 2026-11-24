//! 27차 2번(F6-9a · E58 · V81): 실전 템플릿 멈추기 `POST` [`price_inha::TEMPLATE_STOP_PATH`].
//! 실전중 → 멈춤(승인 없이 바로), 모의중 · 승격대기 · 멈춤은 거부하고 그대로, 없는 번호는 404.

use pi_store::{TemplateId, TemplateState};
use price_inha::{TEMPLATE_NOT_LIVE, TEMPLATES_VIEW_PATH};
use serde_json::json;

use crate::common::{Bench, entry, get, post, stop_path};

// 검증: V81 E58 F6
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 실전중_템플릿은_승인_없이_바로_멈춤이_된다() {
    let bench = Bench::start().await;
    let id = bench.live_template().await;
    let pending_before = bench.approvals_pending();

    let reply = post(bench.addr(), &stop_path(id), &json!({}), None).await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    assert_eq!(reply.body["id"], json!(id.0), "{}", reply.text);
    assert_eq!(reply.body["state"], json!("stopped"), "{}", reply.text);
    assert_eq!(bench.state_of(id), TemplateState::Stopped, "저장소도 멈춤");
    assert_eq!(
        bench.approvals_pending(),
        pending_before,
        "승인 요청을 만들지 않는다"
    );

    let view = get(bench.addr(), TEMPLATES_VIEW_PATH, None).await;
    assert_eq!(
        entry(&view.body, id)["state"],
        json!("stopped"),
        "{}",
        view.text
    );
    bench.close().await;
}

// 검증: V81 E58 F6
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 멈춘_템플릿을_다시_멈추면_거부하고_그대로_멈춤이다() {
    let bench = Bench::start().await;
    let id = bench.live_template().await;
    let first = post(bench.addr(), &stop_path(id), &json!({}), None).await;
    assert_eq!(first.status, 200, "{}", first.text);

    let again = post(bench.addr(), &stop_path(id), &json!({}), None).await;
    assert_eq!(again.status, 409, "{}", again.text);
    assert_eq!(
        again.body["error"],
        json!(TEMPLATE_NOT_LIVE),
        "{}",
        again.text
    );
    assert_eq!(
        again.body["state"],
        json!("stopped"),
        "까닭(지금 상태): {}",
        again.text
    );
    assert_eq!(bench.state_of(id), TemplateState::Stopped);
    bench.close().await;
}

// 검증: V81 E58
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 모의중_템플릿은_멈추기를_거부하고_그대로_모의중이다() {
    let bench = Bench::start().await;
    let id = bench.choose("swing").await;
    let reply = post(bench.addr(), &stop_path(id), &json!({}), None).await;
    assert_eq!(reply.status, 409, "{}", reply.text);
    assert_eq!(
        reply.body["error"],
        json!(TEMPLATE_NOT_LIVE),
        "{}",
        reply.text
    );
    assert_eq!(reply.body["state"], json!("simulating"), "{}", reply.text);
    assert_eq!(bench.state_of(id), TemplateState::Simulating, "그대로다");
    bench.close().await;
}

// 검증: V81 E58
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 승격대기_템플릿은_멈추기를_거부하고_승인_요청도_그대로다() {
    let bench = Bench::start().await;
    let (id, number, _) = bench.pending_template();
    let reply = post(bench.addr(), &stop_path(id), &json!({}), None).await;
    assert_eq!(reply.status, 409, "{}", reply.text);
    assert_eq!(
        reply.body["error"],
        json!(TEMPLATE_NOT_LIVE),
        "{}",
        reply.text
    );
    assert_eq!(
        reply.body["state"],
        json!("promotion_pending"),
        "{}",
        reply.text
    );
    assert_eq!(
        bench.state_of(id),
        TemplateState::PromotionPending,
        "그대로다"
    );
    let view = get(bench.addr(), TEMPLATES_VIEW_PATH, None).await;
    assert_eq!(
        entry(&view.body, id)["approval"],
        json!(number),
        "승인 요청도 그대로: {}",
        view.text
    );
    bench.close().await;
}

// 검증: V81
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 없는_번호는_404이고_번호가_아니면_400이다() {
    let bench = Bench::start().await;
    let missing = post(bench.addr(), &stop_path(TemplateId(999)), &json!({}), None).await;
    assert_eq!(missing.status, 404, "{}", missing.text);
    assert_eq!(
        missing.body["error"],
        json!("not_found"),
        "{}",
        missing.text
    );
    let path = price_inha::TEMPLATE_STOP_PATH.replace("{id}", "abc");
    let wrong = post(bench.addr(), &path, &json!({}), None).await;
    assert_eq!(wrong.status, 400, "{}", wrong.text);
    assert_eq!(
        wrong.body["error"],
        json!("invalid_input"),
        "{}",
        wrong.text
    );
    bench.close().await;
}
