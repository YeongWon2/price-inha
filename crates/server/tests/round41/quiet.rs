//! 40차 5번: 조용한 시간 — 켜면 그 시간대(시장 시간대의 시각, 자정을 넘는 구간 포함)에는 급하지 않은 알림(notify의
//! `ignores_quiet_hours`가 거짓 — 백업 실패)이 메신저 · 폰 푸시로 가지 않고, 급한 알림(승인 요청)은 그대로 감 · 틀린
//! 시각 글자 400(결정 24 · V89 · V91 · E15). 시험 시계는 월요일 10:00(한국 시각)에 서 있다.

use std::time::{Duration, Instant};

use price_inha::{NOTIFY_INVALID_QUIET, NOTIFY_SETTINGS_FILE};
use serde_json::json;

use crate::common::{
    BACKUP_URGENCY, Bench, FakeTelegram, QUIET, STEP, URGENT, is_approval_alert, is_backup_alert,
};
use crate::settings::{read, save, save_ok, settings_body};

/// 연결된 사람.
const OWNER: i64 = 88_000_001;

/// 급하지 않은 알림(백업 실패)이 어느 전달처로도 가지 않았다.
async fn assert_backup_held(bench: &Bench, tg: &FakeTelegram, what: &str) {
    bench.backup_failure().await;
    tokio::time::sleep(QUIET).await;
    assert!(
        tg.sent_to(OWNER).await.iter().all(|t| !is_backup_alert(t)),
        "{what}: 급하지 않은 알림은 메신저로 가지 않는다"
    );
    assert_eq!(
        bench.push().count(BACKUP_URGENCY).await,
        0,
        "{what}: 급하지 않은 알림의 폰 푸시는 미룬다"
    );
}

// 검증: V89 V91 E15 M1
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 조용한_시간에는_급하지_않은_알림이_가지_않고_승인_요청은_간다() {
    let tg = FakeTelegram::start().await;
    let bench = Bench::linked(&tg, OWNER).await;
    // 10:00(한국 시각)이 드는 구간. 시장 시간대가 아닌 UTC(01:00)로 세면 들지 않는다.
    save_ok(&bench, &settings_body(&[], (true, "09:30", "11:00"))).await;

    assert_backup_held(&bench, &tg, "09:30~11:00").await;

    bench.ready().await;
    let (_number, _fingerprint) = bench.awaiting().await;
    tg.wait_sent(
        OWNER,
        "승인 요청은 조용한 시간에도 메신저로",
        is_approval_alert,
    )
    .await;
    bench.push().wait(URGENT, 1).await;
    bench.close().await;
}

// 검증: V89 V91 E15
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 자정을_넘는_구간도_그_시간대에_든다() {
    let tg = FakeTelegram::start().await;
    let bench = Bench::linked(&tg, OWNER).await;
    // 23:00 → 다음 날 10:30. 10:00은 든다.
    save_ok(&bench, &settings_body(&[], (true, "23:00", "10:30"))).await;
    assert_backup_held(&bench, &tg, "23:00~10:30").await;
    bench.close().await;
}

// 검증: V89 V91 E15
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 시간대_밖이거나_꺼져_있으면_급하지_않은_알림도_간다() {
    let tg = FakeTelegram::start().await;
    let bench = Bench::linked(&tg, OWNER).await;
    // 자정을 넘는 22:00 → 07:00은 10:00을 품지 않는다 · 끝(10:00)은 제외라 08:00 → 10:00도 밖 · 꺼져 있으면 시각과 상관없다.
    let cases = [
        (true, "22:00", "07:00"),
        (true, "08:00", "10:00"),
        (false, "09:30", "11:00"),
    ];
    for (sent, quiet) in cases.into_iter().enumerate() {
        save_ok(&bench, &settings_body(&[], quiet)).await;
        bench.backup_failure().await;
        let want = sent + 1;
        let deadline = Instant::now() + STEP;
        loop {
            let alerts = tg
                .sent_to(OWNER)
                .await
                .iter()
                .filter(|t| is_backup_alert(t))
                .count();
            if alerts >= want {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "{quiet:?}: 시간대 밖이면 메신저로 간다"
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        bench.push().wait(BACKUP_URGENCY, want).await;
    }
    bench.close().await;
}

// 검증: V91 E4 C15
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 틀린_시각_글자는_400이고_저장하지_않는다() {
    let tg = FakeTelegram::start().await;
    let bench = Bench::start(&tg).await;
    let before = read(&bench).await;
    for (start, end) in [
        ("25:00", "07:00"),
        ("22:00", "12:60"),
        ("7시", "07:00"),
        ("", "07:00"),
        ("22:00", "7:00"),
        ("2200", "0700"),
        ("10:00", "10:00"),
    ] {
        let reply = save(&bench, &settings_body(&[], (true, start, end))).await;
        assert_eq!(reply.status, 400, "{start}~{end}: {}", reply.text);
        assert_eq!(
            reply.body["error"],
            json!(NOTIFY_INVALID_QUIET),
            "{start}~{end}: {}",
            reply.text
        );
    }
    assert_eq!(read(&bench).await, before, "값은 그대로다");
    assert!(!bench.home().join(NOTIFY_SETTINGS_FILE).exists());
    bench.close().await;
}
