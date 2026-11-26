//! 시연 기록으로 지침이 오르고, 장부 파일 · 이벤트에 남고, 다음 논의의 판단 글에 든다.

use pi_agent::{GUIDELINE_BLOCK_NOTE, GUIDELINE_BLOCK_TITLE};
use pi_journal::Mistake;
use price_inha::{KIND_GUIDELINE, TEAM_GUIDELINES_FILE};
use serde_json::json;

use crate::common::{Bench, MEMO_SYMBOL, events};

// 검증: V97 V104 R8 (시연 기록: 같은 실수 세 번 → 하루 마감에 지침이 오르고 0600 장부 파일 · 이벤트에 남는다)
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn 같은_실수_세_번이면_하루_마감에_지침이_오르고_장부와_이벤트에_남는다() {
    let bench = Bench::start().await;
    bench.reviewed_with(3, &Mistake::ChasedEntry);
    bench.close_day().await;

    let ledger = bench.ledger();
    let list = ledger["guidelines"].as_array().expect("장부에 지침 목록");
    assert_eq!(list.len(), 1, "{ledger}");
    assert_eq!(list[0]["kind"], json!("avoid"));
    assert_eq!(list[0]["state"], json!("active"));
    assert_eq!(list[0]["key"], json!("mistake:chased_entry"));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(bench.home().join(TEAM_GUIDELINES_FILE))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600, "장부 파일은 주인만 읽고 쓴다");
    }
    let id = list[0]["id"].as_str().unwrap().to_string();
    let found = events(&bench.store(), KIND_GUIDELINE);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0]["action"], json!("promoted"));
    assert_eq!(found[0]["id"], json!(id));
    assert_eq!(found[0]["kind"], json!("avoid"));
    assert_eq!(found[0]["why"], json!("mistake"));
    assert!(
        found[0].get("text").is_none(),
        "이벤트에는 지침 글을 담지 않는다"
    );

    bench.close_day().await;
    assert_eq!(
        events(&bench.store(), KIND_GUIDELINE).len(),
        1,
        "같은 근거로 다시 마감해도 바뀐 것이 없다"
    );
    bench.close().await;
}

// 검증: V103 R8 (지침이 오른 뒤의 논의: 판단 글에 "팀 지침" 블록 · 넣은 매매가 장부의 쓰임으로 남는다)
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn 지침이_오른_뒤의_논의는_판단_글에_팀_지침이_들고_쓰임으로_남는다() {
    let bench = Bench::start().await;
    bench.reviewed_with(3, &Mistake::ChasedEntry);
    bench.close_day().await;
    let text = bench.ledger()["guidelines"][0]["text"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(!text.is_empty());

    let trade = bench.judged_buy(MEMO_SYMBOL).await;
    assert!(
        bench.prompts_with(GUIDELINE_BLOCK_TITLE) > 0,
        "블록 머리가 든다"
    );
    assert!(
        bench.prompts_with(GUIDELINE_BLOCK_NOTE) > 0,
        "참고 한 줄이 든다"
    );
    assert!(bench.prompts_with(&text) > 0, "지침 글이 든다");

    let ledger = bench.ledger();
    let pending = ledger["pending"].as_array().expect("쓰임 목록");
    assert!(
        pending.iter().any(|use_| use_["trade"] == json!(trade.0)),
        "넣은 매매가 쓰임으로 남는다: {ledger}"
    );
    assert_eq!(ledger["today_buys"], json!(1), "사자 판단 하나: {ledger}");
    bench.close().await;
}

// 검증: V103 R8 (지침이 없으면 판단 글에 블록이 없다)
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn 지침이_없으면_판단_글에_블록이_없다() {
    let bench = Bench::start().await;
    let _ = bench.judged_buy(MEMO_SYMBOL).await;
    assert_eq!(bench.prompts_with(GUIDELINE_BLOCK_TITLE), 0);
    assert_eq!(bench.prompts_with(GUIDELINE_BLOCK_NOTE), 0);
    bench.close().await;
}

// 검증: V98 R7-2 R8 (한도를 바꾸라는 되풀이는 지침이 되지 못하고 1층 길로 보냈다는 이벤트만 남는다)
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn 한도를_바꾸라는_되풀이는_지침이_되지_못한다() {
    let bench = Bench::start().await;
    bench.reviewed_with(3, &Mistake::Other("손절 한도를 3%로 줄인다".into()));
    bench.close_day().await;
    let ledger = bench.ledger();
    let none = ledger["guidelines"]
        .as_array()
        .is_none_or(|list| list.is_empty());
    assert!(none, "장부에 오르지 않는다: {ledger}");
    let found = events(&bench.store(), KIND_GUIDELINE);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0]["action"], json!("to_rules"));
    assert_eq!(found[0]["why"], json!("layer_one"));
    assert_eq!(found[0]["id"], json!(null));
    bench.close().await;
}
