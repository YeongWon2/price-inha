//! 지식 검색 잇기(T16 · D10 · M7): 자동 회상(V22), 검색 도구, 바깥으로 보내지 않음(V23), 가린 글만 색인.

use pi_knowledge::{SearchMode, SemanticStatus};
use pi_store::{KnowledgeKind, TradeId};
use price_inha::{DemoTurn, RECALL_SOURCE_PREFIX, TeamError};
use serde_json::json;

use crate::common::{Bench, DART_KEY, HYNIX, kst, monday, seen_text};

/// 손절로 팔고 복기까지 마친 지난 매매.
async fn reviewed_trade(bench: &Bench, note: &str) -> TradeId {
    bench.ready().await;
    let past = bench.sold(HYNIX).await;
    let review = json!({"mistakes": ["late_stop"], "note": note}).to_string();
    bench.script(DemoTurn::Review, &review);
    let reviewed = bench.crew().review_day().await.expect("복기");
    assert_eq!(reviewed.reviews.len(), 1);
    past
}

// 검증: V22 T16 D10 M7
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 비슷한_지난_매매가_있으면_판단_기록에_꺼내_본_지난_매매가_남는다() {
    let bench = Bench::start(kst(monday(), 10, 0)).await;
    let past = reviewed_trade(&bench, "자기주식취득결정 공시 뒤 따라 샀고 손절이 늦었다").await;
    bench.set(bench.now() + chrono::Duration::minutes(5));
    let discussed = bench.discuss_buy(HYNIX).await;
    let trade = bench.store().get_trade(discussed.trade).unwrap();
    let wanted = format!("{RECALL_SOURCE_PREFIX}{}", past.0);
    assert!(
        trade.sources.iter().any(|s| s.name == wanted),
        "판단 기록의 자료에 {wanted}가 있다: {:?}",
        trade.sources
    );
    bench.close().await;
}

// 검증: T16 D10 M7
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 검색_도구는_두_글자_낱말과_조건과_뜻_검색을_돌려준다() {
    let bench = Bench::start(kst(monday(), 10, 0)).await;
    let past = reviewed_trade(&bench, "손절 기준을 늦게 지켰다").await;
    let crew = bench.crew();
    let id = past.0.to_string();

    let found = crew
        .search_knowledge(&json!({"mode": "keyword", "query": "손절"}))
        .await
        .expect("키워드 검색");
    assert_eq!(found.mode, SearchMode::Keyword);
    assert!(
        found.results.iter().any(|hit| hit.record_ref == id),
        "두 글자 낱말로 지난 복기를 찾는다: {:?}",
        found.results
    );

    let by_condition = crew
        .search_knowledge(&json!({"mode": "condition", "symbol": HYNIX, "max_pnl": "0"}))
        .await
        .expect("조건 검색");
    assert_eq!(by_condition.mode, SearchMode::Condition);
    assert!(by_condition.trade_refs().contains(&id), "{by_condition:?}");
    let winners = crew
        .search_knowledge(&json!({"mode": "condition", "symbol": HYNIX, "min_pnl": "1"}))
        .await
        .expect("조건 검색");
    assert!(
        !winners.trade_refs().contains(&id),
        "잃은 매매는 번 매매 조건에 없다"
    );

    let semantic = crew
        .search_knowledge(&json!({"mode": "semantic", "query": "손절"}))
        .await
        .expect("뜻 검색은 오류가 아니다");
    assert!(semantic.results.is_empty());
    match semantic.semantic {
        SemanticStatus::Unavailable { code, .. } => assert_eq!(code, "no_embedder"),
        other => panic!("임베딩이 없으면 쓸 수 없음: {other:?}"),
    }

    for bad in [
        json!({"mode": "nope", "query": "손절"}),
        json!({"query": "손절"}),
        json!({"mode": "keyword", "query": ""}),
        json!({"mode": "condition", "max_pnl": "숫자아님"}),
    ] {
        let refused = crew.search_knowledge(&bad).await;
        assert!(
            matches!(refused, Err(TeamError::Knowledge(_))),
            "{bad}: {refused:?}"
        );
    }
    bench.close().await;
}

// 검증: V23 M7 N8 E38
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 지식_기록은_가린_글만_색인하고_밖으로_보내지_않는다() {
    let bench = Bench::start(kst(monday(), 10, 0)).await;
    let marker = "회상표지라운드열하나";
    let note = format!("{marker} 손절이 늦었다 키 {DART_KEY}");
    let past = reviewed_trade(&bench, &note).await;
    let id = past.0.to_string();
    let store = bench.store();
    let mut indexed = String::new();
    for kind in [KnowledgeKind::Trade, KnowledgeKind::Review] {
        let chunks = store.knowledge_chunks(kind, &id).unwrap();
        assert!(!chunks.is_empty(), "{kind:?} 기록이 색인됐다");
        for chunk in chunks {
            indexed.push_str(&chunk.text);
        }
    }
    assert!(indexed.contains(marker), "복기 글이 색인됐다");
    assert!(!indexed.contains(DART_KEY), "색인에는 가린 글만 있다");
    let found = bench
        .app
        .knowledge()
        .search(
            &pi_knowledge::SearchRequest::keywords(marker),
            pi_store::UnixMillis(bench.now().timestamp_millis()),
        )
        .unwrap();
    assert!(found.results.iter().any(|hit| hit.record_ref == id));
    assert!(
        found
            .results
            .iter()
            .all(|hit| !hit.snippet.contains(DART_KEY))
    );
    bench
        .crew()
        .search_knowledge(&json!({"mode": "semantic", "query": marker}))
        .await
        .unwrap();

    bench.app.back_up_now().await.expect("백업");
    let outward = format!(
        "{}{}",
        bench.data.seen().await,
        seen_text(&bench.push.server).await
    );
    assert!(
        !outward.contains(marker),
        "지식 기록을 밖으로 보낸 요청이 없다"
    );
    for path in bench.github.paths() {
        let bytes = bench.github.file(&path);
        assert!(
            !String::from_utf8_lossy(&bytes).contains(marker),
            "백업은 암호문으로만 나간다"
        );
    }
    bench.close().await;
}
