//! 하루 시험(V47)에 기록 출력(tracing)과 백업 묶음도 넣는다: 시험용 키를 넣고 가짜 AI가 그 키 글자를
//! 말하게 한 채 논의 · 매매 · 복기 · 백업을 돌린 뒤, 기록 출력과 풀어 본 백업 묶음에서 그 키 글자를 찾는다.

use std::io;
use std::sync::{Arc, Mutex};

use pi_agent::Role;
use pi_secrets::SecretStore;
use price_inha::{DemoTurn, RedactingWriter};
use serde_json::json;
use tracing_subscriber::fmt::MakeWriter;

use crate::common::{Bench, DART_KEY, HYNIX, kst, monday};

/// 기록 출력을 모으는 곳.
#[derive(Clone, Default)]
struct Sink(Arc<Mutex<Vec<u8>>>);

impl Sink {
    fn text(&self) -> String {
        String::from_utf8_lossy(&self.0.lock().unwrap()).into_owned()
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

// 검증: V47 E38 N8 T18
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 하루를_돌린_뒤_기록_출력과_백업_묶음에도_그_키_글자가_없다() {
    let bench = Bench::start(kst(monday(), 10, 0)).await;
    let secrets = Arc::new(SecretStore::open(bench.home()).unwrap());
    let sink = Sink::default();
    let subscriber = tracing_subscriber::fmt()
        .with_ansi(false)
        .with_max_level(tracing::Level::TRACE)
        .with_writer(RedactingWriter::new(secrets, sink.clone()))
        .finish();
    tracing::subscriber::set_global_default(subscriber).expect("이 바이너리에서 한 번만 건다");

    bench.script(
        DemoTurn::Talk(Role::Scout),
        &format!("새 공시를 확인했다 {DART_KEY} @strategy @risk"),
    );
    let review = json!({"mistakes": ["late_stop"], "note": format!("손절이 늦었다 {DART_KEY}")});
    bench.script(DemoTurn::Review, &review.to_string());
    bench.ready().await;
    let _ = bench.sold(HYNIX).await;
    let _ = bench.discuss_buy(HYNIX).await;
    bench.crew().review_day().await.expect("복기");
    tracing::warn!("하루 시험이 남긴 줄 {DART_KEY} 끝");
    let report = bench.app.back_up_now().await.expect("백업");

    let text = sink.text();
    assert!(text.contains("하루 시험이 남긴 줄"), "기록 출력을 모았다");
    assert!(!text.contains(DART_KEY), "기록 출력에 키 글자: {text}");
    let bytes = bench.github.file(&report.remote_path);
    let bundle = pi_backup::open(&bytes, &bench.age.identity).expect("우리 키로 풀린다");
    assert!(!bundle.files.is_empty());
    for file in &bundle.files {
        let plain = String::from_utf8_lossy(&file.bytes);
        assert!(!plain.contains(DART_KEY), "{}에 키 글자", file.path);
    }
    bench.close().await;
}
