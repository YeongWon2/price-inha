//! 39차 3번: 받기 루프 — 처음 보는 1:1에만 코드 답 · 그룹 무시 · 다시 켜도 이어 받기 · 오류 종류대로(V87 · V88).

use std::time::Duration;

use price_inha::MESSENGER_HELP;
use serde_json::json;

use crate::common::{
    BACKOFF_INITIAL, BACKOFF_MAX, Bench, FakeTelegram, QUIET, STEP, is_pairing_reply,
};

/// 받기 루프가 그 갱신을 지나갔다(그 뒤 번호를 `offset`으로 물었다)는 것을 기다린다.
async fn wait_past(tg: &FakeTelegram, update: i64) {
    let deadline = std::time::Instant::now() + STEP;
    loop {
        if tg.offsets().await.iter().flatten().any(|o| *o > update) {
            return;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "시간 안에 {update}번 갱신을 지나가지 않았다"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

// 검증: V88 V87
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 처음_말을_건_1대1_대화에_코드를_한_번만_답한다() {
    let tg = FakeTelegram::start().await;
    let bench = Bench::with_token(&tg).await;
    let sender: i64 = 81_000_001;

    tg.push_dm(sender, "안녕하세요");
    tg.wait_sent(sender, "코드 답", is_pairing_reply).await;
    let again = tg.push_dm(sender, "코드 다시 주세요");
    wait_past(&tg, again).await;
    tokio::time::sleep(QUIET).await;

    let replies = tg.sent_to(sender).await;
    assert_eq!(
        replies.len(),
        1,
        "기다리는 동안 코드를 다시 주지 않는다: {replies:?}"
    );
    assert_eq!(tg.sent().await.len(), 1, "다른 대화로도 보내지 않는다");
    let status = bench.status().await;
    assert_eq!(status["telegram"]["pending"], json!(1), "{status}");
    bench.close().await;
}

// 검증: V88
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 그룹_글은_받지_않고_답하지_않는다() {
    let tg = FakeTelegram::start().await;
    let bench = Bench::with_token(&tg).await;
    let sender: i64 = 81_000_002;
    let group: i64 = -1_009_000_777;

    let id = tg.push_group(sender, group, "/status");
    wait_past(&tg, id).await;
    tokio::time::sleep(QUIET).await;

    assert!(
        tg.sent().await.is_empty(),
        "그룹에도 그 사람에게도 답하지 않는다"
    );
    let status = bench.status().await;
    assert_eq!(
        status["telegram"]["pending"],
        json!(0),
        "페어링 요청도 없다: {status}"
    );
    bench.close().await;
}

// 검증: V87 V88
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 다시_켜도_받은_자리를_이어_같은_글을_두_번_처리하지_않는다() {
    let tg = FakeTelegram::start().await;
    let bench = Bench::with_token(&tg).await;
    let owner: i64 = 81_000_003;
    bench.pair(&tg, owner).await;

    let help = tg.push_dm(owner, "/help");
    tg.wait_sent(owner, "도움말", |t| t.contains(MESSENGER_HELP))
        .await;
    wait_past(&tg, help).await;
    let before = tg.offsets().await.len();

    let bench = bench.restart(&tg).await;
    tg.wait_count("getUpdates", before + 2).await;
    let after: Vec<Option<i64>> = tg.offsets().await.split_off(before);
    assert!(
        after.iter().all(|o| o.is_some_and(|o| o > help)),
        "다시 켠 뒤에도 받은 자리 다음부터 묻는다: {after:?}"
    );
    tokio::time::sleep(QUIET).await;
    let helps = tg
        .sent_to(owner)
        .await
        .into_iter()
        .filter(|t| t.contains(MESSENGER_HELP))
        .count();
    assert_eq!(helps, 1, "같은 글을 두 번 처리하지 않는다");
    let status = bench.status().await;
    assert_eq!(
        status["telegram"]["paired"],
        json!(true),
        "장부도 이어진다: {status}"
    );
    bench.close().await;
}

// 검증: V87
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 토큰이_거절되면_받기를_멈추고_상태가_unauthorized다() {
    let tg = FakeTelegram::start().await;
    let bench = Bench::with_token(&tg).await;

    tg.revoke();
    bench.wait_state("unauthorized").await;
    let polled = tg.count("getUpdates").await;
    tokio::time::sleep(QUIET).await;
    assert_eq!(tg.count("getUpdates").await, polled, "다시 묻지 않는다");
    let status = bench.status().await;
    assert_eq!(
        status["telegram"]["configured"],
        json!(true),
        "토큰은 남는다: {status}"
    );
    bench.close().await;
}

// 검증: V87
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 요청이_많다는_답에는_그만큼_쉬고_다시_받는다() {
    let tg = FakeTelegram::start().await;
    let bench = Bench::with_token(&tg).await;

    tg.rate_limit_once(37);
    let sender: i64 = 81_000_004;
    tg.push_dm(sender, "안녕하세요");
    tg.wait_sent(sender, "쉰 뒤의 코드 답", is_pairing_reply)
        .await;
    assert!(
        bench.clock().slept(Duration::from_secs(37)),
        "retry_after 만큼 시계로 쉰다"
    );
    bench.close().await;
}

// 검증: V87
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 연결이_실패하면_두_배씩_물러서며_다시_하고_나아지면_받는다() {
    let tg = FakeTelegram::start().await;
    let bench = Bench::with_token(&tg).await;

    tg.fail_updates(4);
    let sender: i64 = 81_000_005;
    tg.push_dm(sender, "안녕하세요");
    tg.wait_sent(sender, "나아진 뒤의 코드 답", is_pairing_reply)
        .await;
    let clock = bench.clock();
    assert!(clock.slept(BACKOFF_INITIAL), "처음 물러서기");
    assert!(clock.slept(BACKOFF_INITIAL * 2), "두 배");
    assert!(clock.slept(BACKOFF_MAX), "가장 긴 간격에서 멈춘다");
    assert!(
        !clock.slept(BACKOFF_INITIAL * 4),
        "가장 긴 간격을 넘지 않는다"
    );
    bench.wait_state("polling").await;
    bench.close().await;
}
