//! 묶음 C "지식 검색 잇기"(server.md 10c 13번 · knowledge 4차 4번)와 묶음 D(10c N3 참고):
//!
//! - 뜻 검색 도구가 `hybrid_search`로 이어진다: 다시 만들기가 끝나기 전에는 키워드 결과 + `reindexing`, 끝난
//!   뒤에는 뜻이 가까운 조각이 키워드가 맞지 않아도 나온다. 질의 임베딩이 늦으면 도구가 시간 제한으로 끊고
//!   키워드 결과 + `embedder_timeout`을 준다.
//! - 검색 결과의 바깥 자료 snippet에 "바깥 글" 표시가 붙는다.
//! - 시장 메모 색인: 정산된 결정 메모의 교훈(복기 담당이 쓴 글)이 시장 메모(`market_note` · `team`)로 색인된다.
//! - 다시 만들기는 한가할 때만 돈다(논의 중에는 Ollama에 묻지 않는다). 끄는 순간 진행 중인 걸음도 끊는다.
//! - 원격 세션에서 뜻 검색 설정 POST는 403이다. 진단의 Ollama 확인은 짧게 끊고, 설정 파일의 주소가 루프백이
//!   아니면 그렇게 알린다.
//!
//! 실행 파일의 `--demo` 조립(가짜 AI · 시연 자료)으로 띄운다. Ollama는 로컬 가짜 서버뿐이다.

use std::time::{Duration, Instant};

use pi_knowledge::{RecordText, SearchMode, SemanticStatus};
use pi_store::{KnowledgeKind, KnowledgeSource, UnixMillis};
use price_inha::{DemoTurn, SEMANTIC_SETTINGS_FILE, SETUP_SEMANTIC_PATH};
use serde_json::json;

use crate::common::{
    Bench, FakeOllama, Knobs, MEMO_SYMBOL, STEP, days_after, decision_day, kst, lesson_reply,
    ollama_check, post,
};

/// 가짜 Ollama의 모델 이름.
const MODEL: &str = "bge-m3";
/// 시험 교훈(낱말 하나가 표지다).
const LESSON: &str =
    "교훈표지라운드육사 다섯 거래일 수익으로 사기 판단이 맞았다. 실적 가설이 뒷받침됐다.";

fn now_of(bench: &Bench) -> UnixMillis {
    UnixMillis(bench.now().timestamp_millis())
}

/// 팀 기록 하나와 바깥 자료 하나를 색인해 둔다.
fn seed(bench: &Bench) {
    let knowledge = bench.knowledge();
    knowledge
        .index(RecordText {
            kind: KnowledgeKind::Trade,
            source: KnowledgeSource::Team,
            record_ref: "T-sem".to_string(),
            text: "손절 원칙을 지켜 작게 잃었다".to_string(),
            at: now_of(bench),
        })
        .unwrap();
    knowledge
        .index(RecordText::external(
            "news:round64",
            "손절 매물 쏟아진 시장 소식",
            "바깥 기사 요약",
            "https://news.example.test/round64",
            now_of(bench),
        ))
        .unwrap();
}

/// 남은 색인 설정의 모델이 `bge-m3`가 될 때까지(다시 만들기가 끝날 때까지) 시계를 옮긴다.
async fn embedded(bench: &Bench) {
    let deadline = Instant::now() + STEP;
    loop {
        let done = bench
            .knowledge()
            .stored_settings()
            .unwrap()
            .is_some_and(|s| s.model == MODEL);
        if done {
            return;
        }
        assert!(Instant::now() < deadline, "다시 만들기가 끝나야 한다");
        bench.set(bench.now() + chrono::Duration::seconds(31));
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

fn code_of(semantic: &SemanticStatus) -> Option<&str> {
    match semantic {
        SemanticStatus::Unavailable { code, .. } => Some(code.as_str()),
        SemanticStatus::Available => None,
    }
}

// 검증: D10 V23
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn 뜻_검색_도구는_다시_만들기_전에는_키워드와_reindexing이고_뒤에는_뜻이_가까운_조각을_준다()
{
    let bench = Bench::start().await;
    seed(&bench);
    let ollama = FakeOllama::start().await;
    // 다시 만들기 첫 걸음이 끝나지 않게 늦춰 두고 켠다.
    ollama.slow_embeds(Duration::from_secs(30));
    let saved = bench.semantic(true, &ollama.uri()).await;
    assert_eq!(saved.status, 200, "{}", saved.text);
    let early = bench
        .search(json!({"mode": "semantic", "query": "손절"}))
        .await;
    assert_eq!(code_of(&early.semantic), Some("reindexing"), "{early:?}");
    assert!(
        early.results.iter().any(|h| h.record_ref == "T-sem"),
        "키워드 결과가 함께 온다: {early:?}"
    );

    // 늦춤을 풀고 다시 켜서(진행 중인 걸음은 끊긴다) 끝까지 만든다.
    ollama.slow_embeds(Duration::ZERO);
    bench.semantic(false, &ollama.uri()).await;
    bench.semantic(true, &ollama.uri()).await;
    embedded(&bench).await;
    let found = bench
        .search(json!({"mode": "semantic", "query": "전혀맞지않는질의"}))
        .await;
    assert_eq!(found.semantic, SemanticStatus::Available, "{found:?}");
    assert_eq!(found.mode, SearchMode::Semantic);
    assert!(
        !found.results.is_empty(),
        "키워드가 맞지 않아도 뜻(벡터)이 가까운 조각이 나온다: {found:?}"
    );
    bench.close().await;
}

// 검증: D10 V23
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn 질의_임베딩이_늦으면_도구가_시간_제한으로_끊고_키워드_결과를_준다() {
    let knobs = Knobs {
        semantic_timeout: Duration::from_secs(1),
        ..Knobs::default()
    };
    let bench = Bench::start_with(knobs).await;
    seed(&bench);
    let ollama = FakeOllama::start().await;
    bench.semantic(true, &ollama.uri()).await;
    embedded(&bench).await;
    ollama.slow_embeds(Duration::from_secs(10));
    let started = Instant::now();
    let out = bench
        .search(json!({"mode": "semantic", "query": "손절"}))
        .await;
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "시간 제한 안에 끝난다: {:?}",
        started.elapsed()
    );
    assert_eq!(code_of(&out.semantic), Some("embedder_timeout"), "{out:?}");
    assert!(
        out.results.iter().any(|h| h.record_ref == "T-sem"),
        "{out:?}"
    );
    bench.close().await;
}

// 검증: M7 E38
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn 검색_결과의_바깥_자료_snippet에는_바깥_글_표시가_붙는다() {
    let bench = Bench::start().await;
    seed(&bench);
    let out = bench
        .search(json!({"mode": "keyword", "query": "손절"}))
        .await;
    let outside = out
        .results
        .iter()
        .find(|h| h.source == "external")
        .unwrap_or_else(|| panic!("바깥 자료가 나온다: {out:?}"));
    assert!(outside.snippet.contains("바깥 글"), "{}", outside.snippet);
    let team = out
        .results
        .iter()
        .find(|h| h.source == "team")
        .unwrap_or_else(|| panic!("팀 기록이 나온다: {out:?}"));
    assert!(!team.snippet.contains("바깥 글"), "{}", team.snippet);
    bench.close().await;
}

// 검증: M7 D10 V22
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn 정산된_결정_메모의_교훈은_시장_메모로_색인되어_찾아진다() {
    let bench = Bench::start().await;
    bench.script(DemoTurn::Review, &lesson_reply(LESSON));
    let trade = bench.judged_buy(MEMO_SYMBOL).await;
    bench.close_on(decision_day(), MEMO_SYMBOL, 100_000);
    bench.close_on(days_after(5), MEMO_SYMBOL, 110_000);
    bench.set(kst(days_after(5), 16, 0));
    bench.close_day().await;
    assert_eq!(bench.memo_of(trade).await["state"], "settled");

    let key = format!("memo:{}", trade.0);
    let chunks = bench
        .store()
        .knowledge_chunks(KnowledgeKind::MarketNote, &key)
        .unwrap();
    assert!(!chunks.is_empty(), "교훈이 시장 메모로 색인됐다");
    assert!(chunks.iter().all(|c| c.source == KnowledgeSource::Team));
    assert!(chunks.iter().any(|c| c.text.contains("교훈표지라운드육사")));
    let out = bench
        .search(json!({"mode": "keyword", "query": "교훈표지라운드육사"}))
        .await;
    assert!(
        out.results
            .iter()
            .any(|h| h.kind == "market_note" && h.record_ref == key),
        "{out:?}"
    );
    assert_eq!(bench.outside_ai_requests().await, 0, "시연 AI만 쓴다");
    bench.close().await;
}

// 검증: D10 V23
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn 다시_만들기는_논의_중에는_쉬고_논의가_끝나면_돈다() {
    let knobs = Knobs {
        ai_pause: Duration::from_millis(700),
        ..Knobs::default()
    };
    let bench = Bench::start_with(knobs).await;
    seed(&bench);
    bench.quote(MEMO_SYMBOL).await;
    let crew = bench.crew();
    let discussing = tokio::spawn(async move { crew.discuss(MEMO_SYMBOL, "한가함 시험").await });
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(!discussing.is_finished(), "논의가 열려 있다");
    let ollama = FakeOllama::start().await;
    bench.semantic(true, &ollama.uri()).await;
    for _ in 0..4 {
        bench.set(bench.now() + chrono::Duration::seconds(31));
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(!discussing.is_finished(), "논의가 아직 열려 있다");
    assert_eq!(ollama.embeds().await, 0, "논의 중에는 Ollama에 묻지 않는다");
    tokio::time::timeout(STEP, discussing)
        .await
        .expect("논의가 끝난다")
        .unwrap()
        .expect("논의가 끝난다");
    embedded(&bench).await;
    assert!(ollama.embeds().await > 0, "논의가 끝나면 다시 만든다");
    bench.close().await;
}

// 검증: D10 V23
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn 끄는_순간_진행_중인_걸음도_끊고_임베딩을_두지_않는다() {
    let bench = Bench::start().await;
    seed(&bench);
    let ollama = FakeOllama::start().await;
    ollama.slow_embeds(Duration::from_secs(3));
    bench.semantic(true, &ollama.uri()).await;
    let deadline = Instant::now() + STEP;
    while ollama.embeds().await == 0 {
        assert!(Instant::now() < deadline, "걸음이 Ollama에 묻는다");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let off = bench.semantic(false, &ollama.uri()).await;
    assert_eq!(off.status, 200, "{}", off.text);
    tokio::time::sleep(Duration::from_secs(4)).await;
    let missing = bench.store().chunks_missing_embedding(MODEL, 100).unwrap();
    assert_eq!(missing.len(), 2, "끈 걸음의 답은 두지 않는다");
    bench.close().await;
}

// 검증: D10 V23
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn 원격_세션은_뜻_검색_설정을_바꿀_수_없다() {
    let bench = Bench::start().await;
    let ollama = FakeOllama::start().await;
    let cookie = bench.remote_login().await;
    let body = json!({"enabled": true, "base_url": ollama.uri(), "model": MODEL});
    let reply = post(bench.addr(), SETUP_SEMANTIC_PATH, &cookie, &body).await;
    assert_eq!(reply.status, 403, "{}", reply.text);
    assert!(
        !bench.home().join(SEMANTIC_SETTINGS_FILE).exists(),
        "설정이 남지 않는다"
    );
    bench.close().await;
}

// 검증: D1 D10
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn 진단은_ollama가_늦으면_짧게_끊고_루프백이_아닌_설정_주소를_알린다() {
    let bench = Bench::start().await;
    let ollama = FakeOllama::start().await;
    bench.semantic(true, &ollama.uri()).await;
    ollama.slow_tags(Duration::from_secs(20));
    let (slow, took) = ollama_check(bench.home()).await;
    assert!(!slow.ok, "{slow:?}");
    assert!(took < Duration::from_secs(8), "짧게 끊는다: {took:?}");

    let file = bench.home().join(SEMANTIC_SETTINGS_FILE);
    std::fs::write(
        &file,
        json!({"enabled": true, "base_url": "http://192.168.0.2:11434", "model": MODEL})
            .to_string(),
    )
    .unwrap();
    let (outside, _) = ollama_check(bench.home()).await;
    assert!(!outside.ok, "{outside:?}");
    assert!(outside.what.contains("루프백"), "{outside:?}");
    bench.close().await;
}
