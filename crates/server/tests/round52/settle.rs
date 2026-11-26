//! 결정 메모 잇기: 판단 → 정산 대기 → 5거래일 뒤 정산(교훈) · 정산 못 함 · AI 상한 · 다음 논의의 브리핑.

use pi_store::TradeId;
use price_inha::DemoTurn;
use serde_json::{Value, json};

use crate::common::{
    Bench, KIND_MEMO, MEMO_SYMBOL, OTHER_SYMBOL, days_after, decision_day, events, kst,
    lesson_reply, ymd,
};

/// 시험 교훈(두 문장).
const LESSON: &str = "다섯 거래일 수익률 10%로 사기 판단이 맞았다. 실적 개선 가설이 뒷받침됐다.";

/// 그 매매의 `office.memo` 이벤트들.
fn memo_events(bench: &Bench, trade: TradeId) -> Vec<Value> {
    events(&bench.store(), KIND_MEMO)
        .into_iter()
        .filter(|e| e["trade"] == trade.0)
        .collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn 판단된_매매마다_정산_대기_메모가_생기고_5거래일_뒤_마감에_교훈과_함께_정산된다() {
    // 검증: V70
    let bench = Bench::start().await;
    bench.script(DemoTurn::Review, &lesson_reply(LESSON));
    let trade = bench.judged_buy(MEMO_SYMBOL).await;

    // 정산 전에는 "정산 대기"로 보인다(R7-4 · M11 · C25).
    let memo = bench.memo_of(trade).await;
    assert_eq!(memo["state"], "pending", "{memo}");
    assert_eq!(memo["symbol"], MEMO_SYMBOL);
    assert_eq!(memo["decision"], "buy");
    assert_eq!(memo["date"], ymd(decision_day()));
    for absent in ["return_pct", "excess_pct", "hold_days", "lesson"] {
        assert!(
            memo.get(absent).is_none_or(Value::is_null),
            "{absent}: {memo}"
        );
    }

    // 결정 날 · 정산 날(5거래일 뒤) 종가를 남기고 정산 날 마감을 부른다.
    bench.close_on(decision_day(), MEMO_SYMBOL, 100_000);
    bench.close_on(days_after(5), MEMO_SYMBOL, 110_000);
    bench.set(kst(days_after(4), 16, 0));
    bench.close_day().await;
    assert_eq!(
        bench.memo_of(trade).await["state"],
        "pending",
        "정산 날 전에는 기다린다"
    );

    bench.set(kst(days_after(5), 16, 0));
    bench.close_day().await;
    let memo = bench.memo_of(trade).await;
    assert_eq!(memo["state"], "settled", "{memo}");
    assert_eq!(memo["return_pct"].as_f64(), Some(10.0), "{memo}");
    assert_eq!(memo["hold_days"], 5);
    assert_eq!(memo["lesson"], LESSON);
    assert_eq!(memo["verdict"], "right", "수익이 나면 맞음: {memo}");
    assert!(
        memo.get("excess_pct").is_none_or(Value::is_null),
        "기준 지수 시세가 없으면 비운다: {memo}"
    );
    assert_eq!(
        memo_events(&bench, trade),
        vec![json!({"trade": trade.0, "state": "settled"})]
    );

    // 다시 마감해도 한 번만 바뀐다.
    bench.close_day().await;
    assert_eq!(memo_events(&bench, trade).len(), 1);
    assert_eq!(bench.outside_ai_requests().await, 0);
    bench.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn 정산_날_종가가_없으면_기다리다_5거래일_더_지나면_정산_못_함이_된다() {
    // 검증: E54
    let bench = Bench::start().await;
    bench.script(DemoTurn::Review, &lesson_reply(LESSON));
    let trade = bench.judged_buy(MEMO_SYMBOL).await;
    bench.close_on(decision_day(), MEMO_SYMBOL, 100_000);

    bench.set(kst(days_after(7), 16, 0));
    bench.close_day().await;
    assert_eq!(
        bench.memo_of(trade).await["state"],
        "pending",
        "시세를 기다린다"
    );

    bench.set(kst(days_after(10), 16, 0));
    bench.close_day().await;
    let memo = bench.memo_of(trade).await;
    assert_eq!(memo["state"], "unsettled", "{memo}");
    assert_eq!(memo["unsettled_reason"], "no_price", "{memo}");
    assert_eq!(memo["verdict"], "unknown", "{memo}");
    assert!(memo.get("lesson").is_none_or(Value::is_null), "{memo}");
    assert_eq!(
        memo_events(&bench, trade),
        vec![json!({"trade": trade.0, "state": "unsettled"})]
    );
    bench.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn 교훈은_하루_ai_상한의_팀장_예비분을_쓰지_않아_상한이면_정산_대기로_남는다() {
    // 검증: N20
    // 상한 2번이면 팀장 예비분(최소 2번)을 빼고 남는 몫이 없다.
    // (54차 대조) 상한이 턴 단위면 상한 2에서는 논의가 팀장 몫만으로 돌아 사기로 판단되지 않는다. 사기 판단을 먼저
    // 만들고 상한을 정한다(정산 날은 새로 센다).
    let bench = Bench::start().await;
    let trade = bench.judged_buy(MEMO_SYMBOL).await;
    let bench = bench.with_ai_limit(2).await;
    bench.script(DemoTurn::Review, &lesson_reply(LESSON));
    bench.close_on(decision_day(), MEMO_SYMBOL, 100_000);
    bench.close_on(days_after(5), MEMO_SYMBOL, 90_000);

    bench.set(kst(days_after(5), 16, 0));
    bench.close_day().await;
    let memo = bench.memo_of(trade).await;
    assert_eq!(
        memo["state"], "pending",
        "교훈을 받지 못하면 다음에 다시 한다: {memo}"
    );
    assert_eq!(bench.prompts_with("[어제 메모]"), 0, "교훈을 묻지 않는다");
    assert!(memo_events(&bench, trade).is_empty());
    bench.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn 팔기_판단은_부호를_뒤집어_정산하고_틀리면_틀림이다() {
    // 검증: V70
    let bench = Bench::start().await;
    bench.script(DemoTurn::Review, &lesson_reply(LESSON));
    bench.script(
        DemoTurn::Verdict,
        &json!({
            "decision": "sell", "hypothesis": "시험 가설 고점", "trigger": "시험 계기",
            "entry": 100_000, "stop": 105_000, "target": 90_000, "expected_value": 5_000
        })
        .to_string(),
    );
    let discussed = bench.discussed(MEMO_SYMBOL).await;
    assert_eq!(discussed.outcome.decision, pi_agent::Decision::Sell);
    let trade = discussed.trade;
    assert_eq!(bench.memo_of(trade).await["decision"], "sell");

    bench.close_on(decision_day(), MEMO_SYMBOL, 100_000);
    bench.close_on(days_after(5), MEMO_SYMBOL, 105_000);
    bench.set(kst(days_after(5), 16, 0));
    bench.close_day().await;
    let memo = bench.memo_of(trade).await;
    assert_eq!(memo["state"], "settled", "{memo}");
    assert_eq!(memo["return_pct"].as_f64(), Some(-5.0), "{memo}");
    assert_eq!(memo["verdict"], "wrong", "{memo}");
    bench.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn 다음_논의의_브리핑에_같은_종목_결정과_다른_종목_교훈이_들어간다() {
    // 검증: N20
    let bench = Bench::start().await;
    bench.script(DemoTurn::Review, &lesson_reply(LESSON));
    let trade = bench.judged_buy(MEMO_SYMBOL).await;
    bench.close_on(decision_day(), MEMO_SYMBOL, 100_000);
    bench.close_on(days_after(5), MEMO_SYMBOL, 110_000);
    bench.set(kst(days_after(5), 16, 0));
    bench.close_day().await;
    assert_eq!(bench.memo_of(trade).await["state"], "settled");

    let before = bench.prompts_with(LESSON);
    let dated = bench.prompts_with(&format!("{} 사기", ymd(decision_day())));
    bench.set(kst(days_after(6), 10, 0));
    bench.judged_buy(MEMO_SYMBOL).await;
    assert!(
        bench.prompts_with(LESSON) > before,
        "같은 종목 논의의 브리핑에 지난 교훈이 들어간다"
    );
    assert!(
        bench.prompts_with(&format!("{} 사기", ymd(decision_day()))) > dated,
        "같은 종목은 결정(날짜 · 판단)도 들어간다"
    );

    let before = bench.prompts_with(LESSON);
    bench.judged_buy(OTHER_SYMBOL).await;
    assert!(
        bench.prompts_with(LESSON) > before,
        "다른 종목 논의에는 교훈만 들어간다"
    );
    bench.close().await;
}
