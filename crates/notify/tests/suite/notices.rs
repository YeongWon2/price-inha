//! 알림 목록(폰 화면의 목록이 읽는다).

use pi_notify::{NewNotice, NoticeKind};
use pi_store::UnixMillis;
use serde_json::json;

use crate::common::{SEC, T0, env, notice};

// 검증: N5 E2 V10 V11 V12
#[tokio::test]
async fn 여섯_종류_모두_알림_목록에_번호_순서로_나온다() {
    let env = env().await;
    for (i, kind) in NoticeKind::ALL.into_iter().enumerate() {
        env.notifier
            .notify(notice(kind, T0 + i as i64 * SEC))
            .await
            .unwrap();
    }
    let list = env.notifier.notices_after(0, 100).unwrap();
    let kinds: Vec<NoticeKind> = list.iter().map(|n| n.kind).collect();
    assert_eq!(kinds, NoticeKind::ALL);
    let seqs: Vec<u64> = list.iter().map(|n| n.seq).collect();
    assert!(seqs.windows(2).all(|w| w[0] < w[1]));
    assert_eq!(list[2].at, Some(UnixMillis(T0 + 2 * SEC)));
    assert_eq!(list[0].title, "제목");
    assert_eq!(list[0].body, "본문");
    assert_eq!(list[0].link, None);
}

// 검증: N5
#[tokio::test]
async fn 번호_뒤부터_개수만큼만_준다() {
    let env = env().await;
    let mut seqs = Vec::new();
    for _ in 0..5 {
        seqs.push(
            env.notifier
                .notify(notice(NoticeKind::Stall, T0))
                .await
                .unwrap()
                .seq,
        );
    }
    let page = env.notifier.notices_after(seqs[1], 2).unwrap();
    assert_eq!(
        page.iter().map(|n| n.seq).collect::<Vec<_>>(),
        [seqs[2], seqs[3]]
    );
    assert!(env.notifier.notices_after(seqs[4], 10).unwrap().is_empty());
    assert!(env.notifier.notices_after(0, 0).unwrap().is_empty());
}

// 검증: N5
#[tokio::test]
async fn 알림이_아닌_이벤트는_건너뛰고_개수는_채운다() {
    let env = env().await;
    let a = env
        .notifier
        .notify(notice(NoticeKind::Stall, T0))
        .await
        .unwrap()
        .seq;
    env.store.append("chat", &json!({"text": "잡담"})).unwrap();
    env.store
        .append("approval.notice", &json!({"x": 1}))
        .unwrap();
    let b = env
        .notifier
        .notify(notice(NoticeKind::Anomaly, T0))
        .await
        .unwrap()
        .seq;
    env.store.append("chat", &json!({})).unwrap();
    let c = env
        .notifier
        .notify(NewNotice {
            link: Some("/a".to_owned()),
            ..notice(NoticeKind::AccessLock, T0)
        })
        .await
        .unwrap()
        .seq;

    let list = env.notifier.notices_after(0, 3).unwrap();
    assert_eq!(list.iter().map(|n| n.seq).collect::<Vec<_>>(), [a, b, c]);
    assert_eq!(list[2].link.as_deref(), Some("/a"));
    let from_middle = env.notifier.notices_after(a, 1).unwrap();
    assert_eq!(from_middle.len(), 1);
    assert_eq!(from_middle[0].seq, b);
}
