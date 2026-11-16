//! 템플릿 잇기(T8 · C4 · C14 · E5).

use pi_store::{TemplateId, TemplateState, TradeState};
use price_inha::{TEMPLATE_OVER_CAP, TEMPLATE_PATH, TradingError};
use serde_json::json;

use crate::common::{Bench, HYNIX, adjust, buy, choose_swing, kst, monday, post};

// 검증: C4 C14 E5 T8
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 템플릿을_고르면_모의중으로_저장되고_값은_상한_안에서만_바뀐다() {
    let bench = Bench::demo(kst(monday(), 10, 0)).await;
    let addr = bench.addr();
    let store = bench.store();

    let reply = post(addr, TEMPLATE_PATH, Some(&json!({"style": "swing"}))).await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert_eq!(reply.body["style"], json!("swing"));
    assert_eq!(reply.body["state"], json!("simulating"));
    let id = reply.body["id"].as_i64().expect("템플릿 번호");
    let saved = store.get_template(TemplateId(id)).unwrap();
    assert_eq!(
        saved.state,
        TemplateState::Simulating,
        "고른 템플릿은 모의중이다"
    );
    assert_eq!(saved.revision, 1);

    let reply = adjust(addr, id, json!({"daily_loss_limit_pct": "2"})).await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert_eq!(reply.body["revision"], json!(2));
    assert_eq!(reply.body["state"], json!("simulating"));
    assert_eq!(store.template_revisions(TemplateId(id)).unwrap().len(), 2);

    // 하루 손실 한도의 절대 상한은 5%다.
    let reply = adjust(addr, id, json!({"daily_loss_limit_pct": "6"})).await;
    assert_eq!(reply.status, 400);
    assert_eq!(reply.body["error"], json!(TEMPLATE_OVER_CAP));
    assert!(
        reply.body["fields"]
            .as_array()
            .is_some_and(|f| f.contains(&json!("daily_loss_limit_pct"))),
        "넘은 칸이 보인다: {}",
        reply.body
    );
    assert_eq!(
        store.template_revisions(TemplateId(id)).unwrap().len(),
        2,
        "거부된 값은 저장하지 않는다"
    );

    let reply = adjust(addr, id, json!({"no_such_field": "1"})).await;
    assert_eq!(reply.status, 400);
    assert_eq!(reply.body["error"], json!("invalid_input"));
    let reply = post(addr, TEMPLATE_PATH, Some(&json!({"style": "scalping"}))).await;
    assert_eq!(reply.status, 400, "모르는 스타일은 받지 않는다");
    bench.close().await;
}

// 검증: T6 T8
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 고른_템플릿이_없으면_주문_안을_받지_않는다() {
    let bench = Bench::demo(kst(monday(), 10, 0)).await;
    let trade = bench.judged(HYNIX);
    let result = bench.trading().place(buy(trade, HYNIX, bench.now())).await;
    assert!(
        matches!(result, Err(TradingError::NoTemplate)),
        "템플릿 없이 점검하지 않는다: {result:?}"
    );
    assert_eq!(bench.trade_state(trade), TradeState::Judged);
    choose_swing(bench.addr()).await;
    bench.close().await;
}
