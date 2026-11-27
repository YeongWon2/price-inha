//! 오류로 끝난 논의의 쓰임: 판단 글에 지침을 넣었어도 논의가 오류로 끝나면 그 매매의 쓰임을 장부에서 지운다.
//!
//! 오류는 이렇게 만든다: 그 종목 시세를 넣지 않고 논의한다 — 팀장이 사기로 판단한 뒤 매매 담당의 주문 안을 주문 길에
//! 넘길 때 시세가 없어 도구가 실패하고, 논의가 오류로 끝난다(판단은 저장된 채).

use pi_agent::GUIDELINE_BLOCK_TITLE;
use pi_journal::Mistake;
use pi_store::TradeState;
use serde_json::Value;

use crate::common::{Bench, HYNIX};

fn pending_count(ledger: &Value) -> usize {
    ledger["pending"].as_array().map_or(0, Vec::len)
}

// 검증: V103 R8 (오류로 끝난 논의는 그 매매의 쓰임을 남기지 않는다)
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn 논의가_오류로_끝나면_넣었던_지침의_쓰임이_장부에서_지워진다() {
    let bench = Bench::start().await;
    bench.reviewed_with(3, &Mistake::ChasedEntry);
    bench.close_day().await;

    let result = bench.discuss_unquoted(HYNIX).await;
    assert!(
        result.is_err(),
        "주문 안을 넘기지 못해 논의가 오류로 끝난다"
    );
    assert!(
        bench.prompts_with(GUIDELINE_BLOCK_TITLE) > 0,
        "논의를 열 때 판단 글에 지침은 들었다"
    );
    let judged = bench.store().trades_in_state(TradeState::Judged).unwrap();
    assert_eq!(judged.len(), 1, "판단은 저장된 채다");
    let ledger = bench.ledger();
    assert_eq!(pending_count(&ledger), 0, "쓰임이 남지 않는다: {ledger}");
    bench.close().await;
}
