//! 지침 성적의 서버 끝: 지침이 든 논의 → 사기 → 익절(손절)로 팔림 → 하루 정리에서 그 지침의 익절(손절)이 오른다.

use pi_agent::GUIDELINE_BLOCK_TITLE;
use pi_journal::Mistake;
use pi_rules::Purpose;
use price_inha::GUIDELINES_PATH;
use serde_json::{Value, json};

use crate::common::{Bench, ENTRY, HYNIX};

/// 같은 실수 세 번으로 지침 하나를 올리고, 그 지침이 든 논의로 산 매매를 보유중까지 만든다.
async fn held_with_guideline(bench: &Bench) -> pi_store::TradeId {
    bench.reviewed_with(3, &Mistake::ChasedEntry);
    bench.close_day().await;
    let trade = bench.hold_from_discussion(HYNIX).await;
    assert!(
        bench.prompts_with(GUIDELINE_BLOCK_TITLE) > 0,
        "판단 글에 팀 지침이 든다"
    );
    let ledger = bench.ledger();
    assert!(
        pending_has(&ledger, trade.0),
        "넣은 매매가 쓰임으로 남는다: {ledger}"
    );
    trade
}

fn pending_has(ledger: &Value, trade: i64) -> bool {
    ledger["pending"]
        .as_array()
        .is_some_and(|list| list.iter().any(|use_| use_["trade"] == json!(trade)))
}

// 검증: V99 V103 R8 (지침을 따른 매매가 익절로 끝나면 그 지침의 쓰임 · 익절이 하루 정리에서 오른다)
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn 지침이_든_논의로_사서_익절로_팔리면_하루_정리에서_그_지침의_익절이_오른다() {
    let bench = Bench::start().await;
    let trade = held_with_guideline(&bench).await;
    bench
        .sell(trade, HYNIX, Purpose::TakeProfit, ENTRY + 4_000)
        .await;
    bench.close_day().await;

    let ledger = bench.ledger();
    let guideline = &ledger["guidelines"][0];
    assert_eq!(guideline["uses"], json!(1), "{ledger}");
    assert_eq!(guideline["wins"], json!(1), "{ledger}");
    assert_eq!(guideline["losses"], json!(0), "{ledger}");
    assert!(
        !pending_has(&ledger, trade.0),
        "센 매매는 쓰임 목록에서 빠진다"
    );
    let reply = bench.get(GUIDELINES_PATH).await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    assert_eq!(
        reply.body["active"][0]["win_rate_pct"],
        json!(100),
        "{}",
        reply.body
    );
    bench.close().await;
}

// 검증: V99 V103 R8 (손절로 끝나면 그 지침의 손절이 오른다)
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn 지침이_든_논의로_사서_손절로_팔리면_하루_정리에서_그_지침의_손절이_오른다() {
    let bench = Bench::start().await;
    let trade = held_with_guideline(&bench).await;
    bench
        .sell(trade, HYNIX, Purpose::StopLoss, ENTRY - 3_000)
        .await;
    bench.close_day().await;

    let ledger = bench.ledger();
    let guideline = &ledger["guidelines"][0];
    assert_eq!(guideline["uses"], json!(1), "{ledger}");
    assert_eq!(guideline["wins"], json!(0), "{ledger}");
    assert_eq!(guideline["losses"], json!(1), "{ledger}");
    let reply = bench.get(GUIDELINES_PATH).await;
    assert_eq!(
        reply.body["active"][0]["win_rate_pct"],
        json!(0),
        "{}",
        reply.body
    );
    bench.close().await;
}
