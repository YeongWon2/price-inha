//! 조용한 시간에 보내기: 승인과 이상은 무시하고, 기본값은 없다.

use pi_notify::{AttemptOutcome, HoldReason, NoticeKind};
use pi_store::UnixMillis;

use crate::common::{env, env_with, kst, night_quiet, notice, path_of};

// 검증: V12
#[tokio::test]
async fn 조용한_시간을_정하지_않으면_언제든_보낸다() {
    let env = env().await;
    let sub = env.subscribe(1);
    let night = kst(23, 30);
    env.clock.set(night);
    let delivery = env
        .notifier
        .notify(notice(NoticeKind::DailySummary, night))
        .await
        .unwrap();
    assert_eq!(delivery.held, None);
    assert_eq!(env.push.received_at(&path_of(&sub)).len(), 1);
}

// 검증: V12
#[tokio::test]
async fn 조용한_시간이면_하루_정리와_백업_실패_푸시는_미루고_이벤트는_남긴다() {
    let env = env_with(Some(night_quiet()), 2_000).await;
    let sub = env.subscribe(1);
    let night = kst(23, 30);
    env.clock.set(night);
    for kind in [NoticeKind::DailySummary, NoticeKind::BackupFailure] {
        let delivery = env.notifier.notify(notice(kind, night)).await.unwrap();
        assert_eq!(delivery.held, Some(HoldReason::QuietHours));
        assert!(delivery.attempts.is_empty());
        assert_eq!(delivery.first_attempt_at, None);
        assert!(delivery.seq > 0);
    }
    assert!(env.push.received_at(&path_of(&sub)).is_empty());
    // 알림 목록에는 둘 다 있다.
    let list = env.notifier.notices_after(0, 10).unwrap();
    assert_eq!(list.len(), 2);
}

// 검증: V10 V11
#[tokio::test]
async fn 조용한_시간에도_승인_요청과_이상은_보낸다() {
    let env = env_with(Some(night_quiet()), 2_000).await;
    let sub = env.subscribe(1);
    let night = kst(3, 0);
    env.clock.set(night);
    for kind in [NoticeKind::ApprovalRequest, NoticeKind::Anomaly] {
        let delivery = env.notifier.notify(notice(kind, night)).await.unwrap();
        assert_eq!(delivery.held, None);
        assert_eq!(
            delivery.attempts[0].outcome,
            AttemptOutcome::Delivered { status: 201 }
        );
        assert!(delivery.within_deadline(UnixMillis(night)));
    }
    assert_eq!(env.push.received_at(&path_of(&sub)).len(), 2);
}

// 검증: V12
#[tokio::test]
async fn 조용한_시간_밖이면_하루_정리도_보내고_경계는_시작_포함_끝_제외다() {
    let env = env_with(Some(night_quiet()), 2_000).await;
    let sub = env.subscribe(1);
    for (hour, minute, held) in [
        (12, 0, false),
        (21, 59, false),
        (22, 0, true),
        (6, 59, true),
        (7, 0, false),
    ] {
        let at = kst(hour, minute);
        env.clock.set(at);
        let delivery = env
            .notifier
            .notify(notice(NoticeKind::DailySummary, at))
            .await
            .unwrap();
        assert_eq!(delivery.held.is_some(), held, "{hour}:{minute}");
    }
    assert_eq!(env.push.received_at(&path_of(&sub)).len(), 3);
}

// 검증: V12
#[tokio::test]
async fn 조용한_시간을_고치고_없애면_바로_따른다() {
    let env = env().await;
    let sub = env.subscribe(1);
    let night = kst(23, 0);
    env.clock.set(night);

    env.notifier.set_quiet_hours(Some(night_quiet()));
    let held = env
        .notifier
        .notify(notice(NoticeKind::DailySummary, night))
        .await
        .unwrap();
    assert_eq!(held.held, Some(HoldReason::QuietHours));

    env.notifier.set_quiet_hours(None);
    let sent = env
        .notifier
        .notify(notice(NoticeKind::DailySummary, night))
        .await
        .unwrap();
    assert_eq!(sent.held, None);
    assert_eq!(env.push.received_at(&path_of(&sub)).len(), 1);
}
