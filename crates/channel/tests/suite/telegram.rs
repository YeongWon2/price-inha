//! 텔레그램 어댑터: 확인 · 받기 · 보내기 · 오류(V87).

use std::net::TcpListener;

use chrono::DateTime;
use pi_channel::{Channel, ChatId, Cursor, Error, MsgRef, TELEGRAM_TEXT_MAX};
use serde_json::json;
use wiremock::matchers::path;
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::common::{
    BOT, Numbered, TOKEN, bot_path, fail, ok, out, param, sent_texts, telegram_at, text_update,
};

/// 오류 글(`Display` · `Debug`)에 토큰이 없다.
fn assert_no_token(err: &Error) {
    assert!(!err.to_string().contains(TOKEN), "Display에 토큰: {err}");
    assert!(!format!("{err:?}").contains(TOKEN), "Debug에 토큰: {err:?}");
}

/// 받은 요청들(없으면 빈 목록).
async fn received(server: &MockServer) -> Vec<wiremock::Request> {
    server.received_requests().await.unwrap_or_default()
}

// 검증: V87
#[tokio::test]
async fn check_는_봇_이름을_준다() {
    let server = MockServer::start().await;
    Mock::given(path(bot_path("getMe")))
        .respond_with(ok(json!({
            "id": 1001, "is_bot": true, "first_name": "시험봇", "username": BOT
        })))
        .expect(1)
        .mount(&server)
        .await;
    let tg = telegram_at(&server.uri());
    assert_eq!(tg.check().await, Ok(BOT.to_string()));
    assert_eq!(tg.name(), "telegram");
    assert_eq!(tg.limits().text_max, TELEGRAM_TEXT_MAX);
    assert_eq!(TELEGRAM_TEXT_MAX, 4000);
}

// 검증: V87
#[tokio::test]
async fn 틀린_토큰이면_종류_있는_오류이고_글에_토큰이_없다() {
    let server = MockServer::start().await;
    Mock::given(path(bot_path("getMe")))
        .respond_with(fail(401, "Unauthorized"))
        .mount(&server)
        .await;
    let err = telegram_at(&server.uri()).check().await.unwrap_err();
    assert_eq!(err, Error::Unauthorized);
    assert_no_token(&err);
}

// 검증: V87
#[tokio::test]
async fn poll_은_받은_글을_읽고_offset_을_잇는다() {
    let server = MockServer::start().await;
    Mock::given(path(bot_path("getUpdates")))
        .respond_with(ok(json!([
            text_update(7, 5001, 5001, "private", "안녕"),
            text_update(8, 5002, -100200, "group", "모두 안녕"),
        ])))
        .up_to_n_times(1)
        .with_priority(1)
        .mount(&server)
        .await;
    Mock::given(path(bot_path("getUpdates")))
        .respond_with(ok(json!([])))
        .mount(&server)
        .await;
    let tg = telegram_at(&server.uri());

    let first = tg.poll(None).await.unwrap();
    assert_eq!(first.items.len(), 2);
    let a = &first.items[0];
    assert_eq!(a.channel, "telegram");
    assert_eq!(a.sender.0, "5001");
    assert_eq!(a.chat, ChatId("5001".to_string()));
    assert!(a.is_direct);
    assert_eq!(a.text, "안녕");
    assert_eq!(a.message_id, 107);
    assert_eq!(a.at, DateTime::from_timestamp(1_790_000_007, 0).unwrap());
    let b = &first.items[1];
    assert_eq!(b.sender.0, "5002");
    assert_eq!(b.chat.0, "-100200");
    assert!(!b.is_direct, "그룹 글은 1:1이 아니다");
    assert_eq!(first.next, Some(Cursor(9)));

    let second = tg.poll(first.next).await.unwrap();
    assert!(second.items.is_empty());
    assert_eq!(
        second.next,
        Some(Cursor(9)),
        "받은 것이 없으면 자리는 그대로"
    );

    let reqs = received(&server).await;
    assert_eq!(reqs.len(), 2);
    assert_eq!(param(&reqs[1], "offset").as_deref(), Some("9"));
    assert_eq!(param(&reqs[0], "timeout").as_deref(), Some("1"));
}

// 검증: V87
#[tokio::test]
async fn 빈_응답은_빈_목록이고_자리는_그대로다() {
    let server = MockServer::start().await;
    Mock::given(path(bot_path("getUpdates")))
        .respond_with(ok(json!([])))
        .expect(1)
        .mount(&server)
        .await;
    let polled = telegram_at(&server.uri())
        .poll(Some(Cursor(42)))
        .await
        .unwrap();
    assert!(polled.items.is_empty());
    assert_eq!(polled.next, Some(Cursor(42)));
    let reqs = received(&server).await;
    assert_eq!(param(&reqs[0], "offset").as_deref(), Some("42"));
}

// 검증: V87
#[tokio::test]
async fn 글이_아닌_갱신은_넘기되_자리는_옮긴다() {
    let server = MockServer::start().await;
    Mock::given(path(bot_path("getUpdates")))
        .respond_with(ok(json!([
            { "update_id": 20, "edited_message": {
                "message_id": 5, "chat": { "id": 5001, "type": "private" },
                "date": 1_790_000_000, "text": "고친 글" } },
            { "update_id": 21, "message": {
                "message_id": 6, "from": { "id": 5001, "is_bot": false, "first_name": "시험" },
                "chat": { "id": 5001, "type": "private" }, "date": 1_790_000_000,
                "photo": [] } },
        ])))
        .mount(&server)
        .await;
    let polled = telegram_at(&server.uri()).poll(None).await.unwrap();
    assert!(polled.items.is_empty());
    assert_eq!(polled.next, Some(Cursor(22)));
}

// 검증: V87
#[tokio::test]
async fn 깨진_응답은_malformed_이고_글에_토큰이_없다() {
    for body in [
        ResponseTemplate::new(200).set_body_string("<html>gateway</html>"),
        ok(json!("결과가 목록이 아니다")),
    ] {
        let server = MockServer::start().await;
        Mock::given(path(bot_path("getUpdates")))
            .respond_with(body)
            .mount(&server)
            .await;
        let err = telegram_at(&server.uri()).poll(None).await.unwrap_err();
        assert!(matches!(err, Error::Malformed(_)), "{err:?}");
        assert_no_token(&err);
    }
}

// 검증: V87
#[tokio::test]
async fn 너무_많은_요청은_retry_after_를_담고_다시_하지_않는다() {
    let server = MockServer::start().await;
    Mock::given(path(bot_path("getUpdates")))
        .respond_with(ResponseTemplate::new(429).set_body_json(json!({
            "ok": false, "error_code": 429,
            "description": "Too Many Requests: retry after 7",
            "parameters": { "retry_after": 7 }
        })))
        .expect(1)
        .mount(&server)
        .await;
    let err = telegram_at(&server.uri()).poll(None).await.unwrap_err();
    assert_eq!(
        err,
        Error::RateLimited {
            retry_after_secs: 7
        }
    );
    assert_no_token(&err);
    server.verify().await;
}

// 검증: V87
#[tokio::test]
async fn 그_밖의_오류는_상태가_든_api_오류다() {
    let server = MockServer::start().await;
    Mock::given(path(bot_path("getUpdates")))
        .respond_with(fail(500, "Internal Server Error"))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(path(bot_path("sendMessage")))
        .respond_with(fail(400, "Bad Request: chat not found"))
        .expect(1)
        .mount(&server)
        .await;
    let tg = telegram_at(&server.uri());
    let err = tg.poll(None).await.unwrap_err();
    assert!(matches!(err, Error::Api { status: 500, .. }), "{err:?}");
    assert_no_token(&err);
    let err = tg
        .send(&ChatId("5001".to_string()), &out("안녕"))
        .await
        .unwrap_err();
    assert!(matches!(err, Error::Api { status: 400, .. }), "{err:?}");
    assert_no_token(&err);
}

// 검증: V87
#[tokio::test]
async fn 닿지_못하면_transport_이고_글에_토큰이_없다() {
    let closed = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", closed.local_addr().unwrap());
    drop(closed);
    let tg = telegram_at(&base);
    for err in [
        tg.check().await.unwrap_err(),
        tg.poll(None).await.unwrap_err(),
    ] {
        assert!(matches!(err, Error::Transport(_)), "{err:?}");
        assert_no_token(&err);
    }
}

// 검증: V87
#[tokio::test]
async fn 짧은_글은_한_번에_보낸다() {
    let server = MockServer::start().await;
    Mock::given(path(bot_path("sendMessage")))
        .respond_with(Numbered::from(100))
        .mount(&server)
        .await;
    let refs = telegram_at(&server.uri())
        .send(&ChatId("5001".to_string()), &out("안녕 😀"))
        .await
        .unwrap();
    assert_eq!(
        refs,
        vec![MsgRef {
            chat: ChatId("5001".to_string()),
            message_id: 100
        }]
    );
    let reqs = received(&server).await;
    assert_eq!(reqs.len(), 1);
    assert_eq!(param(&reqs[0], "chat_id").as_deref(), Some("5001"));
    assert_eq!(param(&reqs[0], "text").as_deref(), Some("안녕 😀"));
}

/// 줄마다 한도보다 훨씬 짧은 긴 글(한글 · 이모지 섞임, 한도의 두 배 넘게).
fn long_text() -> String {
    (0..400)
        .map(|i| format!("{i:03}번째 줄 — 한글과 이모지 😀🚀 섞인 글입니다"))
        .collect::<Vec<_>>()
        .join("\n")
}

// 검증: V87
#[tokio::test]
async fn 긴_글은_줄_경계에서_나눠_차례로_보낸다() {
    let server = MockServer::start().await;
    Mock::given(path(bot_path("sendMessage")))
        .respond_with(Numbered::from(100))
        .mount(&server)
        .await;
    let text = long_text();
    assert!(text.chars().count() > 2 * TELEGRAM_TEXT_MAX);
    let refs = telegram_at(&server.uri())
        .send(&ChatId("5001".to_string()), &out(&text))
        .await
        .unwrap();
    let texts = sent_texts(&received(&server).await);
    assert!(texts.len() >= 3, "조각 {}개", texts.len());
    assert_eq!(refs.len(), texts.len());
    let ids: Vec<i64> = refs.iter().map(|r| r.message_id).collect();
    assert_eq!(ids, (100..100 + texts.len() as i64).collect::<Vec<_>>());
    for piece in &texts {
        assert!(!piece.is_empty());
        assert!(piece.chars().count() <= TELEGRAM_TEXT_MAX);
    }
    assert_eq!(texts.join("\n"), text, "줄 경계에서 차례대로 나눴다");
}

// 검증: V87
#[tokio::test]
async fn 중간_조각이_실패하면_나머지를_보내지_않는다() {
    let server = MockServer::start().await;
    Mock::given(path(bot_path("sendMessage")))
        .respond_with(Numbered::from(100))
        .up_to_n_times(1)
        .with_priority(1)
        .mount(&server)
        .await;
    Mock::given(path(bot_path("sendMessage")))
        .respond_with(fail(400, "Bad Request"))
        .mount(&server)
        .await;
    let err = telegram_at(&server.uri())
        .send(&ChatId("5001".to_string()), &out(&long_text()))
        .await
        .unwrap_err();
    assert!(matches!(err, Error::Api { status: 400, .. }), "{err:?}");
    assert_eq!(received(&server).await.len(), 2);
}

// 검증: V87
#[tokio::test]
async fn 요청은_모두_설정한_주소의_봇_경로로만_간다() {
    let server = MockServer::start().await;
    Mock::given(path(bot_path("getMe")))
        .respond_with(ok(
            json!({ "id": 1001, "is_bot": true, "first_name": "시험봇", "username": BOT }),
        ))
        .mount(&server)
        .await;
    Mock::given(path(bot_path("getUpdates")))
        .respond_with(ok(json!([])))
        .mount(&server)
        .await;
    Mock::given(path(bot_path("sendMessage")))
        .respond_with(Numbered::from(1))
        .mount(&server)
        .await;
    let tg = telegram_at(&server.uri());
    tg.check().await.unwrap();
    tg.poll(None).await.unwrap();
    tg.send(&ChatId("5001".to_string()), &out("안녕"))
        .await
        .unwrap();
    let paths: Vec<String> = received(&server)
        .await
        .iter()
        .map(|r| r.url.path().to_string())
        .collect();
    assert_eq!(
        paths,
        vec![
            bot_path("getMe"),
            bot_path("getUpdates"),
            bot_path("sendMessage")
        ]
    );
}
