//! 6번: 설정 화면에 하루 AI 호출 상한. 설정 화면이 읽는 설정 상태(`GET` [`SETUP_PATH`])의
//! [`SETUP_AI_DAILY_LIMIT`] 칸에 지금 값(없음 = `null` / 수)이 보이고, AI 설정 저장 길(`POST` [`SETUP_AI_PATH`]의
//! [`AI_DAILY_LIMIT_KEY`] 칸)로 바꾼다. 바꾼 값은 다시 켜도 같고 팀의 하루 상한으로 쓰인다.

use price_inha::{AI_DAILY_LIMIT_KEY, SETUP_AI_DAILY_LIMIT, SETUP_AI_PATH, SETUP_PATH};
use serde_json::{Value, json};

use crate::common::{Bench, Plan, WATCHED, get, kst, monday, post};

/// 설정 상태의 하루 AI 호출 상한 칸.
async fn shown(bench: &Bench) -> Value {
    let reply = get(bench.addr(), SETUP_PATH).await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    assert!(
        reply.body.get(SETUP_AI_DAILY_LIMIT).is_some(),
        "설정 상태에 하루 AI 호출 상한 칸이 있다: {}",
        reply.text
    );
    reply.body[SETUP_AI_DAILY_LIMIT].clone()
}

/// 상한을 바꾼다.
async fn change(bench: &Bench, limit: Value) -> crate::common::Reply {
    let mut body = serde_json::Map::new();
    body.insert(AI_DAILY_LIMIT_KEY.to_string(), limit);
    post(bench.addr(), SETUP_AI_PATH, &Value::Object(body)).await
}

// 검증: E30 N5
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 설정_상태에_지금_상한이_보인다() {
    let none = Plan::demo().start(kst(monday(), 10, 0)).await;
    assert_eq!(shown(&none).await, Value::Null, "처음에는 상한 없음");
    none.close().await;

    let two = Plan::demo().ai_limit(2).start(kst(monday(), 10, 0)).await;
    assert_eq!(shown(&two).await, json!(2), "켤 때 준 상한이 보인다");
    two.close().await;
}

// 검증: E30 N5 C3
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 설정_저장으로_상한을_바꾸고_다시_켜도_같고_비우면_없음이다() {
    let bench = Plan::demo().start(kst(monday(), 10, 0)).await;
    let reply = change(&bench, json!(3)).await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    assert_eq!(reply.body[SETUP_AI_DAILY_LIMIT], json!(3), "{}", reply.text);
    assert_eq!(shown(&bench).await, json!(3));

    let at = bench.now();
    let bench = bench.reopen(at).await;
    assert_eq!(shown(&bench).await, json!(3), "다시 켜도 같다");

    let reply = change(&bench, Value::Null).await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    assert_eq!(shown(&bench).await, Value::Null, "비우면 상한 없음");
    bench.close().await;
}

// 검증: E30 C3
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 잘못된_상한은_400이고_그대로_둔다() {
    let bench = Plan::demo().start(kst(monday(), 10, 0)).await;
    assert_eq!(change(&bench, json!(5)).await.status, 200);
    for bad in [json!(0), json!(-1), json!(1.5), json!("3"), json!([3])] {
        let reply = change(&bench, bad.clone()).await;
        assert_eq!(reply.status, 400, "{bad}: {}", reply.text);
        assert_eq!(
            reply.body["error"], "invalid_input",
            "{bad}: {}",
            reply.text
        );
        assert_eq!(shown(&bench).await, json!(5), "{bad}는 저장하지 않는다");
    }
    bench.close().await;
}

// 검증: E30 E34 N5
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 저장한_상한이_다시_켠_뒤_팀의_하루_상한이_된다() {
    let bench = Plan::demo().start(kst(monday(), 10, 0)).await;
    let reply = change(&bench, json!(1)).await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    let at = bench.now();
    let bench = bench.reopen(at).await;

    let _first = bench.crew().discuss(WATCHED, "첫째").await;
    let after_first = bench.prompts().len();
    assert!(after_first > 0, "첫 논의는 AI를 부른다");
    let _second = bench.crew().discuss(WATCHED, "둘째").await;
    assert_eq!(
        bench.prompts().len(),
        after_first,
        "상한 1에 닿으면 둘째 논의는 AI를 부르지 않는다"
    );
    bench.close().await;
}
