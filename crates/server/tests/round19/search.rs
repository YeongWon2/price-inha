//! 12번 검색 도구(knowledge 4차 잇기): 조건 검색의 날짜(`from` · `to`) · 실수 유형, 복기 때 실수 기록 색인,
//! 뉴스 · 공시 요약을 바깥 자료(`external`)로 색인하고 자동 회상에서는 빼기.
//!
//! 날짜는 `"YYYY-MM-DD"`(그 시장 달력의 날짜, 국내는 한국 시각)이고 `from` · `to` 모두 그 날을 넣는다.

use pi_agent::Role;
use pi_store::TradeId;
use price_inha::{DemoTurn, KNOWLEDGE_TOOL, RECALL_SOURCE_PREFIX};
use serde_json::{Value, json};

use crate::common::{
    Bench, DISCLOSURE_NAME, DISCLOSURE_RECEIPT, ENTRY, HYNIX, WATCHED, kst, monday, settle, tuesday,
};

/// 월요일에 판 매매(실수 `late_stop`)와 화요일에 판 매매(실수 `chased_entry`). 둘 다 같은 종목이다.
async fn two_days(bench: &Bench) -> (TradeId, TradeId) {
    bench.ready().await;
    let monday_trade = bench.reviewed(HYNIX, &["late_stop"], "손절이 늦었다").await;
    bench.set(kst(tuesday(), 10, 0));
    settle().await;
    bench.trade_today().await;
    let tuesday_trade = bench
        .reviewed(HYNIX, &["chased_entry"], "급등을 따라 샀다")
        .await;
    (monday_trade, tuesday_trade)
}

async fn condition(bench: &Bench, input: Value) -> Vec<String> {
    let found = bench
        .crew()
        .search_knowledge(&input)
        .await
        .expect("조건 검색");
    found.trade_refs()
}

// 검증: T16 D10 M7
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 조건_검색은_날짜와_실수_유형으로_거른다() {
    let bench = Bench::start(kst(monday(), 10, 0)).await;
    let (first, second) = two_days(&bench).await;
    let (first, second) = (first.0.to_string(), second.0.to_string());

    let all = condition(&bench, json!({"mode": "condition", "symbol": HYNIX})).await;
    assert!(
        all.contains(&first) && all.contains(&second),
        "조건이 없으면 둘 다: {all:?}"
    );

    let on_tuesday = condition(
        &bench,
        json!({"mode": "condition", "from": "2026-11-17", "to": "2026-11-17"}),
    )
    .await;
    assert_eq!(on_tuesday, vec![second.clone()], "화요일에 판 것만");
    let until_monday = condition(&bench, json!({"mode": "condition", "to": "2026-11-16"})).await;
    assert_eq!(until_monday, vec![first.clone()], "월요일까지 판 것만");
    let from_monday = condition(&bench, json!({"mode": "condition", "from": "2026-11-16"})).await;
    assert!(
        from_monday.contains(&first) && from_monday.contains(&second),
        "그 날을 넣는다: {from_monday:?}"
    );

    let late = condition(&bench, json!({"mode": "condition", "mistake": "late_stop"})).await;
    assert_eq!(late, vec![first.clone()], "그 실수가 있는 매매만");
    let chased = condition(
        &bench,
        json!({"mode": "condition", "mistake": "chased_entry", "to": "2026-11-16"}),
    )
    .await;
    assert!(
        chased.is_empty(),
        "다른 조건과 함께 맞아야 한다: {chased:?}"
    );
    let none = condition(
        &bench,
        json!({"mode": "condition", "mistake": "panic_exit"}),
    )
    .await;
    assert!(none.is_empty(), "그 실수가 있는 매매가 없다: {none:?}");

    let wrong = bench
        .crew()
        .search_knowledge(&json!({"mode": "condition", "from": "어제"}))
        .await;
    assert!(wrong.is_err(), "읽을 수 없는 날짜는 거절한다: {wrong:?}");
    bench.close().await;
}

// 검증: T16 D10
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 담당이_도구로_날짜와_실수_유형_조건을_넘기면_맞는_매매만_받는다() {
    let bench = Bench::start(kst(monday(), 10, 0)).await;
    let (first, second) = two_days(&bench).await;
    let call = json!({
        "action": KNOWLEDGE_TOOL,
        "args": {"mode": "condition", "from": "2026-11-17", "to": "2026-11-17", "mistake": "chased_entry"}
    })
    .to_string();
    bench.script(DemoTurn::Talk(Role::Scout), &call);
    bench.quote(WATCHED, ENTRY, bench.now()).await;
    // 같은 답을 되풀이해 논의는 끝나지 못할 수 있다. 도구 결과가 담당에게 갔는지만 본다.
    let _ = bench.crew().discuss(WATCHED, "자기주식취득결정 공시").await;
    let head = format!("[도구 결과 {KNOWLEDGE_TOOL}]");
    let results: Vec<String> = bench
        .ai()
        .prompts()
        .iter()
        .filter_map(|p| {
            p.split(&head)
                .nth(1)
                .map(|rest| rest.replace([' ', '\n'], ""))
        })
        .collect();
    assert!(!results.is_empty(), "도구 결과가 담당에게 간다");
    let wanted = format!("\"record_ref\":\"{}\"", second.0);
    let unwanted = format!("\"record_ref\":\"{}\"", first.0);
    assert!(
        results.iter().any(|r| r.contains(&wanted)),
        "화요일의 chased_entry 매매가 결과에 있다: {results:?}"
    );
    assert!(
        results.iter().all(|r| !r.contains(&unwanted)),
        "월요일의 late_stop 매매는 결과에 없다: {results:?}"
    );
    bench.close().await;
}

// 검증: T16 M7 D10
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 복기의_실수는_실수_기록으로_색인되고_실수가_없으면_만들지_않는다() {
    let bench = Bench::start(kst(monday(), 10, 0)).await;
    bench.ready().await;
    let with = bench
        .reviewed(HYNIX, &["late_stop", "oversized_position"], "손절이 늦었다")
        .await;
    let without = bench.reviewed(HYNIX, &[], "계획대로 했다").await;
    let found = bench
        .crew()
        .search_knowledge(&json!({"mode": "keyword", "query": "oversized_position"}))
        .await
        .expect("키워드 검색");
    let mistakes: Vec<_> = found
        .results
        .iter()
        .filter(|hit| hit.kind == "mistake")
        .collect();
    assert!(
        mistakes
            .iter()
            .any(|hit| hit.record_ref == with.0.to_string() && hit.source == "team"),
        "실수 기록(종류 mistake · 등급 team · 기록 키 = 매매 번호)이 있다: {:?}",
        found.results
    );
    let late = bench
        .crew()
        .search_knowledge(&json!({"mode": "keyword", "query": "late_stop"}))
        .await
        .expect("키워드 검색");
    assert!(
        late.results
            .iter()
            .all(|hit| !(hit.kind == "mistake" && hit.record_ref == without.0.to_string())),
        "실수 없는 복기는 실수 기록을 만들지 않는다: {:?}",
        late.results
    );
    bench.close().await;
}

// 검증: M7 D10 V22
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 공시_요약은_바깥_자료로_색인되어_명시_검색에는_나오고_자동_회상에는_나오지_않는다() {
    let bench = Bench::start(kst(monday(), 10, 0)).await;
    bench.ready().await;
    bench.discuss_buy(HYNIX).await;
    let found = bench
        .crew()
        .search_knowledge(&json!({"mode": "keyword", "query": "자기주식취득결정"}))
        .await
        .expect("키워드 검색");
    let external: Vec<_> = found
        .results
        .iter()
        .filter(|hit| hit.source == "external")
        .collect();
    assert!(
        external
            .iter()
            .any(|hit| hit.kind == "market_note" && hit.snippet.contains(DISCLOSURE_NAME)),
        "논의 때 받은 공시가 바깥 자료(market_note · external)로 색인된다: {:?}",
        found.results
    );

    bench.set(bench.now() + chrono::Duration::minutes(5));
    let discussed = bench.discuss_buy(WATCHED).await;
    let trade = bench.store().get_trade(discussed.trade).unwrap();
    let recalled: Vec<&str> = trade
        .sources
        .iter()
        .map(|s| s.name.as_str())
        .filter(|name| name.starts_with(RECALL_SOURCE_PREFIX))
        .collect();
    assert!(
        recalled
            .iter()
            .all(|name| !name.contains(DISCLOSURE_RECEIPT) && !name.contains("dart")),
        "바깥 자료는 자동 회상에 나오지 않는다: {recalled:?}"
    );
    bench.close().await;
}
