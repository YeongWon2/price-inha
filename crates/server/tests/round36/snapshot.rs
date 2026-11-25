//! 34차 1번 · 5번: 사무실 첫 화면 자료의 기다리는 승인 칸(C27 · E49 · V62).

use pi_approval::Subject;
use price_inha::OFFICE_PENDING_APPROVALS;
use serde_json::{Value, json};

use crate::common::{Bench, DART_KEY, buy_order, kst, monday, post, rule};

async fn start() -> Bench {
    Bench::start(kst(monday(), 10, 0)).await
}

/// 첫 화면 자료의 기다리는 승인 목록(배열이어야 한다).
fn pending_of(body: &Value) -> Vec<Value> {
    body[OFFICE_PENDING_APPROVALS]
        .as_array()
        .unwrap_or_else(|| panic!("{OFFICE_PENDING_APPROVALS} 칸은 배열이다: {body}"))
        .clone()
}

/// 목록에 실린 승인 번호들(차례 그대로).
fn numbers_of(list: &[Value]) -> Vec<String> {
    list.iter()
        .map(|row| row["number"].as_str().unwrap_or_default().to_string())
        .collect()
}

/// 객체의 칸 이름들(정렬).
fn keys_of(value: &Value) -> Vec<&str> {
    let mut keys: Vec<&str> = value
        .as_object()
        .unwrap_or_else(|| panic!("객체여야 한다: {value}"))
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    keys
}

// 검증: V62 C27 E49
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 기다리는_승인이_없으면_빈_목록이고_옛_칸은_그대로다() {
    let bench = start().await;
    let body = bench.office().await;
    assert_eq!(body[OFFICE_PENDING_APPROVALS], json!([]), "{body}");
    for field in ["agents", "pending", "last_seq", "talk"] {
        assert!(
            body.get(field).is_some(),
            "옛 칸 {field}은 그대로다: {body}"
        );
    }
    assert_eq!(body["agents"].as_array().map(Vec::len), Some(6), "{body}");
    bench.close().await;
}

// 검증: V62 C27 E49 R5-10
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 기다리는_승인만_담당_번호_시각과_함께_오래된_것부터_싣는다() {
    let bench = start().await;
    let base = bench.now();
    let at = |seconds: i64| base + chrono::Duration::seconds(seconds);

    bench.set(at(1));
    let improvement = bench.ask(rule(11), "개선안 11 반영 요청");
    bench.set(at(2));
    let restart = bench.ask(Subject::Restart, "전체 정지 뒤 다시 시작 요청");
    bench.set(at(3));
    let refused = bench.ask(rule(12), "개선안 12 반영 요청");
    bench.set(at(4));
    let order = bench.ask(
        buy_order(&bench, "signal-round36-a", 600),
        "하이닉스 5주 사기",
    );
    bench.set(at(5));
    bench.refuse(&refused);

    let body = bench.office().await;
    let list = pending_of(&body);
    assert_eq!(
        numbers_of(&list),
        [improvement.clone(), restart.clone(), order.clone()],
        "기다리는 것만, 만든 차례(오래된 것부터): {body}"
    );
    let expected = [
        (&improvement, "review", at(1)),
        (&restart, "leader", at(2)),
        (&order, "trader", at(4)),
    ];
    for (row, (number, agent, made)) in list.iter().zip(expected) {
        assert_eq!(
            keys_of(row),
            ["agent", "created_at", "number", "short_code", "summary"],
            "{row}"
        );
        assert_eq!(row["number"], json!(number), "{row}");
        assert_eq!(row["agent"], json!(agent), "요청한 담당: {row}");
        assert_eq!(
            row["created_at"],
            json!(made.timestamp_millis()),
            "만든 시각(밀리초): {row}"
        );
        assert_eq!(
            row["short_code"],
            json!(bench.view(number).short_code),
            "지문 앞 6자: {row}"
        );
        assert!(
            row["summary"].as_str().is_some_and(|s| !s.is_empty()),
            "가린 요약 글: {row}"
        );
    }
    bench.close().await;
}

// 검증: V62 C27 C7
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 승인하거나_거절하면_기다리는_승인에서_빠진다() {
    let bench = start().await;
    bench.ready_for_approval().await;
    let (number, fingerprint) = bench.awaiting().await;
    let number = number.as_str().to_string();
    let improvement = bench.ask(rule(21), "개선안 21 반영 요청");

    let before = pending_of(&bench.office().await);
    assert_eq!(
        numbers_of(&before),
        [number.clone(), improvement.clone()],
        "{before:?}"
    );
    assert_eq!(before[0]["agent"], json!("trader"), "주문은 매매 담당");

    let path = format!("/api/approvals/{number}/approve");
    let reply = post(
        bench.addr(),
        &path,
        &json!({"fingerprint": fingerprint.as_str()}),
    )
    .await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    let after_approve = pending_of(&bench.office().await);
    assert_eq!(
        numbers_of(&after_approve),
        std::slice::from_ref(&improvement),
        "승인한 것은 빠진다: {after_approve:?}"
    );

    bench.refuse(&improvement);
    let after_refuse = bench.office().await;
    assert_eq!(
        after_refuse[OFFICE_PENDING_APPROVALS],
        json!([]),
        "거절한 것도 빠진다: {after_refuse}"
    );
    bench.close().await;
}

// 검증: V62 C27 E49
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 만료되거나_답이_없어_닫히면_기다리는_승인에서_빠진다() {
    let bench = start().await;
    let expiring = bench.ask(
        buy_order(&bench, "signal-round36-b", 30),
        "곧 끝나는 신호의 주문",
    );
    let improvement = bench.ask(rule(31), "개선안 31 반영 요청");
    let before = pending_of(&bench.office().await);
    assert_eq!(
        numbers_of(&before),
        [expiring.clone(), improvement.clone()],
        "{before:?}"
    );

    bench.advance(60).await;
    bench.wait_closed(&expiring).await;
    let after = pending_of(&bench.office().await);
    assert_eq!(
        numbers_of(&after),
        [improvement],
        "만료된 것은 빠지고 답을 기다리는 개선안은 남는다: {after:?}"
    );
    bench.close().await;
}

// 검증: V62 C27 N8 E38
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 기다리는_승인에는_비밀값과_전체_지문이_없다() {
    let bench = start().await;
    let marker = "가린-요약-표시-34";
    let number = bench.ask(rule(41), &format!("{marker} 키 {DART_KEY} 로 받은 자료"));
    let fingerprint = bench.view(&number).fingerprint.as_str().to_string();

    let body = bench.office().await;
    let list = pending_of(&body);
    assert_eq!(numbers_of(&list), std::slice::from_ref(&number), "{body}");
    let summary = list[0]["summary"].as_str().expect("요약은 글");
    assert!(summary.contains(marker), "사유가 보인다: {summary}");
    assert!(!summary.contains(DART_KEY), "비밀 값은 가린다: {summary}");
    let whole = body.to_string();
    assert!(!whole.contains(DART_KEY), "응답 어디에도 비밀 값이 없다");
    assert!(!whole.contains(&fingerprint), "전체 지문은 싣지 않는다");
    bench.close().await;
}
