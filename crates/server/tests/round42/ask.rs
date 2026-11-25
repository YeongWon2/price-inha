//! 41차 3번: 메신저에서 담당에게 묻기(결정 26 · 33차 C18 · E43 · E44 · V58 · V90) — 지목한 담당만 사무실 팀 방 멘션과
//! 같은 길로 답하고 답이 메신저와 `office.chat` 양쪽에 · 둘이면 지목한 차례로 · 표시 이름 · id · 조사 · 없는 이름은 사무실과
//! 같은 안내 · 지목 없으면 도움말과 묻는 법(AI 호출 0) · 하루 상한 · 글 길이 · 보내기 간격은 사무실과 같은 규칙(까닭 답,
//! AI 호출 0) · 허용 안 된 사람의 글은 아무 일도 없음 · 사무실 글에 "메신저에서" 표시(`via`).

use price_inha::{
    KIND_OFFICE_MESSAGE, MESSENGER_ASK_CAPPED, MESSENGER_ASK_HINT, MESSENGER_ASK_TOO_FAST,
    MESSENGER_ASK_TOO_LONG, MESSENGER_UNKNOWN, MESSENGER_VIA, OFFICE_NO_MATCH_TEXT,
};
use serde_json::Value;

use crate::common::{Bench, FakeTelegram, QUIET, is_pairing_reply, said, wait_for};

/// 연결된 사람.
const OWNER: i64 = 87_200_001;
/// strategy의 채팅 답.
const STRATEGY_SAYS: &str = "전략입니다. 지켜봅니다.";
/// risk의 채팅 답.
const RISK_SAYS: &str = "리스크입니다. 손절선은 지킵니다.";

/// 연결된 사람과 운용 중인 팀, 정해 둔 채팅 답.
async fn asking_bench(tg: &FakeTelegram) -> Bench {
    let bench = Bench::linked(tg, OWNER).await;
    bench.ready().await;
    bench.ai().script_chat("strategy", STRATEGY_SAYS);
    bench.ai().script_chat("risk", RISK_SAYS);
    bench
}

/// 사무실 팀 방 글 이벤트 가운데 글에 `needle`이 든 것.
fn office_message(bench: &Bench, needle: &str) -> Option<Value> {
    bench
        .events(KIND_OFFICE_MESSAGE)
        .into_iter()
        .find(|e| e["text"].as_str().is_some_and(|t| t.contains(needle)))
}

/// 그 글 번호에 붙은 그 담당의 끝난 답(`office.chat` `complete`).
fn complete_reply(bench: &Bench, source: &Value, agent: &str) -> Option<Value> {
    bench.events(pi_office::EVENT_CHAT).into_iter().find(|e| {
        &e["sourceMessageId"] == source && e["agent"] == agent && e["status"] == "complete"
    })
}

/// 그 대화로 보낸 글 가운데 `needle`이 든 첫 글의 차례.
async fn position(tg: &FakeTelegram, needle: &str) -> Option<usize> {
    tg.sent_to(OWNER)
        .await
        .iter()
        .position(|t| t.contains(needle))
}

// 검증: V58 C18 R5-6 V90
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 지목한_담당만_답하고_답이_메신저와_office_chat_양쪽에_있고_사무실_글에_메신저에서_표시가_붙는다()
 {
    let tg = FakeTelegram::start().await;
    let bench = asking_bench(&tg).await;
    let question = "지금 어떻게 보세요?";
    tg.push_dm(OWNER, &format!("@전략 {question}"));
    let reply = tg
        .wait_sent(OWNER, "전략의 답", |t| {
            t.contains(&said("전략", STRATEGY_SAYS))
        })
        .await;
    assert!(!reply.contains(RISK_SAYS), "{reply}");

    let message = office_message(&bench, question).expect("사무실 팀 방 글로 남는다");
    assert_eq!(message["room"], "team", "{message}");
    assert_eq!(
        message["via"], MESSENGER_VIA,
        "메신저에서 온 글 표시: {message}"
    );
    let source = message["sourceMessageId"].clone();
    wait_for("office.chat에 전략의 끝난 답", || {
        complete_reply(&bench, &source, "strategy").is_some()
    })
    .await;
    let done = complete_reply(&bench, &source, "strategy").unwrap();
    assert_eq!(done["content"], STRATEGY_SAYS, "{done}");
    tokio::time::sleep(QUIET).await;
    assert!(complete_reply(&bench, &source, "risk").is_none());
    assert_eq!(
        bench.ai().roles_with(question),
        vec!["strategy"],
        "지목한 담당의 AI 턴만 돈다"
    );
    bench.close().await;
}

// 검증: V58 C18 R5-6
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 두_담당을_지목하면_지목한_차례로_답한다() {
    let tg = FakeTelegram::start().await;
    let bench = asking_bench(&tg).await;
    let question = "둘 다 어떻게 보세요?";
    tg.push_dm(OWNER, &format!("@리스크 @전략 {question}"));
    tg.wait_sent(OWNER, "리스크의 답", |t| {
        t.contains(&said("리스크", RISK_SAYS))
    })
    .await;
    tg.wait_sent(OWNER, "전략의 답", |t| {
        t.contains(&said("전략", STRATEGY_SAYS))
    })
    .await;
    let risk = position(&tg, &said("리스크", RISK_SAYS)).await.unwrap();
    let strategy = position(&tg, &said("전략", STRATEGY_SAYS)).await.unwrap();
    assert!(risk < strategy, "지목한 차례로 답한다");
    assert_eq!(bench.ai().roles_with(question), vec!["risk", "strategy"]);
    bench.close().await;
}

// 검증: V58 C18
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 담당_id와_조사가_붙은_이름도_지목으로_받는다() {
    let tg = FakeTelegram::start().await;
    let bench = asking_bench(&tg).await;
    tg.push_dm(OWNER, "@strategy 금리는 어때요?");
    tg.wait_sent(OWNER, "id로 지목한 전략의 답", |t| {
        t.contains(&said("전략", STRATEGY_SAYS))
    })
    .await;
    assert_eq!(bench.ai().roles_with("금리는 어때요?"), vec!["strategy"]);

    bench.pass(3);
    tg.push_dm(OWNER, "@리스크는 손절을 어떻게 봐요?");
    tg.wait_sent(OWNER, "조사가 붙은 리스크의 답", |t| {
        t.contains(&said("리스크", RISK_SAYS))
    })
    .await;
    assert_eq!(bench.ai().roles_with("손절을 어떻게 봐요?"), vec!["risk"]);
    bench.close().await;
}

// 검증: V58 E43
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 없는_이름이면_사무실과_같은_안내를_답하고_ai를_부르지_않는다() {
    let tg = FakeTelegram::start().await;
    let bench = asking_bench(&tg).await;
    bench.ai().settled().await;
    tg.push_dm(OWNER, "@회계 이번 분기 어때요?");
    tg.wait_sent(OWNER, "없는 이름 안내", |t| {
        t.contains(OFFICE_NO_MATCH_TEXT)
    })
    .await;
    tokio::time::sleep(QUIET).await;
    assert_eq!(bench.ai().asked_with("이번 분기 어때요?"), 0);
    bench.close().await;
}

// 검증: V58 E44 V90
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 지목이_없으면_도움말과_묻는_법을_답하고_ai를_부르지_않는다() {
    let tg = FakeTelegram::start().await;
    let bench = asking_bench(&tg).await;
    tg.push_dm(OWNER, "오늘 시장 어때요?");
    let reply = tg
        .wait_sent(OWNER, "지목 없는 글의 답", |t| {
            t.contains(MESSENGER_ASK_HINT)
        })
        .await;
    assert!(reply.contains(MESSENGER_UNKNOWN), "{reply}");
    assert!(MESSENGER_ASK_HINT.contains("@전략"), "{MESSENGER_ASK_HINT}");
    tokio::time::sleep(QUIET).await;
    assert_eq!(bench.ai().asked_with("오늘 시장 어때요?"), 0);
    assert!(
        office_message(&bench, "오늘 시장 어때요?").is_none(),
        "지목 없는 글은 사무실로 넘기지 않는다"
    );
    bench.close().await;
}

// 검증: V58 E45 R5-6
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 하루_ai_상한에_걸리면_까닭을_답하고_ai를_부르지_않는다() {
    let tg = FakeTelegram::start().await;
    let bench = Bench::linked(&tg, OWNER).await;
    // 상한 2: 팀장 예비분(최소 2)만 남는다 — 담당 답에 쓸 몫이 없다.
    let bench = bench.with_ai_limit(&tg, 2).await;
    bench.ai().script_chat("strategy", STRATEGY_SAYS);
    tg.push_dm(OWNER, "@전략 상한 시험입니다");
    tg.wait_sent(OWNER, "상한 까닭", |t| t.contains(MESSENGER_ASK_CAPPED))
        .await;
    tokio::time::sleep(QUIET).await;
    assert_eq!(bench.ai().asked_with("상한 시험입니다"), 0);
    assert_eq!(
        tg.sent_to(OWNER)
            .await
            .iter()
            .filter(|t| t.contains(STRATEGY_SAYS))
            .count(),
        0
    );
    bench.close().await;
}

// 검증: V58 E4
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 글이_사무실_길이_한도를_넘으면_까닭을_답하고_ai를_부르지_않는다() {
    let tg = FakeTelegram::start().await;
    let bench = asking_bench(&tg).await;
    let long = format!("@전략 긴글시험{}", "가".repeat(600));
    tg.push_dm(OWNER, &long);
    tg.wait_sent(OWNER, "길이 까닭", |t| {
        t.contains(MESSENGER_ASK_TOO_LONG)
    })
    .await;
    tokio::time::sleep(QUIET).await;
    assert_eq!(bench.ai().asked_with("긴글시험"), 0);
    assert!(office_message(&bench, "긴글시험").is_none());
    bench.close().await;
}

// 검증: V58 E4 R5-6
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 보내기_간격_안에_또_물으면_까닭을_답하고_뒤_글은_ai를_부르지_않는다() {
    let tg = FakeTelegram::start().await;
    let bench = asking_bench(&tg).await;
    tg.push_dm(OWNER, "@전략 첫째 질문입니다");
    tg.push_dm(OWNER, "@전략 둘째 질문입니다");
    tg.wait_sent(OWNER, "간격 까닭", |t| {
        t.contains(MESSENGER_ASK_TOO_FAST)
    })
    .await;
    tg.wait_sent(OWNER, "첫 글의 답", |t| {
        t.contains(&said("전략", STRATEGY_SAYS))
    })
    .await;
    tokio::time::sleep(QUIET).await;
    assert_eq!(bench.ai().asked_with("첫째 질문입니다"), 1);
    assert_eq!(bench.ai().asked_with("둘째 질문입니다"), 0);

    // 시계로 간격이 지나면 다시 물을 수 있다.
    bench.pass(3);
    tg.push_dm(OWNER, "@전략 셋째 질문입니다");
    wait_for("셋째 글이 AI 턴으로 간다", || {
        bench.ai().asked_with("셋째 질문입니다") == 1
    })
    .await;
    bench.close().await;
}

// 검증: V58 V88 V90
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 허용되지_않은_사람이나_그룹의_글은_아무_일도_없다() {
    let tg = FakeTelegram::start().await;
    let bench = asking_bench(&tg).await;
    let stranger: i64 = 87_200_099;
    tg.push_dm(stranger, "@전략 몰래 묻습니다");
    tg.wait_sent(stranger, "모르는 사람에게는 코드", is_pairing_reply)
        .await;
    tg.push_group(OWNER, -1_009_000_042, "@전략 그룹에서 묻습니다");
    tokio::time::sleep(QUIET).await;
    assert_eq!(bench.ai().asked_with("몰래 묻습니다"), 0);
    assert_eq!(bench.ai().asked_with("그룹에서 묻습니다"), 0);
    assert!(office_message(&bench, "몰래 묻습니다").is_none());
    assert!(office_message(&bench, "그룹에서 묻습니다").is_none());
    assert!(
        tg.sent()
            .await
            .iter()
            .all(|s| !s.text.contains(STRATEGY_SAYS)),
        "아무 담당도 답하지 않는다"
    );
    bench.close().await;
}
