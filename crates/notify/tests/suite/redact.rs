//! 보내기 전에 가린다(E38). 푸시 · 이벤트 · 알림 목록 어디에도 비밀 글자가 없다.

use pi_notify::{NewNotice, NoticeKind};
use pi_store::{EventRange, UnixMillis};

use crate::common::{SECRET_TEXT, T0, env, notice, open_json, path_of};

fn secret_notice() -> NewNotice {
    NewNotice {
        kind: NoticeKind::BackupFailure,
        title: format!("제목 {SECRET_TEXT}"),
        body: format!("본문 {SECRET_TEXT} 끝"),
        link: Some(format!("/x/{SECRET_TEXT}")),
        occurred_at: UnixMillis(T0),
    }
}

// 검증: E38
#[tokio::test]
async fn 푸시에_나가는_글에서_비밀_값이_가려진다() {
    let env = env().await;
    let sub = env.subscribe(1);
    env.notifier.notify(secret_notice()).await.unwrap();
    let json = open_json(&sub, &env.push.received_at(&path_of(&sub))[0]);
    assert_eq!(json["title"], "제목 ●●●");
    assert_eq!(json["body"], "본문 ●●● 끝");
    assert_eq!(json["link"], "/x/●●●");
    assert!(!json.to_string().contains("ZQ7"));
}

// 검증: E38
#[tokio::test]
async fn 기록되는_이벤트에도_가린_글만_남는다() {
    let env = env().await;
    env.notifier.notify(secret_notice()).await.unwrap();
    let events = env.store.read_events(EventRange::default(), 10).unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].payload["title"], "제목 ●●●");
    assert_eq!(events[0].payload["body"], "본문 ●●● 끝");
    assert_eq!(events[0].payload["link"], "/x/●●●");
    assert!(!events[0].payload.to_string().contains("ZQ7"));
}

// 검증: E38
#[tokio::test]
async fn 알림_목록에도_가린_글이_나온다() {
    let env = env().await;
    env.notifier.notify(secret_notice()).await.unwrap();
    let list = env.notifier.notices_after(0, 10).unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].title, "제목 ●●●");
    assert_eq!(list[0].body, "본문 ●●● 끝");
    assert_eq!(list[0].link.as_deref(), Some("/x/●●●"));
}

// 검증: E38
#[tokio::test]
async fn 자르기_전에_가려서_잘린_자리에_비밀_일부가_남지_않는다() {
    let env = env().await;
    let sub = env.subscribe(1);
    // 가린 뒤 201글자. 상한 200글자에서 잘린다. 자르고 나서 가리면 비밀 앞부분이 남는다.
    let body = format!("{}{}", "가".repeat(198), SECRET_TEXT);
    env.notifier
        .notify(NewNotice {
            body,
            ..notice(NoticeKind::Stall, T0)
        })
        .await
        .unwrap();
    let json = open_json(&sub, &env.push.received_at(&path_of(&sub))[0]);
    let pushed = json["body"].as_str().unwrap();
    assert!(pushed.starts_with(&"가".repeat(198)));
    assert!(!pushed.contains('Z') && !pushed.contains('Q') && !pushed.contains('7'));
    assert!(!pushed.contains('비'));
    assert_eq!(pushed.chars().count(), 200);
}

// 검증: E38
#[tokio::test]
async fn 가릴_것이_없으면_글이_그대로다() {
    let env = env().await;
    let sub = env.subscribe(1);
    env.notifier
        .notify(NewNotice {
            title: "그대로".to_owned(),
            body: "손대지 않는다".to_owned(),
            ..notice(NoticeKind::Stall, T0)
        })
        .await
        .unwrap();
    let json = open_json(&sub, &env.push.received_at(&path_of(&sub))[0]);
    assert_eq!(json["title"], "그대로");
    assert_eq!(json["body"], "손대지 않는다");
}
