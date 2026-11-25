//! 40차 "발판 먼저"(39차 교훈): 새 시험 발판이 **지금 구현에서** 서는지 본다 — 팀이 운용 중이 되고 그대로 남음 ·
//! 승인 요청을 만들 수 있음 · 지금 알림이 메신저와 폰 푸시로 감. 이 파일의 시험은 구현 전에도 통과하는 것이 정상이다
//! (나머지 40차 시험이 이 발판 위에서 red인 까닭이 발판이 아니라 빠진 기능임을 보인다).

use std::time::Duration;

use pi_store::TeamState;

use crate::common::{
    BACKUP_URGENCY, Bench, FakeTelegram, URGENT, is_approval_alert, is_backup_alert,
};

/// 연결된 사람.
const OWNER: i64 = 85_000_001;

// 검증: V89 F4
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 발판은_운용_중이_되고_그대로_남는다() {
    let tg = FakeTelegram::start().await;
    let bench = Bench::linked(&tg, OWNER).await;
    bench.ready().await;
    let store = bench.store();
    assert_eq!(store.team_state().unwrap(), TeamState::Operating);
    tokio::time::sleep(Duration::from_secs(2)).await;
    assert_eq!(
        store.team_state().unwrap(),
        TeamState::Operating,
        "시세가 흐르므로 이상 감시가 팀을 멈추지 않는다"
    );
    bench.close().await;
}

// 검증: V89 V90 M1
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 발판에서_승인_요청을_만들면_메신저와_폰_푸시로_간다() {
    let tg = FakeTelegram::start().await;
    let bench = Bench::linked(&tg, OWNER).await;
    bench.ready().await;
    let (_number, _fingerprint) = bench.awaiting().await;
    tg.wait_sent(OWNER, "승인 요청 알림", is_approval_alert)
        .await;
    bench.push().wait(URGENT, 1).await;
    bench.close().await;
}

// 검증: V89 E15
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 발판에서_백업_실패_알림이_메신저와_폰_푸시로_간다() {
    let tg = FakeTelegram::start().await;
    let bench = Bench::linked(&tg, OWNER).await;
    bench.backup_failure().await;
    tg.wait_sent(OWNER, "백업 실패 알림", is_backup_alert).await;
    bench.push().wait(BACKUP_URGENCY, 1).await;
    bench.close().await;
}
