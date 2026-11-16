//! tracing 출력의 가리기(T18 · E38 · N8).

use std::io;
use std::sync::{Arc, Mutex};

use pi_secrets::SecretStore;
use price_inha::RedactingWriter;
use tracing_subscriber::fmt::MakeWriter;

use crate::common::{AI_KEY, BROKER_ID, BROKER_SECRET, register_keys};

/// 쓴 글자를 모으는 곳.
#[derive(Clone, Default)]
struct Sink(Arc<Mutex<Vec<u8>>>);

impl Sink {
    fn text(&self) -> String {
        String::from_utf8(self.0.lock().unwrap().clone()).unwrap()
    }
}

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

    fn make_writer(&'a self) -> Sink {
        self.clone()
    }
}

// 검증: T18 E38 N8
#[tokio::test]
async fn 기록_출력에는_등록된_키_값이_가려져_나가고_나머지_글은_그대로다() {
    let dir = tempfile::tempdir().unwrap();
    register_keys(dir.path()).await;
    let secrets = Arc::new(SecretStore::open(dir.path()).unwrap());
    let sink = Sink::default();
    let subscriber = tracing_subscriber::fmt()
        .with_ansi(false)
        .with_writer(RedactingWriter::new(secrets, sink.clone()))
        .finish();
    tracing::subscriber::with_default(subscriber, || {
        tracing::info!(client = BROKER_ID, "증권사 응답 첫째줄 {BROKER_SECRET} 끝");
        tracing::warn!("ai 요청 둘째줄 {AI_KEY}");
    });

    let text = sink.text();
    for secret in [BROKER_ID, BROKER_SECRET, AI_KEY] {
        assert!(
            !text.contains(secret),
            "키 값이 기록에 나가면 안 된다: {text}"
        );
    }
    assert!(
        text.contains("증권사 응답 첫째줄"),
        "나머지 글은 그대로다: {text}"
    );
    assert!(text.contains("끝"));
    assert!(text.contains("ai 요청 둘째줄"));
    assert_eq!(text.lines().count(), 2, "사건마다 한 줄이다: {text}");
}
