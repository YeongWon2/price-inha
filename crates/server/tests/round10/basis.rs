//! 매매 근거와 쉬운 말 요약(T12, V16 · V18 · C6).

use pi_store::TradeState;

use crate::common::*;

fn basis_path(id: i64) -> String {
    price_inha::TRADE_BASIS_PATH.replace("{id}", &id.to_string())
}

// 검증: V18, C6, T12
#[tokio::test]
async fn 매매를_누르면_쉬운_말_근거_요약이_보인다() {
    let bench = Bench::start(kst(monday(), 10, 0)).await;
    bench.ready().await;
    let discussed = bench.discuss_buy(HYNIX).await;
    let addr = bench.addr();

    let reply = get(addr, &basis_path(discussed.trade.0)).await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert_eq!(reply.body["id"], serde_json::json!(discussed.trade.0));
    let summary = reply.body["summary"].as_str().expect("요약 글");
    let trade = bench.store().get_trade(discussed.trade).unwrap();
    assert_eq!(
        summary,
        pi_journal::plain_summary(&trade),
        "요약은 복기 모듈의 고정 문장 틀 그대로다"
    );
    assert!(
        summary.contains("자사주 매입으로 수급 개선"),
        "가설이 보인다"
    );

    let missing = get(addr, &basis_path(9_999)).await;
    assert_eq!(missing.status, 404);
    assert_eq!(missing.body["error"], serde_json::json!("not_found"));
    let anonymous = get_anonymous(addr, &basis_path(discussed.trade.0)).await;
    assert_eq!(anonymous.status, 401, "세션이 없으면 열리지 않는다");
    bench.close().await;
}

// 검증: V16, P9
#[tokio::test]
async fn 체결된_매매는_가설_계기_진입_손절_목표가_비어_있지_않다() {
    let bench = Bench::start(kst(monday(), 10, 0)).await;
    bench.ready().await;
    let discussed = bench.discuss_buy(HYNIX).await;
    assert_eq!(bench.trade_state(discussed.trade), TradeState::Ordered);

    let later = bench.now() + chrono::Duration::seconds(1);
    bench.quote(HYNIX, ENTRY, later).await;
    assert_eq!(
        bench.trade_state(discussed.trade),
        TradeState::Holding,
        "지정가 이하의 시세가 오면 체결된다"
    );

    let holding = bench.store().trades_in_state(TradeState::Holding).unwrap();
    assert_eq!(holding.len(), 1);
    for trade in &holding {
        assert!(
            pi_journal::missing_fields(&trade.rationale).is_empty(),
            "근거 칸이 다 있다: {:?}",
            trade.rationale
        );
        let rationale = &trade.rationale;
        assert!(
            rationale.entry.is_some() && rationale.stop.is_some() && rationale.target.is_some()
        );
    }
    bench.close().await;
}
