//! `GET /api/guidelines`(세션, 원격도 읽는다) · `POST /api/guidelines/{id}/retire`(이 컴퓨터에서만).

use pi_journal::Mistake;
use price_inha::{GUIDELINES_PATH, KIND_GUIDELINE};
use serde_json::{Value, json};

use crate::common::{Bench, events, get, post};

fn retire_path(id: &str) -> String {
    format!("{GUIDELINES_PATH}/{id}/retire")
}

/// 같은 실수 세 번으로 지침 하나를 올리고 그 번호를 돌려준다.
async fn one_guideline(bench: &Bench) -> String {
    bench.reviewed_with(3, &Mistake::NoRationale);
    bench.close_day().await;
    let body = bench.guidelines().await;
    body["active"][0]["id"]
        .as_str()
        .expect("활성 지침")
        .to_string()
}

// 검증: V104 R8 (읽기: 활성 · 내린 것 목록과 성적 칸, 이 컴퓨터면 고칠 수 있음)
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn 지침_목록은_활성과_내린_것을_성적과_함께_보인다() {
    let bench = Bench::start().await;
    let empty = bench.guidelines().await;
    assert_eq!(empty["active"], json!([]), "{empty}");
    assert_eq!(empty["retired"], json!([]), "{empty}");
    assert_eq!(empty["editable"], json!(true));

    let id = one_guideline(&bench).await;
    let body = bench.guidelines().await;
    let row: &Value = &body["active"][0];
    assert_eq!(row["id"], json!(id));
    assert_eq!(row["kind"], json!("avoid"));
    assert_eq!(row["state"], json!("active"));
    assert_eq!(row["uses"], json!(0));
    assert_eq!(row["wins"], json!(0));
    assert_eq!(row["losses"], json!(0));
    assert_eq!(row["win_rate_pct"], json!(null));
    assert_eq!(row["retired_why"], json!(null));
    assert!(row["text"].as_str().is_some_and(|t| !t.is_empty()));
    assert!(row["when"].as_str().is_some_and(|t| !t.is_empty()));
    assert_eq!(row["basis"].as_array().map(Vec::len), Some(3));
    bench.close().await;
}

// 검증: V104 R8 (세션이 없으면 읽지 못한다)
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn 세션이_없으면_지침_목록을_읽지_못한다() {
    let bench = Bench::start().await;
    bench.go_remote();
    let reply = get(bench.addr(), GUIDELINES_PATH, None).await;
    assert_eq!(reply.status, 401, "{}", reply.text);
    bench.close().await;
}

// 검증: V104 R8 (이 컴퓨터에서 끄면 내린 것으로 옮기고 이벤트를 남긴다 · 없는 번호는 404)
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn 이_컴퓨터에서_끄면_내린_것으로_옮기고_이벤트를_남긴다() {
    let bench = Bench::start().await;
    let id = one_guideline(&bench).await;
    let reply = bench.post(&retire_path(&id), &json!({})).await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    assert_eq!(reply.body, json!({"id": id, "state": "retired"}));

    let body = bench.guidelines().await;
    assert_eq!(body["active"], json!([]));
    assert_eq!(body["retired"][0]["id"], json!(id));
    assert_eq!(body["retired"][0]["retired_why"], json!("person"));

    let retired: Vec<Value> = events(&bench.store(), KIND_GUIDELINE)
        .into_iter()
        .filter(|event| event["action"] == json!("retired"))
        .collect();
    assert_eq!(retired.len(), 1, "{retired:?}");
    assert_eq!(retired[0]["id"], json!(id));
    assert_eq!(retired[0]["why"], json!("person"));

    let again = bench.post(&retire_path(&id), &json!({})).await;
    assert_eq!(again.status, 200, "이미 내렸어도 같은 답: {}", again.text);
    let count = events(&bench.store(), KIND_GUIDELINE)
        .iter()
        .filter(|event| event["action"] == json!("retired"))
        .count();
    assert_eq!(count, 1, "다시 꺼도 이벤트는 하나");

    let missing = bench.post(&retire_path("g999"), &json!({})).await;
    assert_eq!(missing.status, 404, "{}", missing.text);
    assert_eq!(missing.body, json!({"error": "not_found"}));
    bench.close().await;
}

// 검증: V104 R8 (다른 기기는 읽기만 하고 끄면 403 — 지침은 그대로다)
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn 다른_기기는_읽기만_하고_끄면_403이다() {
    let bench = Bench::start().await;
    let id = one_guideline(&bench).await;
    let cookie = bench.remote_login().await;
    let read = get(bench.addr(), GUIDELINES_PATH, Some(&cookie)).await;
    assert_eq!(read.status, 200, "원격 세션도 읽는다: {}", read.text);
    assert_eq!(read.body["editable"], json!(false));
    let reply = post(bench.addr(), &retire_path(&id), &cookie, &json!({})).await;
    assert_eq!(reply.status, 403, "{}", reply.text);
    let after = get(bench.addr(), GUIDELINES_PATH, Some(&cookie)).await;
    assert_eq!(after.body["active"][0]["id"], json!(id), "그대로 활성");
    bench.close().await;
}
