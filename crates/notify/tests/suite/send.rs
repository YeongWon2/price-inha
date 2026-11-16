//! 보내기: 번호 있는 이벤트 + 웹 푸시, 10초 안 첫 시도(N5 · V10 · V11 · E2).

use pi_notify::{
    AttemptOutcome, FIRST_ATTEMPT_WITHIN_MS, MAX_PUSH_BODY_CHARS, MAX_PUSH_TITLE_CHARS, NewNotice,
    NoticeKind,
};
use pi_store::{EventRange, UnixMillis};

use crate::common::{PUSH_ORIGIN, SEC, SUBJECT, T0, check_vapid, env, notice, open_json, path_of};

// 검증: N5 V10
#[tokio::test]
async fn 승인_요청을_보내면_구독자가_풀어_읽는다() {
    let env = env().await;
    let sub = env.subscribe(1);
    let delivery = env
        .notifier
        .notify(NewNotice {
            kind: NoticeKind::ApprovalRequest,
            title: "승인이 필요해요".to_owned(),
            body: "삼성전자 10주 매수".to_owned(),
            link: Some("/approvals/abc".to_owned()),
            occurred_at: UnixMillis(T0),
        })
        .await
        .unwrap();
    assert_eq!(delivery.attempts.len(), 1);
    assert_eq!(
        delivery.attempts[0].outcome,
        AttemptOutcome::Delivered { status: 201 }
    );
    assert_eq!(delivery.attempts[0].endpoint, sub.endpoint);

    let got = env.push.received_at(&path_of(&sub));
    assert_eq!(got.len(), 1);
    let json = open_json(&sub, &got[0]);
    assert_eq!(json["kind"], "approval_request");
    assert_eq!(json["title"], "승인이 필요해요");
    assert_eq!(json["body"], "삼성전자 10주 매수");
    assert_eq!(json["link"], "/approvals/abc");
    assert_eq!(json["seq"], delivery.seq);
    // 푸시에는 필요한 칸만 싣는다.
    assert_eq!(json.as_object().unwrap().len(), 5);
}

// 검증: N5
#[tokio::test]
async fn 주소가_없으면_link_칸이_없다() {
    let env = env().await;
    let sub = env.subscribe(1);
    env.notifier
        .notify(notice(NoticeKind::Stall, T0))
        .await
        .unwrap();
    let json = open_json(&sub, &env.push.received_at(&path_of(&sub))[0]);
    assert!(json.get("link").is_none());
    assert_eq!(json.as_object().unwrap().len(), 4);
}

// 검증: N5 V10
#[tokio::test]
async fn 머리글에_암호화_형식과_ttl과_긴급도와_vapid_서명이_있다() {
    let env = env().await;
    let sub = env.subscribe(1);
    env.notifier
        .notify(notice(NoticeKind::ApprovalRequest, T0))
        .await
        .unwrap();
    let got = &env.push.received_at(&path_of(&sub))[0];
    assert_eq!(got.headers["content-encoding"], "aes128gcm");
    assert_eq!(got.headers["content-type"], "application/octet-stream");
    assert_eq!(got.headers["urgency"], "high");
    let ttl: u64 = got.headers["ttl"].parse().unwrap();
    assert!(ttl > 0);
    let claims = check_vapid(
        &got.headers["authorization"],
        &env.notifier.vapid_public_key(),
    );
    assert_eq!(claims.aud, PUSH_ORIGIN);
    assert_eq!(claims.sub, SUBJECT);
    assert!(claims.exp > T0 / SEC && claims.exp <= T0 / SEC + 24 * 3_600);
}

// 검증: V12
#[tokio::test]
async fn 하루_정리는_높은_긴급도로_보내지_않는다() {
    let env = env().await;
    let sub = env.subscribe(1);
    env.notifier
        .notify(notice(NoticeKind::DailySummary, T0))
        .await
        .unwrap();
    let got = &env.push.received_at(&path_of(&sub))[0];
    assert_ne!(got.headers["urgency"], "high");
    assert_eq!(open_json(&sub, got)["kind"], "daily_summary");
}

// 검증: N5
#[tokio::test]
async fn 이벤트가_번호와_시각과_함께_쌓이고_내용은_가린_전체다() {
    let env = env().await;
    env.subscribe(1);
    let first = env
        .notifier
        .notify(NewNotice {
            link: Some("/x".to_owned()),
            ..notice(NoticeKind::BackupFailure, T0)
        })
        .await
        .unwrap();
    let second = env
        .notifier
        .notify(notice(NoticeKind::Anomaly, T0 + 5 * SEC))
        .await
        .unwrap();
    assert_eq!(second.seq, first.seq + 1);

    let events = env.store.read_events(EventRange::default(), 10).unwrap();
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].seq, first.seq);
    assert_eq!(events[0].kind, "notice.backup_failure");
    assert_eq!(events[0].at, Some(UnixMillis(T0)));
    assert_eq!(
        events[0].payload,
        serde_json::json!({"title": "제목", "body": "본문", "link": "/x"})
    );
    assert_eq!(events[1].kind, "notice.anomaly");
    assert_eq!(events[1].at, Some(UnixMillis(T0 + 5 * SEC)));
    assert_eq!(
        events[1].payload,
        serde_json::json!({"title": "제목", "body": "본문"})
    );
}

// 검증: N5
#[tokio::test]
async fn 구독이_없어도_이벤트는_쌓이고_시도는_없다() {
    let env = env().await;
    let delivery = env
        .notifier
        .notify(notice(NoticeKind::Stall, T0))
        .await
        .unwrap();
    assert!(delivery.seq > 0);
    assert!(delivery.attempts.is_empty());
    assert_eq!(delivery.first_attempt_at, None);
    assert!(!delivery.within_deadline(UnixMillis(T0)));
    assert!(env.push.received().is_empty());
    assert_eq!(env.store.read_after(0, 10).unwrap().len(), 1);
}

// 검증: N5 V10
#[tokio::test]
async fn 구독이_여럿이면_모두에게_가고_각자_자기_키로_푼다() {
    let env = env().await;
    let subs = [env.subscribe(1), env.subscribe(2), env.subscribe(3)];
    let delivery = env
        .notifier
        .notify(notice(NoticeKind::ApprovalRequest, T0))
        .await
        .unwrap();
    assert_eq!(delivery.attempts.len(), 3);
    for (sub, attempt) in subs.iter().zip(&delivery.attempts) {
        assert_eq!(attempt.endpoint, sub.endpoint);
        let got = env.push.received_at(&path_of(sub));
        assert_eq!(got.len(), 1);
        assert_eq!(open_json(sub, &got[0])["kind"], "approval_request");
    }
    // 다른 사람의 키로는 풀리지 않게 각자 다른 암호문이다.
    let a = env.push.received_at(&path_of(&subs[0]))[0].body.clone();
    let b = env.push.received_at(&path_of(&subs[1]))[0].body.clone();
    assert_ne!(a, b);
}

// 검증: N5 V11
#[tokio::test]
async fn 첫_시도는_시계의_지금이고_일이_생긴_뒤_10초_안이면_기한을_지킨다() {
    assert_eq!(FIRST_ATTEMPT_WITHIN_MS, 10_000);
    let env = env().await;
    env.subscribe(1);
    env.clock.set(T0 + 2 * SEC);
    let delivery = env
        .notifier
        .notify(notice(NoticeKind::Anomaly, T0))
        .await
        .unwrap();
    assert_eq!(delivery.first_attempt_at, Some(UnixMillis(T0 + 2 * SEC)));
    assert!(delivery.within_deadline(UnixMillis(T0)));
}

// 검증: N5 V11
#[tokio::test]
async fn 늦게_알려도_바로_보내고_기한을_넘겼다고_알려_준다() {
    let env = env().await;
    let sub = env.subscribe(1);
    env.clock.set(T0 + 15 * SEC);
    let delivery = env
        .notifier
        .notify(notice(NoticeKind::Anomaly, T0))
        .await
        .unwrap();
    assert_eq!(env.push.received_at(&path_of(&sub)).len(), 1);
    assert_eq!(delivery.first_attempt_at, Some(UnixMillis(T0 + 15 * SEC)));
    assert!(!delivery.within_deadline(UnixMillis(T0)));
    // 딱 10초는 기한 안이다.
    env.clock.set(T0 + 10 * SEC);
    let on_time = env
        .notifier
        .notify(notice(NoticeKind::Anomaly, T0))
        .await
        .unwrap();
    assert!(on_time.within_deadline(UnixMillis(T0)));
}

// 검증: V11
#[tokio::test]
async fn 시세가_끊긴_이상_알림이_바로_폰에_온다() {
    let env = env().await;
    let sub = env.subscribe(1);
    let delivery = env
        .notifier
        .notify(NewNotice {
            kind: NoticeKind::Anomaly,
            title: "시세가 끊겼어요".to_owned(),
            body: "시세를 받지 못하고 있어요".to_owned(),
            link: None,
            occurred_at: UnixMillis(T0),
        })
        .await
        .unwrap();
    assert!(delivery.within_deadline(UnixMillis(T0)));
    let json = open_json(&sub, &env.push.received_at(&path_of(&sub))[0]);
    assert_eq!(json["kind"], "anomaly");
    assert_eq!(json["title"], "시세가 끊겼어요");
}

// 검증: E2
#[tokio::test]
async fn 접속_잠금_알림이_이벤트와_푸시로_나간다() {
    let env = env().await;
    let sub = env.subscribe(1);
    let delivery = env
        .notifier
        .notify(NewNotice {
            kind: NoticeKind::AccessLock,
            title: "접속이 잠겼어요".to_owned(),
            body: "접속 키를 여러 번 틀렸어요".to_owned(),
            link: None,
            occurred_at: UnixMillis(T0),
        })
        .await
        .unwrap();
    assert!(delivery.within_deadline(UnixMillis(T0)));
    let events = env.store.read_events(EventRange::default(), 10).unwrap();
    assert_eq!(events[0].kind, "notice.access_lock");
    assert_eq!(
        open_json(&sub, &env.push.received_at(&path_of(&sub))[0])["kind"],
        "access_lock"
    );
}

// 검증: N5
#[tokio::test]
async fn 푸시에는_제목과_본문을_상한까지만_싣고_이벤트에는_전체가_남는다() {
    let env = env().await;
    let sub = env.subscribe(1);
    let long_title = "제".repeat(500);
    let long_body = "본".repeat(5_000);
    let delivery = env
        .notifier
        .notify(NewNotice {
            title: long_title.clone(),
            body: long_body.clone(),
            ..notice(NoticeKind::DailySummary, T0)
        })
        .await
        .unwrap();
    let json = open_json(&sub, &env.push.received_at(&path_of(&sub))[0]);
    assert_eq!(
        json["title"].as_str().unwrap().chars().count(),
        MAX_PUSH_TITLE_CHARS
    );
    assert_eq!(
        json["body"].as_str().unwrap().chars().count(),
        MAX_PUSH_BODY_CHARS
    );
    let event = &env.store.read_events(EventRange::default(), 10).unwrap()[0];
    assert_eq!(event.seq, delivery.seq);
    assert_eq!(event.payload["title"], long_title.as_str());
    assert_eq!(event.payload["body"], long_body.as_str());
}
