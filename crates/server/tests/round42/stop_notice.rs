//! 41차 6번: 전체 정지 알림(40차 검토 · 결정 25 · F4 · R14-1 · V9 · V89 · V91) — 팀이 전체 정지가 되면(내가 정지: 화면 ·
//! 메신저 `/stop`, 최대 손실) 멈춤 알림(`NoticeKind::Stall`, 급함)이 한 번 난다 · 제목 "전체 정지" · 본문에 까닭과 "다시
//! 시작은 계좌 화면에서" · 이미 정지면 다시 내지 않음 · 폰과 메신저(설정대로) · 조용한 시간에도 감.

use pi_store::{TeamCause, TeamState};
use price_inha::{
    MESSENGER_ALREADY_STOPPED, MESSENGER_STOPPED, STOP_NOTICE_MAX_LOSS, STOP_NOTICE_OWNER,
    STOP_NOTICE_RESTART, STOP_NOTICE_TITLE, TEAM_STOP_PATH,
};

use crate::common::{Bench, FakeTelegram, QUIET, URGENT, post_empty, wait_for};
use crate::settings::{save_ok, settings_body};

/// 연결된 사람.
const OWNER: i64 = 87_500_001;

/// 전체 정지 알림의 메신저 글인가(제목으로 시작하고 다시 시작 안내가 든다 — `/stop`의 답 글과 다르다).
fn is_stop_notice(text: &str) -> bool {
    text.starts_with(STOP_NOTICE_TITLE) && text.contains(STOP_NOTICE_RESTART)
}

/// 그 대화로 간 전체 정지 알림들.
async fn stop_notices(tg: &FakeTelegram) -> Vec<String> {
    tg.sent_to(OWNER)
        .await
        .into_iter()
        .filter(|t| is_stop_notice(t))
        .collect()
}

// 검증: V9 V89 R14-1 F4 M1
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 화면에서_내가_정지하면_멈춤_알림이_한_번_메신저와_폰으로_간다() {
    let tg = FakeTelegram::start().await;
    let bench = Bench::linked(&tg, OWNER).await;
    bench.ready().await;
    let reply = post_empty(bench.addr(), TEAM_STOP_PATH).await;
    assert_eq!(reply.status, 200, "{}", reply.text);

    let notice = tg.wait_sent(OWNER, "전체 정지 알림", is_stop_notice).await;
    assert!(
        notice.contains(STOP_NOTICE_OWNER),
        "까닭은 내가 정지: {notice}"
    );
    assert!(!notice.contains(STOP_NOTICE_MAX_LOSS), "{notice}");
    bench.push().wait(URGENT, 1).await;
    tokio::time::sleep(QUIET).await;
    assert_eq!(stop_notices(&tg).await.len(), 1, "같은 정지에 한 번만");
    assert_eq!(bench.push().count(URGENT).await, 1, "폰에도 한 번만");
    bench.close().await;
}

// 검증: V9 V89 V90 R14-1
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 메신저_stop으로_정지해도_멈춤_알림이_가고_다시_정지해도_더_나지_않는다() {
    let tg = FakeTelegram::start().await;
    let bench = Bench::linked(&tg, OWNER).await;
    bench.ready().await;
    tg.push_dm(OWNER, "/stop");
    tg.wait_sent(OWNER, "정지 답", |t| t.contains(MESSENGER_STOPPED))
        .await;
    let notice = tg.wait_sent(OWNER, "전체 정지 알림", is_stop_notice).await;
    assert!(notice.contains(STOP_NOTICE_OWNER), "{notice}");

    tg.push_dm(OWNER, "/stop");
    tg.wait_sent(OWNER, "이미 정지 답", |t| {
        t.contains(MESSENGER_ALREADY_STOPPED)
    })
    .await;
    let again = post_empty(bench.addr(), TEAM_STOP_PATH).await;
    assert_eq!(again.status, 409, "{}", again.text);
    tokio::time::sleep(QUIET).await;
    assert_eq!(
        stop_notices(&tg).await.len(),
        1,
        "이미 전체 정지면 알림을 다시 내지 않는다"
    );
    bench.close().await;
}

// 검증: V9 V89 R14-1 T13
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 최대_손실로_전체_정지가_되면_까닭이_최대_손실인_멈춤_알림이_한_번_간다() {
    let tg = FakeTelegram::start().await;
    let bench = Bench::linked(&tg, OWNER).await;
    bench.ready_for_loss().await;
    bench.hold().await;
    bench.crash().await;
    let store = bench.store();
    wait_for("최대 손실로 전체 정지", || {
        store.team_state().unwrap() == TeamState::FullStop
    })
    .await;
    assert_eq!(
        store.team_history().unwrap().pop().unwrap().cause,
        TeamCause::MaxLoss
    );
    let notice = tg.wait_sent(OWNER, "전체 정지 알림", is_stop_notice).await;
    assert!(
        notice.contains(STOP_NOTICE_MAX_LOSS),
        "까닭은 최대 손실: {notice}"
    );
    assert!(!notice.contains(STOP_NOTICE_OWNER), "{notice}");
    bench.push().wait(URGENT, 1).await;

    // 정지한 뒤에 손실 시세가 또 와도 다시 내지 않는다.
    bench.crash().await;
    tokio::time::sleep(QUIET).await;
    assert_eq!(stop_notices(&tg).await.len(), 1, "같은 정지에 한 번만");
    bench.close().await;
}

// 검증: V9 V91 E15
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 조용한_시간에도_멈춤_알림은_간다() {
    let tg = FakeTelegram::start().await;
    let bench = Bench::linked(&tg, OWNER).await;
    bench.ready().await;
    // 시계는 월요일 10시(한국) — 09:00~11:00에 든다.
    save_ok(&bench, &settings_body(&[], None, (true, "09:00", "11:00"))).await;
    let reply = post_empty(bench.addr(), TEAM_STOP_PATH).await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    tg.wait_sent(OWNER, "조용한 시간의 전체 정지 알림", is_stop_notice)
        .await;
    bench.push().wait(URGENT, 1).await;
    bench.close().await;
}

// 검증: V9 V91 C15
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 멈춤의_폰을_끄면_멈춤_알림은_메신저로만_간다() {
    let tg = FakeTelegram::start().await;
    let bench = Bench::linked(&tg, OWNER).await;
    bench.ready().await;
    save_ok(
        &bench,
        &settings_body(&[("stall", false, true)], None, (false, "22:00", "07:00")),
    )
    .await;
    let reply = post_empty(bench.addr(), TEAM_STOP_PATH).await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    tg.wait_sent(OWNER, "메신저로 간 전체 정지 알림", is_stop_notice)
        .await;
    tokio::time::sleep(QUIET).await;
    assert_eq!(
        bench.push().count(URGENT).await,
        0,
        "설정대로 폰으로는 가지 않는다"
    );
    bench.close().await;
}
