//! 조용한 시간에 미룬 푸시를 끝난 뒤 다시 보낸다(notify 4차).
//! 미룬 표시는 번호 있는 이벤트로 남아 다시 켜도 이어진다. 알림 목록에는 나오지 않는다.

use pi_notify::{AttemptOutcome, HELD_RELEASE_WINDOW_MS, HoldReason, NoticeKind};

use crate::common::{build_notifier, env_with, kst, night_quiet, notice, open_json, path_of};

// 검증: V12
#[tokio::test]
async fn 조용한_시간이_끝나면_미룬_푸시를_한_번씩_보내고_다시는_보내지_않는다() {
    let env = env_with(Some(night_quiet()), 2_000).await;
    let sub = env.subscribe(1);
    env.clock.set(kst(23, 30));
    let mut held = Vec::new();
    for kind in [NoticeKind::DailySummary, NoticeKind::BackupFailure] {
        let delivery = env
            .notifier
            .notify(notice(kind, kst(23, 30)))
            .await
            .unwrap();
        assert_eq!(delivery.held, Some(HoldReason::QuietHours));
        held.push((kind, delivery.seq));
    }
    assert!(env.push.received_at(&path_of(&sub)).is_empty());

    env.clock.set(kst(7, 0) + 24 * 3_600 * 1_000);
    let released = env.notifier.release_held().await.unwrap();
    assert_eq!(released.len(), 2);
    assert_eq!(
        released.iter().map(|d| d.seq).collect::<Vec<_>>(),
        held.iter().map(|h| h.1).collect::<Vec<_>>(),
        "번호 순서로 보낸다"
    );
    for delivery in &released {
        assert_eq!(delivery.held, None);
        assert_eq!(
            delivery.attempts[0].outcome,
            AttemptOutcome::Delivered { status: 201 }
        );
        assert!(delivery.first_attempt_at.is_some());
    }
    let received = env.push.received_at(&path_of(&sub));
    assert_eq!(received.len(), 2);
    for (got, (kind, seq)) in received.iter().zip(&held) {
        let json = open_json(&sub, got);
        assert_eq!(json["kind"], kind.name());
        assert_eq!(json["seq"], *seq);
        assert_eq!(json["title"], "제목");
        assert_eq!(json["body"], "본문");
    }

    assert!(env.notifier.release_held().await.unwrap().is_empty());
    assert_eq!(env.push.received_at(&path_of(&sub)).len(), 2);
}

// 검증: V12
#[tokio::test]
async fn 아직_조용한_시간이면_보내지_않고_미룬_것을_그대로_둔다() {
    let env = env_with(Some(night_quiet()), 2_000).await;
    let sub = env.subscribe(1);
    env.clock.set(kst(23, 0));
    env.notifier
        .notify(notice(NoticeKind::DailySummary, kst(23, 0)))
        .await
        .unwrap();

    env.clock.set(kst(23, 50));
    assert!(env.notifier.release_held().await.unwrap().is_empty());
    assert!(env.push.received_at(&path_of(&sub)).is_empty());

    env.clock.set(kst(7, 30) + 24 * 3_600 * 1_000);
    assert_eq!(env.notifier.release_held().await.unwrap().len(), 1);
    assert_eq!(env.push.received_at(&path_of(&sub)).len(), 1);
}

// 검증: V12
#[tokio::test]
async fn 미루지_않고_이미_보낸_알림은_다시_보내지_않는다() {
    let env = env_with(Some(night_quiet()), 2_000).await;
    let sub = env.subscribe(1);
    env.clock.set(kst(12, 0));
    env.notifier
        .notify(notice(NoticeKind::DailySummary, kst(12, 0)))
        .await
        .unwrap();
    env.clock.set(kst(3, 0));
    env.notifier
        .notify(notice(NoticeKind::ApprovalRequest, kst(3, 0)))
        .await
        .unwrap();
    assert_eq!(env.push.received_at(&path_of(&sub)).len(), 2);

    env.clock.set(kst(8, 0) + 24 * 3_600 * 1_000);
    assert!(env.notifier.release_held().await.unwrap().is_empty());
    assert_eq!(env.push.received_at(&path_of(&sub)).len(), 2);
}

// 검증: V12
#[tokio::test]
async fn 기한을_넘긴_미룬_알림은_보내지_않는다() {
    let env = env_with(Some(night_quiet()), 2_000).await;
    let sub = env.subscribe(1);
    env.clock.set(kst(23, 0));
    env.notifier
        .notify(notice(NoticeKind::BackupFailure, kst(23, 0)))
        .await
        .unwrap();

    env.clock.set(kst(23, 0) + HELD_RELEASE_WINDOW_MS + 1);
    assert!(env.notifier.release_held().await.unwrap().is_empty());
    assert!(env.push.received_at(&path_of(&sub)).is_empty());
}

// 검증: V12
#[tokio::test]
async fn 미룬_표시는_다시_켜도_이어지고_알림_목록에는_섞이지_않는다() {
    let env = env_with(Some(night_quiet()), 2_000).await;
    let sub = env.subscribe(1);
    env.clock.set(kst(23, 0));
    let seq = env
        .notifier
        .notify(notice(NoticeKind::DailySummary, kst(23, 0)))
        .await
        .unwrap()
        .seq;
    let list = env.notifier.notices_after(0, 10).unwrap();
    assert_eq!(list.iter().map(|n| n.seq).collect::<Vec<_>>(), [seq]);

    let again = build_notifier(
        &env.store,
        &env.clock,
        env.push.base(),
        Some(night_quiet()),
        2_000,
    );
    env.clock.set(kst(8, 0) + 24 * 3_600 * 1_000);
    let released = again.release_held().await.unwrap();
    assert_eq!(released.len(), 1);
    assert_eq!(released[0].seq, seq);
    assert_eq!(env.push.received_at(&path_of(&sub)).len(), 1);
    assert!(again.release_held().await.unwrap().is_empty());
    assert_eq!(again.notices_after(0, 10).unwrap().len(), 1);
}
