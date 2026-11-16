//! 푸시의 `link` 상한(notify 2차): 아주 긴 주소 때문에 한 레코드를 넘어 모든 보내기가 실패하지 않는다.

use pi_notify::{
    AttemptOutcome, MAX_PLAINTEXT, MAX_PUSH_BODY_CHARS, MAX_PUSH_LINK_CHARS, MAX_PUSH_TITLE_CHARS,
    NewNotice, NoticeKind,
};
use pi_store::EventRange;

use crate::common::{Env, T0, env, notice, open_json, path_of};

async fn send_with_link(env: &Env, link: &str) -> (AttemptOutcome, serde_json::Value, u64) {
    let sub = env.subscribe(1);
    let delivery = env
        .notifier
        .notify(NewNotice {
            link: Some(link.to_owned()),
            ..notice(NoticeKind::ApprovalRequest, T0)
        })
        .await
        .unwrap();
    let outcome = delivery.attempts[0].outcome.clone();
    let got = env.push.received_at(&path_of(&sub));
    let json = got
        .first()
        .map_or(serde_json::Value::Null, |r| open_json(&sub, r));
    (outcome, json, delivery.seq)
}

// 검증: N5 V10
#[tokio::test]
async fn 상한_길이의_주소는_그대로_싣는다() {
    let env = env().await;
    let link = format!("/approvals/{}", "a".repeat(MAX_PUSH_LINK_CHARS - 11));
    assert_eq!(link.chars().count(), MAX_PUSH_LINK_CHARS);
    let (outcome, json, _) = send_with_link(&env, &link).await;
    assert_eq!(outcome, AttemptOutcome::Delivered { status: 201 });
    assert_eq!(json["link"], link.as_str());
}

// 검증: N5 V10
#[tokio::test]
async fn 상한보다_긴_주소는_푸시에_싣지_않고_이벤트에는_전체가_남는다() {
    let env = env().await;
    let link = format!("/approvals/{}", "가".repeat(MAX_PUSH_LINK_CHARS - 10));
    assert_eq!(link.chars().count(), MAX_PUSH_LINK_CHARS + 1);
    let (outcome, json, seq) = send_with_link(&env, &link).await;
    assert_eq!(outcome, AttemptOutcome::Delivered { status: 201 });
    assert!(json.get("link").is_none(), "반쯤 자른 주소를 싣지 않는다");
    assert_eq!(json["seq"], seq);
    let event = &env.store.read_events(EventRange::default(), 10).unwrap()[0];
    assert_eq!(event.payload["link"], link.as_str());
}

// 검증: N5
#[tokio::test]
async fn 아주_긴_주소여도_보내기가_실패하지_않는다() {
    let env = env().await;
    let link = format!("/approvals/{}", "x".repeat(10_000));
    let (outcome, json, _) = send_with_link(&env, &link).await;
    assert_eq!(outcome, AttemptOutcome::Delivered { status: 201 });
    assert!(json.get("link").is_none());
}

// 검증: N5
#[tokio::test]
async fn 제목_본문_주소가_모두_상한이고_이스케이프가_가장_길어도_한_레코드에_들어간다() {
    let env = env().await;
    let sub = env.subscribe(1);
    // 제어 글자는 JSON에서 \u0001처럼 6바이트가 된다.
    let worst = |n: usize| "\u{1}".repeat(n);
    let delivery = env
        .notifier
        .notify(NewNotice {
            kind: NoticeKind::ApprovalRequest,
            title: worst(MAX_PUSH_TITLE_CHARS),
            body: worst(MAX_PUSH_BODY_CHARS),
            link: Some(worst(MAX_PUSH_LINK_CHARS)),
            occurred_at: pi_store::UnixMillis(T0),
        })
        .await
        .unwrap();
    assert_eq!(
        delivery.attempts[0].outcome,
        AttemptOutcome::Delivered { status: 201 }
    );
    let got = env.push.received_at(&path_of(&sub));
    let plain = sub.open(&got[0].body);
    assert!(plain.len() <= MAX_PLAINTEXT);
    let json: serde_json::Value = serde_json::from_str(&plain).unwrap();
    assert_eq!(json["link"], worst(MAX_PUSH_LINK_CHARS).as_str());
}
