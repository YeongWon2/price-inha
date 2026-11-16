//! 보내기가 실패하는 경우: 구독 지우기(404 · 410), 그 밖의 실패, 느린 구독, 제한 시간.

use std::time::Duration;

use pi_notify::{AttemptOutcome, NoticeKind};

use crate::common::{Behavior, T0, env, env_with, notice, path_of};

// 검증: N5
#[tokio::test]
async fn 구독이_사라졌다는_404와_410은_그_구독만_지운다() {
    let env = env().await;
    let gone404 = env.subscribe(1);
    let gone410 = env.subscribe(2);
    let alive = env.subscribe(3);
    env.push.behave(&path_of(&gone404), Behavior::Status(404));
    env.push.behave(&path_of(&gone410), Behavior::Status(410));

    let delivery = env
        .notifier
        .notify(notice(NoticeKind::ApprovalRequest, T0))
        .await
        .unwrap();
    assert_eq!(
        delivery.attempts[0].outcome,
        AttemptOutcome::SubscriptionGone { status: 404 }
    );
    assert_eq!(
        delivery.attempts[1].outcome,
        AttemptOutcome::SubscriptionGone { status: 410 }
    );
    assert_eq!(
        delivery.attempts[2].outcome,
        AttemptOutcome::Delivered { status: 201 }
    );
    let left: Vec<String> = env
        .store
        .push_subscriptions()
        .unwrap()
        .into_iter()
        .map(|s| s.endpoint)
        .collect();
    assert_eq!(left.len(), 1);
    assert_eq!(left[0], alive.endpoint);

    // 지운 구독으로는 다시 보내지 않는다.
    let again = env
        .notifier
        .notify(notice(NoticeKind::ApprovalRequest, T0))
        .await
        .unwrap();
    assert_eq!(again.attempts.len(), 1);
    assert_eq!(env.push.received_at(&path_of(&gone404)).len(), 1);
    assert_eq!(env.push.received_at(&path_of(&gone410)).len(), 1);
}

// 검증: N5
#[tokio::test]
async fn 그_밖의_실패_응답은_구독을_지우지_않는다() {
    let env = env().await;
    let statuses = [400u16, 401, 403, 413, 429, 500, 503];
    let subs: Vec<_> = (1u8..)
        .zip(statuses)
        .map(|(seed, status)| {
            let sub = env.subscribe(seed);
            env.push.behave(&path_of(&sub), Behavior::Status(status));
            (sub, status)
        })
        .collect();
    let delivery = env
        .notifier
        .notify(notice(NoticeKind::Stall, T0))
        .await
        .unwrap();
    for ((_, status), attempt) in subs.iter().zip(&delivery.attempts) {
        assert_eq!(
            attempt.outcome,
            AttemptOutcome::Rejected { status: *status }
        );
    }
    assert_eq!(
        env.store.push_subscriptions().unwrap().len(),
        statuses.len()
    );
}

// 검증: N5
#[tokio::test]
async fn 성공은_201뿐_아니라_200과_202도_받았다고_본다() {
    let env = env().await;
    let a = env.subscribe(1);
    let b = env.subscribe(2);
    env.push.behave(&path_of(&a), Behavior::Status(200));
    env.push.behave(&path_of(&b), Behavior::Status(202));
    let delivery = env
        .notifier
        .notify(notice(NoticeKind::Stall, T0))
        .await
        .unwrap();
    assert_eq!(
        delivery.attempts[0].outcome,
        AttemptOutcome::Delivered { status: 200 }
    );
    assert_eq!(
        delivery.attempts[1].outcome,
        AttemptOutcome::Delivered { status: 202 }
    );
}

// 검증: N5 V11
#[tokio::test]
async fn 응답이_없는_구독은_제한_시간_뒤_시간_초과이고_구독은_남는다() {
    let env = env_with(None, 200).await;
    let hung = env.subscribe(1);
    env.push.behave(&path_of(&hung), Behavior::Hang);
    let delivery = tokio::time::timeout(
        Duration::from_secs(5),
        env.notifier.notify(notice(NoticeKind::Anomaly, T0)),
    )
    .await
    .expect("제한 시간이 지나면 돌아와야 한다")
    .unwrap();
    assert_eq!(delivery.attempts[0].outcome, AttemptOutcome::TimedOut);
    assert_eq!(env.store.push_subscriptions().unwrap().len(), 1);
}

// 검증: N5 V10 V11
#[tokio::test]
async fn 느린_구독이_다른_구독의_첫_시도를_늦추지_않는다() {
    let env = env_with(None, 600).await;
    let slow = env.subscribe(1);
    let fast = env.subscribe(2);
    env.push.behave(&path_of(&slow), Behavior::Hang);

    let notifier = env.notifier.clone();
    let task = tokio::spawn(async move {
        notifier
            .notify(notice(NoticeKind::ApprovalRequest, T0))
            .await
    });
    // 느린 쪽이 제한 시간(600ms)을 채우기 전에 빠른 쪽에는 이미 닿았다.
    let fast_path = path_of(&fast);
    let arrived = tokio::time::timeout(Duration::from_millis(300), async {
        loop {
            if !env.push.received_at(&fast_path).is_empty() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await;
    assert!(arrived.is_ok(), "빠른 구독이 느린 구독을 기다렸다");

    let delivery = task.await.unwrap().unwrap();
    assert_eq!(delivery.attempts[0].outcome, AttemptOutcome::TimedOut);
    assert_eq!(
        delivery.attempts[1].outcome,
        AttemptOutcome::Delivered { status: 201 }
    );
}

// 검증: N5
#[tokio::test]
async fn 서버가_없는_주소로_보내면_실패로_알리고_구독은_남긴다() {
    let env = env().await;
    env.subscribe(1);
    // 가짜 서버를 닫은 것처럼 아무도 듣지 않는 포트로 새 알림 보내기를 만든다.
    let dead = {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        url::Url::parse(&url).unwrap()
    };
    let notifier = crate::common::build_notifier(&env.store, &env.clock, dead, None, 2_000);
    let delivery = notifier
        .notify(notice(NoticeKind::Stall, T0))
        .await
        .unwrap();
    assert!(matches!(
        delivery.attempts[0].outcome,
        AttemptOutcome::Failed(_)
    ));
    assert_eq!(env.store.push_subscriptions().unwrap().len(), 1);
    assert!(delivery.seq > 0, "푸시가 실패해도 이벤트는 남는다");
}
