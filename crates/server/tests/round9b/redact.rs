//! 기록 가리기는 앱과 같은 비밀 보관소(`Arc`)를 쓴다: 띄운 뒤 등록한 키도 가린다(E38 · N8 · T18).

use std::io;
use std::sync::{Arc, Mutex};

use pi_secrets::{REDACTED, SecretStore};
use price_inha::{KEY_AI_API_KEY, KEY_BROKER, RedactingWriter, SETUP_KEYS_PATH};
use serde_json::json;
use tracing_subscriber::fmt::MakeWriter;

use crate::common::{Bench, kst, monday, post};

/// 시험용 키 값들. 진짜 키처럼 보이지 않는다.
const BROKER_ID: &str = "test-client-id-round9b-0001";
const BROKER_SECRET: &str = "test-client-secret-round9b-1";
const AI_KEY: &str = "test-ai-key-round9b-not-real";

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

// 검증: E38 N8 T18 C3 C15
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 띄운_뒤_설정_화면에서_등록한_키_값도_기록_출력에서_가려진다() {
    let dir = tempfile::tempdir().unwrap();
    let secrets = Arc::new(SecretStore::open(dir.path()).unwrap());
    let sink = Sink::default();
    let subscriber = tracing_subscriber::fmt()
        .with_ansi(false)
        .with_writer(RedactingWriter::new(Arc::clone(&secrets), sink.clone()))
        .finish();
    let bench = Bench::sharing(dir, secrets, kst(monday(), 10, 0)).await;

    // 확인은 아무도 듣지 않는 로컬 주소로 가므로 실패할 수 있다. 확인에 실패한 값도 가린다(N8).
    let broker = json!({
        "item": KEY_BROKER,
        "fields": {"client_id": BROKER_ID, "client_secret": BROKER_SECRET},
    });
    let ai = json!({"item": KEY_AI_API_KEY, "fields": {"api_key": AI_KEY}});
    for body in [broker, ai] {
        let reply = post(bench.addr(), SETUP_KEYS_PATH, &body).await;
        assert_ne!(reply.status, 404, "등록 경로가 있어야 한다: {}", reply.body);
    }

    tracing::subscriber::with_default(subscriber, || {
        tracing::info!(client = BROKER_ID, "증권사 응답 첫째줄 {BROKER_SECRET} 끝");
        tracing::warn!("ai 요청 둘째줄 {AI_KEY}");
    });
    let text = sink.text();
    for secret in [BROKER_ID, BROKER_SECRET, AI_KEY] {
        assert!(
            !text.contains(secret),
            "띄운 뒤 등록한 키 값이 기록에 나가면 안 된다: {text}"
        );
    }
    assert!(text.contains(REDACTED), "가린 자리가 보인다: {text}");
    assert!(
        text.contains("증권사 응답 첫째줄") && text.contains("ai 요청 둘째줄"),
        "나머지 글은 그대로다: {text}"
    );
    bench.close().await;
}
