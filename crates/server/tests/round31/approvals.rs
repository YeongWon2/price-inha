//! 29차 1번: 승인 요청 기록(C30 · V84).

use price_inha::{APPROVAL_LOG_PATH, APPROVAL_PATH};
use serde_json::{Value, json};

use crate::common::{Bench, DART_KEY, get, ids, kst, monday, next_of};

async fn start() -> Bench {
    Bench::start(kst(monday(), 10, 0)).await
}

/// 승인 요청 기록을 읽는다(200이어야 한다).
async fn page(bench: &Bench, query: &str) -> Value {
    let reply = get(bench.addr(), &format!("{APPROVAL_LOG_PATH}{query}")).await;
    assert_eq!(reply.status, 200, "{query}: {}", reply.body);
    reply.body
}

/// 그 승인 번호의 줄.
fn line_of(body: &Value, number: &str) -> Value {
    body["approvals"]
        .as_array()
        .and_then(|list| {
            list.iter()
                .find(|row| row["number"] == json!(number))
                .cloned()
        })
        .unwrap_or_else(|| panic!("{number}번이 기록에 있다: {body}"))
}

/// 1초씩 늦게 승인 요청 `count`개(개선안 번호 1부터). (승인 번호, 만든 시각) 차례대로.
fn asked(bench: &Bench, count: i64) -> Vec<(String, i64)> {
    let base = bench.now();
    (1..=count)
        .map(|i| {
            let at = base + chrono::Duration::seconds(i);
            bench.set(at);
            let number = bench.ask(i, &format!("개선안 {i} 반영 요청"));
            (number, at.timestamp_millis())
        })
        .collect()
}

// 검증: V84 C30
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 승인_요청이_없으면_기록이_비고_다음_쪽이_없다() {
    let bench = start().await;
    let body = page(&bench, "").await;
    assert_eq!(body["approvals"], json!([]), "{body}");
    assert_eq!(body["next"], Value::Null, "{body}");
    bench.close().await;
}

// 검증: V84 C30 C3
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 승인_기록은_최근_것부터_쪽을_넘기고_줄에_번호_상태_시각이_있다() {
    let bench = start().await;
    let made = asked(&bench, 5);
    let refused_at = bench.now() + chrono::Duration::seconds(10);
    bench.set(refused_at);
    bench.refuse(&made[1].0);

    let whole = page(&bench, "").await;
    let rows = ids(&whole, "approvals");
    assert_eq!(rows.len(), 5, "{whole}");
    assert!(
        rows.windows(2).all(|w| w[0] > w[1]),
        "최근 것부터: {rows:?}"
    );
    let numbers: Vec<Value> = whole["approvals"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["number"].clone())
        .collect();
    let expected: Vec<Value> = made.iter().rev().map(|(n, _)| json!(n)).collect();
    assert_eq!(numbers, expected, "만든 차례의 거꾸로: {whole}");
    assert_eq!(next_of(&whole), None);

    let refused = line_of(&whole, &made[1].0);
    assert_eq!(refused["status"], json!("rejected"), "{refused}");
    assert_eq!(refused["created_at"], json!(made[1].1), "밀리초: {refused}");
    assert_eq!(refused["resolved_at"], json!(refused_at.timestamp_millis()));
    assert_eq!(refused["consumed_at"], Value::Null);
    let waiting = line_of(&whole, &made[4].0);
    assert_eq!(waiting["status"], json!("pending"), "{waiting}");
    assert_eq!(waiting["created_at"], json!(made[4].1));
    assert_eq!(waiting["resolved_at"], Value::Null);
    assert_eq!(waiting["consumed_at"], Value::Null);

    let first = page(&bench, "?max=2").await;
    assert_eq!(ids(&first, "approvals"), rows[0..2].to_vec());
    assert_eq!(next_of(&first), Some(rows[1]));
    let second = page(&bench, &format!("?max=2&before={}", rows[1])).await;
    assert_eq!(ids(&second, "approvals"), rows[2..4].to_vec());
    assert_eq!(next_of(&second), Some(rows[3]));
    let third = page(&bench, &format!("?max=2&before={}", rows[3])).await;
    assert_eq!(ids(&third, "approvals"), rows[4..5].to_vec());
    assert_eq!(next_of(&third), None, "{third}");
    bench.close().await;
}

// 검증: V84 C30
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 승인_기록을_상태로_거르고_틀린_글자는_400이다() {
    let bench = start().await;
    let made = asked(&bench, 3);
    bench.refuse(&made[0].0);

    let rejected = page(&bench, "?status=rejected").await;
    let rows = rejected["approvals"].as_array().unwrap().clone();
    assert_eq!(rows.len(), 1, "{rejected}");
    assert_eq!(rows[0]["number"], json!(made[0].0));
    let pending = page(&bench, "?status=pending").await;
    let numbers: Vec<Value> = pending["approvals"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["number"].clone())
        .collect();
    assert_eq!(
        numbers,
        vec![json!(made[2].0), json!(made[1].0)],
        "{pending}"
    );
    let one = page(&bench, "?status=pending&max=1").await;
    let cursor = next_of(&one).expect("하나 더 있다");
    let two = page(&bench, &format!("?status=pending&max=1&before={cursor}")).await;
    assert_eq!(two["approvals"][0]["number"], json!(made[1].0));
    assert_eq!(next_of(&two), None);
    let expired = page(&bench, "?status=expired").await;
    assert_eq!(expired["approvals"], json!([]));

    for query in ["?status=bogus", "?status=Pending", "?max=0", "?before=x"] {
        let reply = get(bench.addr(), &format!("{APPROVAL_LOG_PATH}{query}")).await;
        assert_eq!(reply.status, 400, "{query}: {}", reply.body);
        assert_eq!(reply.body["error"], json!("invalid_input"), "{query}");
    }
    bench.close().await;
}

// 검증: V84 C30 N8 E38
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 승인_기록의_요약은_가린_글이고_승인_카드_경로는_그대로다() {
    let bench = start().await;
    let marker = "가린-요약-표시-29";
    let number = bench.ask(41, &format!("{marker} 키 {DART_KEY} 로 받은 자료"));

    let body = page(&bench, "").await;
    let row = line_of(&body, &number);
    let summary = row["summary"].as_str().expect("요약은 글");
    assert!(summary.contains(marker), "사유가 보인다: {summary}");
    assert!(!summary.contains(DART_KEY), "비밀 값은 가린다: {summary}");
    assert!(
        !body.to_string().contains(DART_KEY),
        "응답 어디에도 비밀 값이 없다"
    );

    // 같은 앞머리의 승인 카드 경로는 그대로 열린다(라우트가 부딪히지 않는다).
    let card = get(bench.addr(), &APPROVAL_PATH.replace("{id}", &number)).await;
    assert_eq!(card.status, 200, "{}", card.body);
    assert_eq!(card.body["number"], json!(number));
    assert_eq!(card.body["summary"], row["summary"], "카드와 같은 요약");
    bench.close().await;
}
