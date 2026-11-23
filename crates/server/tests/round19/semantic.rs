//! 13번 뜻 검색(ai 10차 · 결정 15): 설정 "뜻 검색 켜기"(기본 꺼짐, 루프백 주소만, 모델 기본 `bge-m3`),
//! 켜면 로컬 Ollama(가짜 서버)로 기존 기록의 임베딩을 한 걸음씩 다시 만들고, 끄면 멈춘다. 꺼져 있으면
//! Ollama 요청 0건(V23). 진단(`doctor`)에 Ollama 항목.

use std::path::{Path, PathBuf};

use pi_knowledge::{Knowledge, SearchConfig};
use price_inha::{
    CheckItem, CommandLine, CommandOutput, CommandRunner, DEFAULT_EMBED_MODEL, Os,
    SEMANTIC_LOOPBACK_ONLY, SETUP_SEMANTIC_PATH, Service, ServiceSetup, doctor,
};
use serde_json::Value;

use crate::common::{
    Bench, EMBED_DIMS, FakeOllama, HYNIX, REEMBED_BATCH, REEMBED_INTERVAL, dead_base_url,
    doctor_settings, get, kst, monday, settle,
};

/// 걸음 간격(초).
fn interval() -> i64 {
    i64::try_from(REEMBED_INTERVAL.as_secs()).unwrap()
}

/// 기록 몇 개(매매 · 복기 · 실수)를 색인해 둔 서버.
async fn with_records() -> Bench {
    let bench = Bench::start(kst(monday(), 10, 0)).await;
    bench.ready().await;
    bench
        .reviewed(HYNIX, &["late_stop"], "손절 기준을 늦게 지켰다")
        .await;
    bench
}

/// 이 보관 폴더에 남은 색인 설정의 모델 이름(뜻 검색 색인이 다 만들어졌을 때 남는다).
fn indexed_model(bench: &Bench) -> Option<(String, Option<u32>)> {
    Knowledge::new(bench.store(), SearchConfig::default())
        .stored_settings()
        .unwrap()
        .map(|s| (s.model, s.dims))
}

fn input_count(body: &Value) -> usize {
    body["input"].as_array().map_or(0, Vec::len)
}

// 검증: D10 V23
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 뜻_검색은_기본_꺼짐이고_꺼져_있으면_ollama에_한_번도_묻지_않는다() {
    let bench = with_records().await;
    let shown = get(bench.addr(), SETUP_SEMANTIC_PATH).await;
    assert_eq!(shown.status, 200, "{}", shown.text);
    assert_eq!(shown.body["enabled"], false, "기본은 꺼짐: {}", shown.body);
    assert_eq!(shown.body["model"], DEFAULT_EMBED_MODEL);
    assert_eq!(shown.body["base_url"], pi_ai::OLLAMA_DEFAULT_BASE_URL);

    let ollama = FakeOllama::start(&["bge-m3:latest"]).await;
    let saved = bench.semantic(false, &ollama.uri(), "bge-m3").await;
    assert_eq!(saved.status, 200, "{}", saved.text);
    assert_eq!(saved.body["enabled"], false);
    assert_eq!(saved.body["base_url"], ollama.uri().as_str());
    for _ in 0..4 {
        bench.advance(interval()).await;
    }
    assert!(
        ollama.requests().await.is_empty(),
        "꺼져 있으면 Ollama 요청 0건"
    );
    let found = bench
        .crew()
        .search_knowledge(&serde_json::json!({"mode": "keyword", "query": "손절"}))
        .await
        .expect("키워드 검색은 그대로 된다");
    assert!(!found.results.is_empty());
    bench.close().await;
}

// 검증: D10 V23
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 루프백이_아닌_ollama_주소는_받지_않는다() {
    let bench = Bench::start(kst(monday(), 10, 0)).await;
    for outside in [
        "http://192.168.0.2:11434",
        "http://10.0.0.5:11434",
        "https://ollama.example:11434",
    ] {
        let reply = bench.semantic(true, outside, "bge-m3").await;
        assert_eq!(reply.status, 400, "{outside}: {}", reply.text);
        assert_eq!(reply.body["error"], SEMANTIC_LOOPBACK_ONLY, "{outside}");
    }
    let shown = get(bench.addr(), SETUP_SEMANTIC_PATH).await;
    assert_eq!(shown.body["enabled"], false, "거부한 설정은 남지 않는다");
    assert_eq!(shown.body["base_url"], pi_ai::OLLAMA_DEFAULT_BASE_URL);
    for inside in ["http://127.0.0.1:11434", "http://localhost:11434"] {
        let reply = bench.semantic(false, inside, "bge-m3").await;
        assert_eq!(reply.status, 200, "{inside}는 루프백이다: {}", reply.text);
    }
    bench.close().await;
}

// 검증: D10 V23 E38
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 켜면_ollama로_기존_기록의_임베딩을_한_걸음씩_다시_만든다() {
    let bench = with_records().await;
    let ollama = FakeOllama::start(&["bge-m3:latest"]).await;
    let saved = bench.semantic(true, &ollama.uri(), "bge-m3").await;
    assert_eq!(saved.status, 200, "{}", saved.text);
    assert_eq!(saved.body["enabled"], true);
    settle().await;
    assert!(
        ollama.embeds().await.len() <= 1,
        "시계가 걸음 간격을 지나기 전에는 많아야 한 걸음"
    );

    let mut advanced = 0;
    while indexed_model(&bench).is_none_or(|(model, _)| model != "bge-m3") {
        assert!(advanced < 20, "걸음 간격마다 돌아 끝나야 한다");
        bench.advance(interval()).await;
        advanced += 1;
    }
    assert_eq!(
        indexed_model(&bench),
        Some((
            "bge-m3".to_string(),
            Some(u32::try_from(EMBED_DIMS).unwrap())
        )),
        "다 만든 뒤에 색인 설정을 남긴다"
    );
    let embeds = ollama.embeds().await;
    assert!(
        embeds.len() >= 2,
        "기록이 여럿이라 여러 걸음: {}",
        embeds.len()
    );
    assert!(
        embeds.len() <= advanced + 1,
        "걸음 간격마다 한 번: 요청 {} · 간격 {advanced}",
        embeds.len()
    );
    for request in &embeds {
        let body: Value = serde_json::from_slice(&request.body).unwrap();
        assert_eq!(body["model"], "bge-m3");
        let count = input_count(&body);
        assert!(
            (1..=REEMBED_BATCH).contains(&count),
            "한 걸음은 많아야 {REEMBED_BATCH}조각: {body}"
        );
        assert!(
            request.headers.get("authorization").is_none(),
            "인증 헤더가 없다"
        );
    }
    bench.close().await;
}

// 검증: D10 V23
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 끄면_다시_만들기를_멈춘다() {
    let bench = with_records().await;
    let ollama = FakeOllama::start(&["bge-m3:latest"]).await;
    bench.semantic(true, &ollama.uri(), "bge-m3").await;
    let mut advanced = 0;
    while ollama.embeds().await.is_empty() {
        assert!(advanced < 5, "켜면 걸음이 돈다");
        bench.advance(interval()).await;
        advanced += 1;
    }
    let off = bench.semantic(false, &ollama.uri(), "bge-m3").await;
    assert_eq!(off.status, 200, "{}", off.text);
    settle().await;
    let before = ollama.requests().await.len();
    for _ in 0..4 {
        bench.advance(interval()).await;
    }
    assert_eq!(
        ollama.requests().await.len(),
        before,
        "끈 뒤에는 Ollama에 묻지 않는다"
    );
    assert!(
        indexed_model(&bench).is_none_or(|(model, _)| model != "bge-m3"),
        "다 만들지 않은 색인을 다 만든 것으로 남기지 않는다"
    );
    bench.close().await;
}

// ───────────── 진단 ─────────────

/// 아무 명령도 부르지 않는 시험용 실행기(서비스는 등록하지 않은 것으로 보인다).
struct NoCommands;

impl CommandRunner for NoCommands {
    fn run(&self, _command: &CommandLine) -> std::io::Result<CommandOutput> {
        Ok(CommandOutput {
            success: false,
            stdout: String::new(),
            stderr: String::new(),
        })
    }
}

/// 그 보관 폴더를 진단해 Ollama 항목을 돌려준다(진단은 막는 함수라 따로 돈다).
async fn ollama_check(home: &Path) -> price_inha::Check {
    let home: PathBuf = home.to_path_buf();
    tokio::task::spawn_blocking(move || {
        let defs = tempfile::tempdir().unwrap();
        let runner = NoCommands;
        let setup = ServiceSetup {
            os: Os::MacOs,
            exe: PathBuf::from("/opt/price-inha/bin/price-inha"),
            home: home.clone(),
            definition_dir: defs.path().to_path_buf(),
            uid: 501,
            path_env: "/usr/bin:/bin".to_string(),
            run_args: Vec::new(),
        };
        let service = Service::new(setup, &runner);
        let report = doctor(&doctor_settings(&home), &service);
        report
            .check(CheckItem::Ollama)
            .cloned()
            .expect("진단에 Ollama 항목이 있다")
    })
    .await
    .unwrap()
}

// 검증: D10 V23 D1
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 진단에_ollama_항목이_꺼짐_켜짐_닿지_않음을_알린다() {
    let bench = Bench::start(kst(monday(), 10, 0)).await;
    let home = bench.home().to_path_buf();

    let off = ollama_check(&home).await;
    assert!(off.ok, "꺼짐은 문제가 아니다: {off:?}");
    assert!(off.what.contains("꺼짐"), "{off:?}");

    let ollama = FakeOllama::start(&["bge-m3:latest"]).await;
    bench.semantic(false, &ollama.uri(), "bge-m3").await;
    let still_off = ollama_check(&home).await;
    assert!(
        still_off.ok && still_off.what.contains("꺼짐"),
        "{still_off:?}"
    );
    assert!(
        ollama.requests().await.is_empty(),
        "꺼져 있으면 진단도 Ollama에 묻지 않는다"
    );

    bench.semantic(true, &ollama.uri(), "bge-m3").await;
    let on = ollama_check(&home).await;
    assert!(on.ok, "켜짐 · 닿음 · 모델 있음: {on:?}");
    assert!(on.what.contains("켜짐"), "{on:?}");
    assert!(
        ollama
            .requests()
            .await
            .iter()
            .any(|r| r.url.path() == "/api/tags"),
        "받아 둔 모델 목록을 본다"
    );

    let empty = FakeOllama::start(&["llama3:latest"]).await;
    bench.semantic(true, &empty.uri(), "bge-m3").await;
    let no_model = ollama_check(&home).await;
    assert!(!no_model.ok, "모델이 없으면 문제: {no_model:?}");
    assert!(
        no_model
            .fix
            .as_deref()
            .is_some_and(|fix| fix.contains("ollama pull bge-m3")),
        "모델을 받으라고 알린다: {no_model:?}"
    );

    bench.semantic(true, &dead_base_url(), "bge-m3").await;
    let unreachable = ollama_check(&home).await;
    assert!(
        !unreachable.ok,
        "켜졌는데 닿지 않으면 문제: {unreachable:?}"
    );
    assert!(unreachable.fix.is_some(), "{unreachable:?}");
    bench.close().await;
}
