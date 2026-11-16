//! 자료 잇기: 판단 기록의 자료 · 출처 · 알게 된 시각(T10, V32~V36).

use std::collections::HashSet;

use pi_agent::Decision;
use pi_store::StoredFact;
use price_inha::DemoTurn;

use crate::common::*;

fn kinds(facts: &[StoredFact]) -> HashSet<&str> {
    facts.iter().map(|fact| fact.kind.as_str()).collect()
}

// 검증: V33, V34, V35, T10
#[tokio::test]
async fn 판단_기록에_재무_금리_환율_공시_자료와_출처가_남는다() {
    let bench = Bench::start(kst(monday(), 10, 0)).await;
    bench.ready().await;

    let discussed = bench.discuss_buy(HYNIX).await;
    assert_eq!(discussed.outcome.decision, Decision::Buy);
    let facts = bench.store().trade_facts(discussed.trade).unwrap();
    let found = kinds(&facts);
    for kind in ["financials", "korea_rate", "korea_fx", "disclosure"] {
        assert!(found.contains(kind), "{kind} 자료가 붙었다: {found:?}");
    }
    for fact in &facts {
        assert!(!fact.source_name.is_empty(), "출처 이름: {fact:?}");
        assert!(
            fact.source_url.starts_with(&bench.data.server.uri()),
            "출처 주소는 받은 곳(로컬 가짜 서버)이다: {}",
            fact.source_url
        );
        assert!(!fact.source_url.contains(DART_KEY), "주소에 키가 없다");
    }
    let disclosure = facts.iter().find(|f| f.kind == "disclosure").unwrap();
    assert_eq!(disclosure.reference.as_deref(), Some(DISCLOSURE_NO));
    assert_eq!(
        disclosure.value,
        pi_store::FactValue::Text(DISCLOSURE_NAME.to_string())
    );
    let financial = facts.iter().find(|f| f.kind == "financials").unwrap();
    assert_eq!(financial.source_name, "DART");
    let rate = facts.iter().find(|f| f.kind == "korea_rate").unwrap();
    assert_eq!(rate.source_name, "ECOS");
    bench.close().await;
}

// 검증: V36, P10
#[tokio::test]
async fn 매매_근거의_자료는_모두_알게_된_시각이_있고_판단보다_늦은_것이_없다() {
    let bench = Bench::start(kst(monday(), 10, 0)).await;
    bench.ready().await;

    let discussed = bench.discuss_buy(HYNIX).await;
    let trade = bench.store().get_trade(discussed.trade).unwrap();
    let decided = trade.decided_at.expect("판단 시각");
    let facts = bench.store().trade_facts(discussed.trade).unwrap();
    assert!(!facts.is_empty(), "근거 자료가 있다");
    let late = facts.iter().filter(|f| f.known_at > decided).count();
    assert_eq!(late, 0, "판단 시각보다 늦게 안 자료가 0건이다");
    assert!(!trade.sources.is_empty(), "판단의 자료 목록이 있다");
    for source in &trade.sources {
        assert!(
            source.known_at <= decided,
            "판단의 자료도 늦지 않다: {source:?}"
        );
        assert!(source.known_at.0 > 0, "알게 된 시각이 있다");
    }
    bench.close().await;
}

// 검증: V32
#[tokio::test]
async fn 장_중에_바뀐_시세가_논의의_현재가로_들어간다() {
    let bench = Bench::start(kst(monday(), 10, 0)).await;
    bench.ready().await;
    bench.script(DemoTurn::Verdict, &watch_verdict());

    bench.quote(HYNIX, 100_000, kst(monday(), 10, 1)).await;
    bench.crew().discuss(HYNIX, "첫 논의").await.unwrap();
    bench.quote(HYNIX, 101_500, kst(monday(), 10, 2)).await;
    bench.crew().discuss(HYNIX, "둘째 논의").await.unwrap();

    let prompts = bench.ai().prompts();
    assert!(
        prompts.iter().any(|p| p.contains("현재가 100000")),
        "첫 논의는 그때의 시세"
    );
    assert!(
        prompts.iter().any(|p| p.contains("현재가 101500")),
        "둘째 논의는 바뀐 시세"
    );
    bench.close().await;
}
