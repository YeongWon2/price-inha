//! 기록(tracing) · `Debug`에 토큰 · 페어링 코드가 없다(V87 · V88).

use std::io;
use std::net::TcpListener;
use std::sync::{Arc, Mutex};

use pi_channel::{Channel, ChatId, Gatekeeper, Judgement};
use serde_json::json;
use tracing_subscriber::fmt::MakeWriter;
use wiremock::matchers::path;
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::common::{
    BOT, Numbered, TOKEN, TestBook, at_min, bot_path, dm, fail, ok, out, telegram_at, text_update,
};

/// 기록을 모으는 자리.
#[derive(Clone, Default)]
struct Sink(Arc<Mutex<Vec<u8>>>);

impl io::Write for Sink {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl<'a> MakeWriter<'a> for Sink {
    type Writer = Sink;

    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}

impl Sink {
    fn text(&self) -> String {
        String::from_utf8_lossy(&self.0.lock().unwrap()).into_owned()
    }
}

/// 확인 · 받기 · 보내기를 성공 · 실패로 한 번씩 돈다.
async fn drive_telegram() {
    let server = MockServer::start().await;
    Mock::given(path(bot_path("getMe")))
        .respond_with(ok(
            json!({ "id": 1001, "is_bot": true, "first_name": "시험봇", "username": BOT }),
        ))
        .up_to_n_times(1)
        .with_priority(1)
        .mount(&server)
        .await;
    Mock::given(path(bot_path("getMe")))
        .respond_with(fail(401, "Unauthorized"))
        .mount(&server)
        .await;
    Mock::given(path(bot_path("getUpdates")))
        .respond_with(ok(json!([text_update(1, 5001, 5001, "private", "안녕")])))
        .up_to_n_times(1)
        .with_priority(1)
        .mount(&server)
        .await;
    Mock::given(path(bot_path("getUpdates")))
        .respond_with(ResponseTemplate::new(429).set_body_json(json!({
            "ok": false, "error_code": 429, "description": "Too Many Requests: retry after 3",
            "parameters": { "retry_after": 3 }
        })))
        .up_to_n_times(1)
        .with_priority(2)
        .mount(&server)
        .await;
    Mock::given(path(bot_path("getUpdates")))
        .respond_with(ResponseTemplate::new(200).set_body_string("깨진 응답"))
        .mount(&server)
        .await;
    Mock::given(path(bot_path("sendMessage")))
        .respond_with(Numbered::from(1))
        .mount(&server)
        .await;
    let tg = telegram_at(&server.uri());
    let _ = tg.check().await;
    let _ = tg.check().await;
    let _ = tg.poll(None).await;
    let _ = tg.poll(None).await;
    let _ = tg.poll(None).await;
    let _ = tg.send(&ChatId("5001".to_string()), &out("안녕")).await;

    let closed = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", closed.local_addr().unwrap());
    drop(closed);
    let _ = telegram_at(&base).check().await;
}

/// 코드를 주고 · 다시 묻고 · 틀리고 · 맞게 승인한다. 준 코드를 돌려준다.
fn drive_pairing() -> String {
    let gk = Gatekeeper::new(TestBook::new());
    let code = match gk.judge(&dm("5001"), at_min(0)) {
        Ok(Judgement::PairingIssued { code }) => code.as_str().to_string(),
        other => format!("코드 없음: {other:?}"),
    };
    let _ = gk.judge(&dm("5001"), at_min(1));
    let _ = gk.approve("ZZZZZZZZ", at_min(2));
    let _ = gk.approve(&code, at_min(3));
    let _ = gk.revoke(&crate::common::sender("5001"));
    code
}

// 검증: V87 V88
#[tokio::test]
async fn 기록에_토큰과_페어링_코드가_없다() {
    let sink = Sink::default();
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::TRACE)
        .with_ansi(false)
        .with_writer(sink.clone())
        .finish();
    let _guard = tracing::subscriber::set_default(subscriber);

    drive_telegram().await;
    let code = drive_pairing();

    let logged = sink.text();
    assert!(!logged.contains(TOKEN), "기록에 토큰:\n{logged}");
    assert_eq!(code.chars().count(), 8, "코드를 받아야 한다: {code}");
    assert!(!logged.contains(&code), "기록에 코드:\n{logged}");
}

// 검증: V87 V88
#[test]
fn debug_글에_토큰과_코드가_없다() {
    let tg = telegram_at("http://127.0.0.1:9");
    assert!(!format!("{tg:?}").contains(TOKEN));

    let book = TestBook::new();
    let gk = Gatekeeper::new(book.clone());
    let judgement = gk.judge(&dm("5001"), at_min(0)).unwrap();
    let Judgement::PairingIssued { code } = &judgement else {
        panic!("코드를 받아야 한다: {judgement:?}");
    };
    assert!(!format!("{judgement:?}").contains(code.as_str()));
    assert!(!format!("{code:?}").contains(code.as_str()));
    assert!(!format!("{:?}", book.state()).contains(code.as_str()));
}
