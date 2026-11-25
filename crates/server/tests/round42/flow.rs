//! 41차 1 · 2번: 팀 논의가 메신저로 흐른다(결정 26 · V89 · V91 · V58) — 허용된 대화로 `"이름: 글"`이 가린 채 가고 묶음 창
//! 안의 글은 메시지 하나 · 허용된 사람이 없을 때의 글은 버림 · "팀 논의"를 끄면 가지 않음 · 조용한 시간이면 가지 않음
//! (승인 요청은 감, 지난 글은 다시 보내지 않음).
//!
//! 논의 글은 가짜 AI가 정한 글이다(scout가 strategy · risk를 부르고 둘이 답한다, 팀장은 지켜보기). 켜며 뒤에서 도는 하루
//! 판단 글과 섞이지 않게 글 내용으로 가려 센다.

use std::time::Duration;

use price_inha::KIND_OFFICE_TALK;

use crate::common::{
    AI_KEY, Bench, FakeTelegram, HYNIX, QUIET, RISK_TALK, STRATEGY_TALK, TALK_BATCH,
    is_approval_alert, said, sent_with, wait_for,
};
use crate::settings::{save_ok, settings_body};

/// 연결된 사람.
const OWNER: i64 = 87_100_001;

/// 메신저로 간 strategy 논의 줄.
fn strategy_line() -> String {
    said("전략", STRATEGY_TALK)
}

/// 메신저로 간 risk 논의 줄(앞부분).
fn risk_line() -> String {
    said("리스크", "손절 기준을 지키면")
}

/// 논의를 한 번 돌리고 그 논의의 strategy `office.talk`가 날 때까지 기다린다(사무실 길은 그대로다).
async fn discuss_and_wait(bench: &Bench, topic: &str) {
    let before = strategy_talks(bench);
    bench.discuss(topic).await;
    wait_for("이번 논의의 strategy office.talk", || {
        strategy_talks(bench) > before
    })
    .await;
}

fn strategy_talks(bench: &Bench) -> usize {
    bench
        .events(KIND_OFFICE_TALK)
        .iter()
        .filter(|e| e["agent"] == "strategy" && e["text"] == STRATEGY_TALK)
        .count()
}

/// 묶음 창이 닫히고도 남을 만큼 기다린다.
async fn past_batch() {
    tokio::time::sleep(TALK_BATCH + QUIET + Duration::from_millis(300)).await;
}

// 검증: V89 V58 R5-6 M1
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 논의가_돌면_허용된_대화로_이름과_글이_가린_채_묶여_한_메시지로_간다() {
    let tg = FakeTelegram::start().await;
    let bench = Bench::linked(&tg, OWNER).await;
    bench.ready().await;
    bench
        .ai()
        .script_talk("risk", &format!("{RISK_TALK} 참고 열쇠 {AI_KEY}"));
    discuss_and_wait(&bench, "메신저 흐름 시험").await;

    let message = tg
        .wait_sent(OWNER, "논의 글 메시지", |t| {
            t.contains(&strategy_line())
        })
        .await;
    assert!(
        message.contains(&risk_line()),
        "묶음 창 안의 strategy · risk 글은 메시지 하나에 든다: {message}"
    );
    assert!(
        message.contains(HYNIX),
        "종목 논의면 종목이 앞에 붙는다: {message}"
    );
    assert!(!message.contains(AI_KEY), "가린 글만 나간다: {message}");
    assert!(
        message.lines().any(|line| line.ends_with(&strategy_line())),
        "한 줄은 \"이름: 글\" 모양이다: {message}"
    );
    past_batch().await;
    assert_eq!(
        sent_with(&tg, OWNER, &strategy_line()).await,
        1,
        "같은 글을 두 번 보내지 않는다"
    );
    assert!(
        tg.sent().await.iter().all(|s| !s.text.contains(AI_KEY)),
        "어느 메시지에도 비밀 글자가 없다"
    );
    bench.close().await;
}

// 검증: V89 V88
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 허용된_사람이_없을_때의_논의_글은_보내지_않고_연결된_뒤의_논의만_간다() {
    let tg = FakeTelegram::start().await;
    let bench = Bench::with_token(&tg).await;
    bench.ready().await;
    discuss_and_wait(&bench, "연결 전 논의").await;
    past_batch().await;
    bench.pair(&tg, OWNER).await;
    past_batch().await;
    assert_eq!(
        sent_with(&tg, OWNER, &strategy_line()).await,
        0,
        "연결되기 전의 논의 글은 나중에도 보내지 않는다"
    );

    discuss_and_wait(&bench, "연결 뒤 논의").await;
    tg.wait_sent(OWNER, "연결 뒤 논의 글", |t| {
        t.contains(&strategy_line())
    })
    .await;
    past_batch().await;
    assert_eq!(sent_with(&tg, OWNER, &strategy_line()).await, 1);
    bench.close().await;
}

// 검증: V89 V91 C15
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 팀_논의를_끄면_논의_글이_가지_않고_다시_켜면_간다() {
    let tg = FakeTelegram::start().await;
    let bench = Bench::linked(&tg, OWNER).await;
    bench.ready().await;
    save_ok(
        &bench,
        &settings_body(&[], Some(false), (false, "22:00", "07:00")),
    )
    .await;
    discuss_and_wait(&bench, "꺼 둔 논의").await;
    past_batch().await;
    assert_eq!(
        sent_with(&tg, OWNER, &strategy_line()).await,
        0,
        "팀 논의를 끄면 사무실에는 나와도 메신저로는 가지 않는다"
    );

    save_ok(
        &bench,
        &settings_body(&[], Some(true), (false, "22:00", "07:00")),
    )
    .await;
    discuss_and_wait(&bench, "다시 켠 논의").await;
    tg.wait_sent(OWNER, "다시 켠 뒤 논의 글", |t| {
        t.contains(&strategy_line())
    })
    .await;
    past_batch().await;
    assert_eq!(
        sent_with(&tg, OWNER, &strategy_line()).await,
        1,
        "꺼 둔 동안의 글은 나중에 보내지 않는다"
    );
    bench.close().await;
}

// 검증: V89 V91 E15
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 조용한_시간에는_논의_글이_가지_않고_승인_요청은_가며_지난_글은_다시_보내지_않는다() {
    let tg = FakeTelegram::start().await;
    let bench = Bench::linked(&tg, OWNER).await;
    bench.ready().await;
    // 시계는 월요일 10시(한국) — 09:00~11:00에 든다.
    save_ok(&bench, &settings_body(&[], None, (true, "09:00", "11:00"))).await;
    discuss_and_wait(&bench, "조용한 시간 논의").await;
    let (_number, _fingerprint) = bench.awaiting().await;
    tg.wait_sent(OWNER, "급한 알림(승인 요청)은 간다", is_approval_alert)
        .await;
    past_batch().await;
    assert_eq!(
        sent_with(&tg, OWNER, &strategy_line()).await,
        0,
        "조용한 시간의 논의 글은 보내지 않는다"
    );

    save_ok(&bench, &settings_body(&[], None, (false, "09:00", "11:00"))).await;
    discuss_and_wait(&bench, "조용한 시간이 끝난 논의").await;
    tg.wait_sent(OWNER, "조용한 시간 밖의 논의 글", |t| {
        t.contains(&strategy_line())
    })
    .await;
    past_batch().await;
    assert_eq!(
        sent_with(&tg, OWNER, &strategy_line()).await,
        1,
        "조용한 시간에 버린 글은 다시 보내지 않는다"
    );
    bench.close().await;
}
