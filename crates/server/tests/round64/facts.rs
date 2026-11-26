//! 묶음 C "공시"(server.md 10b 보강 2 2번)와 묶음 D(10c N3 참고): 고유번호가 없는 종목의 공시 자리는 "받을 수
//! 없음", DART 키가 없으면 하루 요청 수를 쓰지 않는다, 접수번호가 빈 공시들은 기록 키가 겹치지 않게 색인된다,
//! 조건 검색의 날짜는 복기 시각(`updated_at`)이 아니라 실제로 팔린 시각으로 본다.
//!
//! 시연이 아닌 모드(로컬 가짜 시세 · AI · 자료 서버)로 띄운다.

use pi_knowledge::SearchRequest;
use pi_rules::Purpose;
use pi_store::{TradeState, UnixMillis};
use price_inha::Placed;
use serde_json::json;

use crate::live::{
    Bench, ENTRY, HYNIX, Plan, STOP, WATCHED, kst, monday, ms, order, plus, settle, tuesday,
};

/// 고유번호 표에 없는 종목(가짜 시세 서버가 값을 준다).
const NO_CORP: &str = "100010";
/// DART 하루 수를 남기는 저장소 칸(`team::quota::DART_USED_KEY`와 같은 글자).
const DART_USED: &str = "data_poll.dart_used";

/// 그 종목을 지금 논의한다(시험 계기).
async fn discuss(bench: &Bench, symbol: &str) {
    tokio::time::timeout(
        crate::live::STEP,
        bench.crew().discuss(symbol, "round64 시험 논의"),
    )
    .await
    .expect("논의가 시간 안에 끝나야 한다")
    .expect("논의가 끝나야 한다");
}

// 검증: E32 F2
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 고유번호가_없는_종목의_공시_자리는_공시_없음이_아니라_받을_수_없음이다() {
    let bench = Plan::live()
        .watching(&[WATCHED])
        .priced(NO_CORP, 50_000)
        .start(kst(monday(), 10, 0))
        .await;
    bench.ready().await;
    discuss(&bench, NO_CORP).await;
    assert!(
        bench.ai_calls("disclosure · 받을 수 없음").await > 0,
        "공시 자리는 받을 수 없음으로 보인다"
    );
    assert_eq!(
        bench.ai_calls("공시 없음").await,
        0,
        "고유번호가 없으면 '공시 없음'(기간 안에 공시가 없음)이 아니다"
    );
    bench.close().await;
}

// 검증: D9 F2
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn dart_키가_없으면_하루_요청_수를_쓰지_않는다() {
    let bench = Plan::live()
        .watching(&[WATCHED])
        .without_data_keys()
        .polling(std::time::Duration::from_secs(1))
        .start(kst(monday(), 10, 0))
        .await;
    bench.ready().await;
    bench.advance(600).await;
    discuss(&bench, WATCHED).await;
    settle().await;
    assert!(
        bench.data.requests_like("/api/").await.is_empty(),
        "키가 없으면 DART에 묻지 않는다"
    );
    let used = bench.store().knowledge_meta(DART_USED).unwrap();
    assert!(
        used.as_deref().is_none_or(|text| text.ends_with(" 0")),
        "묻지 않은 요청은 하루 수에 세지 않는다: {used:?}"
    );
    bench.close().await;
}

// 검증: M7 D10
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 접수번호가_빈_공시_둘은_기록_키가_겹치지_않게_따로_색인된다() {
    let bench = Plan::live()
        .watching(&[WATCHED])
        .start(kst(monday(), 10, 0))
        .await;
    bench.ready().await;
    bench
        .data
        .disclose_unnumbered(WATCHED, "첫째보고서번호없음");
    bench
        .data
        .disclose_unnumbered(WATCHED, "둘째보고서번호없음");
    discuss(&bench, WATCHED).await;
    let knowledge = bench.knowledge();
    let now = UnixMillis(bench.now().timestamp_millis());
    let mut refs = Vec::new();
    for name in ["첫째보고서번호없음", "둘째보고서번호없음"] {
        let found = knowledge
            .search(&SearchRequest::keywords(name), now)
            .unwrap();
        let hit = found
            .results
            .iter()
            .find(|hit| hit.source == "external")
            .unwrap_or_else(|| panic!("{name}이 바깥 자료로 색인됐다: {found:?}"));
        refs.push(hit.record_ref.clone());
    }
    assert_ne!(refs[0], refs[1], "기록 키가 겹치지 않는다: {refs:?}");
    bench.close().await;
}

// 검증: V22 D10
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 조건_검색의_날짜는_복기_시각이_아니라_팔린_시각으로_본다() {
    let bench = Plan::live()
        .watching(&[WATCHED])
        .priced(HYNIX, ENTRY)
        .start(kst(monday(), 10, 0))
        .await;
    bench.ready().await;
    let trade = bench.holding(HYNIX).await;
    let exit = plus(bench.now(), 1);
    bench.quote(HYNIX, STOP, exit).await;
    let placed = bench
        .place(order(trade, HYNIX, Purpose::StopLoss, STOP, exit))
        .await;
    assert!(matches!(placed, Placed::Ordered(_)), "판다: {placed:?}");
    bench.quote(HYNIX, STOP, plus(exit, 1)).await;
    assert_eq!(
        bench.trade_state(trade),
        TradeState::Sold,
        "월요일에 팔렸다"
    );

    // 화요일에 복기된다(매매의 `updated_at`이 화요일이 된다).
    let review_at = kst(tuesday(), 16, 0);
    bench.set(review_at);
    bench
        .store()
        .transition_trade(trade, TradeState::Reviewed, ms(review_at))
        .unwrap();
    let id = trade.0.to_string();
    bench
        .knowledge()
        .index(pi_knowledge::RecordText {
            kind: pi_store::KnowledgeKind::Trade,
            source: pi_store::KnowledgeSource::Team,
            record_ref: id.clone(),
            text: format!("매매 {id} · 종목 {HYNIX} 손절"),
            at: ms(review_at),
        })
        .unwrap();

    let on = |date: &str| json!({"mode": "condition", "symbol": HYNIX, "from": date, "to": date});
    let monday_text = monday().format("%Y-%m-%d").to_string();
    let tuesday_text = tuesday().format("%Y-%m-%d").to_string();
    let sold_day = bench
        .crew()
        .search_knowledge(&on(&monday_text))
        .await
        .unwrap();
    assert_eq!(
        sold_day.trade_refs(),
        vec![id.clone()],
        "팔린 날(월요일)로 찾는다"
    );
    let reviewed_day = bench
        .crew()
        .search_knowledge(&on(&tuesday_text))
        .await
        .unwrap();
    assert!(
        reviewed_day.trade_refs().is_empty(),
        "복기한 날(화요일)로는 찾지 않는다: {reviewed_day:?}"
    );
    bench.close().await;
}
