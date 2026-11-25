//! 나가는 글의 가림(V89): 가리개를 거친 글만 보낼 수 있고, 가리기에 실패하면 보내지 않는다.

use std::sync::atomic::{AtomicUsize, Ordering};

use pi_channel::{Channel, ChatId, Error, OutText, RedactError, Redactor};
use static_assertions::assert_not_impl_any;
use wiremock::matchers::path;
use wiremock::{Mock, MockServer};

use crate::common::{
    FailRedactor, MaskRedactor, Numbered, SECRET, bot_path, sent_texts, telegram_at,
};

// 검증: V89 — 가리개를 거치지 않고 보낼 글을 만드는 길이 없다(타입으로 막는다).
assert_not_impl_any!(OutText: From<String>, From<&'static str>, Default, std::str::FromStr);

/// 부른 횟수를 세고 늘 정해진 글을 돌려주는 가리개.
struct Counting(AtomicUsize);

impl Redactor for Counting {
    fn redact(&self, _text: &str) -> Result<String, RedactError> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Ok("가리개가 준 글".to_string())
    }
}

// 검증: V89
#[test]
fn 가리개가_실패하면_보낼_글을_만들_수_없다() {
    let raw = format!("키는 {SECRET}");
    let err = OutText::redacted(&raw, &FailRedactor).unwrap_err();
    assert_eq!(err, Error::Redaction);
    assert!(!err.to_string().contains(SECRET));
    assert!(!format!("{err:?}").contains(SECRET));
}

// 검증: V89
#[test]
fn 보낼_글은_가리개가_준_글이다() {
    let counting = Counting(AtomicUsize::new(0));
    let out = OutText::redacted("원래 글", &counting).unwrap();
    assert_eq!(out.as_str(), "가리개가 준 글");
    assert_eq!(counting.0.load(Ordering::SeqCst), 1);

    let masked = OutText::redacted(&format!("키는 {SECRET}"), &MaskRedactor).unwrap();
    assert_eq!(masked.as_str(), "키는 [가림]");
}

// 검증: V89
#[tokio::test]
async fn 메신저로_나가는_글에_가린_값이_없다() {
    let server = MockServer::start().await;
    Mock::given(path(bot_path("sendMessage")))
        .respond_with(Numbered::from(1))
        .mount(&server)
        .await;
    let out = OutText::redacted(&format!("오늘 정리\n키는 {SECRET}"), &MaskRedactor).unwrap();
    telegram_at(&server.uri())
        .send(&ChatId("5001".to_string()), &out)
        .await
        .unwrap();
    let texts = sent_texts(&server.received_requests().await.unwrap_or_default());
    assert_eq!(texts, vec!["오늘 정리\n키는 [가림]".to_string()]);
    let raw_bodies: Vec<u8> = server
        .received_requests()
        .await
        .unwrap_or_default()
        .iter()
        .flat_map(|r| r.body.clone())
        .collect();
    assert!(!String::from_utf8_lossy(&raw_bodies).contains(SECRET));
}
