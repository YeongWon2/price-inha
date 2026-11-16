//! 구독 두기 · 지우기, 다시 켠 뒤에도 이어서 보내기.

use pi_notify::{Error, NewSubscription, NoticeKind};
use pi_store::{Store, StoreError};

use crate::common::{SEC, Subscriber, T0, build_notifier, env, notice, open_json, path_of};

// 검증: N5
#[tokio::test]
async fn 구독을_두면_저장소_표에_시각과_이름표가_남는다() {
    let env = env().await;
    let sub = Subscriber::new(1);
    let saved = env.notifier.subscribe(sub.new_subscription()).unwrap();
    assert_eq!(saved.endpoint, sub.endpoint);
    assert_eq!(saved.device, "시험 폰");
    assert_eq!(saved.created_at.0, T0);
    assert_eq!(env.store.push_subscriptions().unwrap(), [saved]);
}

// 검증: N5
#[tokio::test]
async fn 같은_주소를_다시_두면_한_줄이고_갱신_시각만_바뀐다() {
    let env = env().await;
    let sub = Subscriber::new(1);
    env.notifier.subscribe(sub.new_subscription()).unwrap();
    env.clock.set(T0 + 60 * SEC);
    let again = env
        .notifier
        .subscribe(NewSubscription {
            device: "새 폰".to_owned(),
            ..sub.new_subscription()
        })
        .unwrap();
    assert_eq!(again.created_at.0, T0);
    assert_eq!(again.updated_at.0, T0 + 60 * SEC);
    assert_eq!(again.device, "새 폰");
    assert_eq!(env.store.push_subscriptions().unwrap().len(), 1);
}

// 검증: N5
#[tokio::test]
async fn 키_모양이_틀린_구독은_받지_않고_아무것도_두지_않는다() {
    let env = env().await;
    let good = Subscriber::new(1).new_subscription();
    let bad: Vec<NewSubscription> = vec![
        NewSubscription {
            p256dh: "AAAA".to_owned(),
            ..good.clone()
        },
        NewSubscription {
            p256dh: "***not base64***".to_owned(),
            ..good.clone()
        },
        // 65바이트지만 곡선 위의 점이 아니다.
        NewSubscription {
            p256dh: base64::Engine::encode(
                &base64::engine::general_purpose::URL_SAFE_NO_PAD,
                [vec![4u8], vec![0xFF; 64]].concat(),
            ),
            ..good.clone()
        },
        NewSubscription {
            auth: "AAAA".to_owned(),
            ..good.clone()
        },
        NewSubscription {
            auth: String::new(),
            ..good.clone()
        },
        NewSubscription {
            p256dh: String::new(),
            ..good.clone()
        },
    ];
    for new in bad {
        assert!(env.notifier.subscribe(new).is_err());
    }
    assert!(env.store.push_subscriptions().unwrap().is_empty());
}

// 검증: N5
#[tokio::test]
async fn 주소가_https가_아니면_저장소가_거절한다() {
    let env = env().await;
    let new = NewSubscription {
        endpoint: "http://push.invalid/x".to_owned(),
        ..Subscriber::new(1).new_subscription()
    };
    assert!(matches!(
        env.notifier.subscribe(new),
        Err(Error::Store(StoreError::InvalidPushSubscription))
    ));
    assert!(env.store.push_subscriptions().unwrap().is_empty());
}

// 검증: N5
#[tokio::test]
async fn 구독을_지우면_더_보내지_않고_없는_것을_지우면_거짓이다() {
    let env = env().await;
    let sub = env.subscribe(1);
    assert!(env.notifier.unsubscribe(&sub.endpoint).unwrap());
    assert!(!env.notifier.unsubscribe(&sub.endpoint).unwrap());
    let delivery = env
        .notifier
        .notify(notice(NoticeKind::Stall, T0))
        .await
        .unwrap();
    assert!(delivery.attempts.is_empty());
    assert!(env.push.received().is_empty());
}

// 검증: N5
#[tokio::test]
async fn 브라우저에_줄_공개키는_65바이트_점의_base64url이다() {
    let env = env().await;
    let text = env.notifier.vapid_public_key();
    let bytes =
        base64::Engine::decode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, &text).unwrap();
    assert_eq!(bytes.len(), 65);
    assert_eq!(bytes[0], 4);
}

// 검증: N5 V10
#[tokio::test]
async fn 다시_켜도_구독이_남아_이어서_보내고_번호가_이어진다() {
    let env = env().await;
    let sub = env.subscribe(1);
    let before = env
        .notifier
        .notify(notice(NoticeKind::Stall, T0))
        .await
        .unwrap();
    let path = env.dir.path().join("pi.sqlite");
    let base = env.push.base();
    let clock = env.clock.clone();
    drop(env.notifier);
    drop(env.store);

    let reopened = Store::open(&path).unwrap();
    let notifier = build_notifier(&reopened, &clock, base, None, 2_000);
    let after = notifier
        .notify(notice(NoticeKind::ApprovalRequest, T0))
        .await
        .unwrap();
    assert_eq!(after.seq, before.seq + 1);
    assert_eq!(after.attempts.len(), 1);
    let got = env.push.received_at(&path_of(&sub));
    assert_eq!(got.len(), 2);
    assert_eq!(open_json(&sub, &got[1])["kind"], "approval_request");
}
