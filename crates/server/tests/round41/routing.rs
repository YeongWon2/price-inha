//! 40차 4 · 7번: 전달처 고르기는 알림이 나가는 한곳에서 — 종류별로 끈 전달처로는 가지 않음(가짜 텔레그램 · 가짜 푸시로
//! 확인) · 끄지 않은 전달처로는 그대로 감 · 설정 파일이 깨지면 기본(모두 보냄)(결정 24 · V89 · V91 · E15).

use price_inha::NOTIFY_SETTINGS_FILE;

use crate::common::{
    BACKUP_URGENCY, Bench, FakeTelegram, QUIET, URGENT, fresh_home, is_approval_alert,
    is_backup_alert,
};
use crate::settings::{read, save_ok, settings_body};

/// 연결된 사람.
const OWNER: i64 = 87_000_001;

/// 조용한 시간은 끈 채로 둔다.
const QUIET_OFF: (bool, &str, &str) = (false, "22:00", "07:00");

// 검증: V89 V91 E15
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 폰_푸시를_끈_종류는_폰으로_가지_않고_메신저로는_간다() {
    let tg = FakeTelegram::start().await;
    let bench = Bench::linked(&tg, OWNER).await;
    save_ok(
        &bench,
        &settings_body(&[("backup_failure", false, true)], QUIET_OFF),
    )
    .await;

    bench.backup_failure().await;
    tg.wait_sent(OWNER, "백업 실패 알림은 메신저로", is_backup_alert)
        .await;
    tokio::time::sleep(QUIET).await;
    assert_eq!(
        bench.push().count(BACKUP_URGENCY).await,
        0,
        "폰 푸시를 끈 종류는 푸시 서비스로 보내지 않는다"
    );
    bench.close().await;
}

// 검증: V89 V91 E15
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 메신저를_끈_종류는_메신저로_가지_않고_폰으로는_간다() {
    let tg = FakeTelegram::start().await;
    let bench = Bench::linked(&tg, OWNER).await;
    save_ok(
        &bench,
        &settings_body(&[("backup_failure", true, false)], QUIET_OFF),
    )
    .await;

    bench.backup_failure().await;
    bench.push().wait(BACKUP_URGENCY, 1).await;
    tokio::time::sleep(QUIET).await;
    assert!(
        tg.sent_to(OWNER).await.iter().all(|t| !is_backup_alert(t)),
        "메신저를 끈 종류는 대화로 보내지 않는다"
    );
    bench.close().await;
}

// 검증: V89 V91 M1
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 승인_요청의_메신저를_끄면_폰으로만_간다() {
    // 잠긴 종류도 한 전달처는 끌 수 있다: 승인 요청은 폰 푸시로만.
    let tg = FakeTelegram::start().await;
    let bench = Bench::linked(&tg, OWNER).await;
    save_ok(
        &bench,
        &settings_body(&[("approval_request", true, false)], QUIET_OFF),
    )
    .await;
    bench.ready().await;

    let (_number, _fingerprint) = bench.awaiting().await;
    bench.push().wait(URGENT, 1).await;
    tokio::time::sleep(QUIET).await;
    assert!(
        tg.sent_to(OWNER)
            .await
            .iter()
            .all(|t| !is_approval_alert(t)),
        "승인 요청 알림이 대화로 가지 않는다"
    );
    bench.close().await;
}

// 검증: V89 V91 E15
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 다른_종류를_꺼도_끄지_않은_종류는_두_전달처로_간다() {
    let tg = FakeTelegram::start().await;
    let bench = Bench::linked(&tg, OWNER).await;
    let routes = [
        ("daily_summary", false, false),
        ("access_lock", false, false),
    ];
    save_ok(&bench, &settings_body(&routes, QUIET_OFF)).await;

    bench.backup_failure().await;
    tg.wait_sent(OWNER, "백업 실패 알림은 메신저로", is_backup_alert)
        .await;
    bench.push().wait(BACKUP_URGENCY, 1).await;
    bench.close().await;
}

// 검증: V89 V91 E15 M6
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 설정_파일이_깨지면_기본으로_모두_보낸다() {
    // 설정을 읽지 못하면 기본(모두 보냄)으로 — 알림이 조용히 사라지지 않게(40차 7번).
    let tg = FakeTelegram::start().await;
    let home = fresh_home().await;
    std::fs::write(home.path().join(NOTIFY_SETTINGS_FILE), "{\"kinds\": [깨진").unwrap();
    let bench = Bench::start_in(home, &tg).await;
    let reply = bench.register(crate::common::TOKEN).await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    bench.wait_state("polling").await;
    bench.pair(&tg, OWNER).await;
    bench.subscribe().await;

    let settings = read(&bench).await;
    let kinds = settings["kinds"].as_array().expect("기본 모양을 읽는다");
    assert!(
        kinds.iter().all(|row| (row["kind"]
            == serde_json::json!(price_inha::NOTIFY_TEAM_TALK_KIND)
            || row["push"] == serde_json::json!(true))
            && row["messenger"] == serde_json::json!(true)),
        "깨진 파일이면 기본: {settings}"
    );

    bench.backup_failure().await;
    tg.wait_sent(OWNER, "백업 실패 알림은 메신저로", is_backup_alert)
        .await;
    bench.push().wait(BACKUP_URGENCY, 1).await;
    bench.close().await;
}
