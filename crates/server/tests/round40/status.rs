//! 39차 2번: 메신저 상태 `GET /api/messenger`의 모양(보낸 사람 ID · 토큰 · 페어링 코드 없음, 결정 24 · V88).

use serde_json::{Value, json};

use crate::common::{Bench, FakeTelegram, QUIET, TOKEN, code_in, is_pairing_reply};

/// 텔레그램 칸의 이름들(정렬).
fn keys_of(status: &Value) -> Vec<String> {
    let mut keys: Vec<String> = status["telegram"]
        .as_object()
        .expect("telegram은 객체다")
        .keys()
        .cloned()
        .collect();
    keys.sort();
    keys
}

// 검증: V88 C2
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 토큰이_없으면_꺼짐이고_바깥으로_나가지_않는다() {
    let tg = FakeTelegram::start().await;
    let bench = Bench::start(&tg).await;
    let status = bench.status().await;
    assert_eq!(status["editable"], json!(true), "이 컴퓨터: {status}");
    let telegram = &status["telegram"];
    assert_eq!(telegram["configured"], json!(false), "{status}");
    assert_eq!(telegram["bot"], Value::Null, "{status}");
    assert_eq!(telegram["state"], json!("off"), "{status}");
    assert_eq!(telegram["paired"], json!(false), "{status}");
    assert_eq!(telegram["pending"], json!(0), "{status}");
    assert_eq!(telegram["locked_until"], Value::Null, "{status}");
    assert_eq!(
        keys_of(&status),
        [
            "bot",
            "configured",
            "locked_until",
            "paired",
            "pending",
            "state"
        ],
        "칸은 이것뿐이다: {status}"
    );
    tokio::time::sleep(QUIET).await;
    assert!(
        tg.requests().await.is_empty(),
        "토큰이 없으면 부르지 않는다"
    );
    bench.close().await;
}

// 검증: V88 N8 C2
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 상태에는_보낸_사람_번호와_토큰과_코드가_없다() {
    let tg = FakeTelegram::start().await;
    let bench = Bench::with_token(&tg).await;
    let sender: i64 = 90_012_345;
    tg.push_dm(sender, "안녕하세요");
    let reply = tg.wait_sent(sender, "코드 답", is_pairing_reply).await;
    let code = code_in(&reply).unwrap();

    let waiting = bench.status().await;
    assert_eq!(waiting["telegram"]["pending"], json!(1), "{waiting}");
    assert_eq!(waiting["telegram"]["paired"], json!(false), "{waiting}");
    let text = waiting.to_string();
    for secret in [sender.to_string().as_str(), TOKEN, code.as_str()] {
        assert!(!text.contains(secret), "상태에 {secret}이(가) 없다: {text}");
    }

    let approved = bench.approve(&code).await;
    assert_eq!(approved.status, 200, "{}", approved.text);
    assert!(
        !approved.text.contains(&sender.to_string()),
        "{}",
        approved.text
    );
    let paired = bench.status().await;
    assert_eq!(paired["telegram"]["paired"], json!(true), "{paired}");
    assert_eq!(paired["telegram"]["pending"], json!(0), "{paired}");
    assert_eq!(paired["telegram"]["state"], json!("polling"), "{paired}");
    let text = paired.to_string();
    for secret in [sender.to_string().as_str(), TOKEN, code.as_str()] {
        assert!(!text.contains(secret), "상태에 {secret}이(가) 없다: {text}");
    }
    bench.close().await;
}
