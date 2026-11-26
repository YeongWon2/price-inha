//! 상한에 닿았을 때의 안전(기능 F 3번 · E52 · E27 · V53 · P2 · P11).
//!
//! - 상한 때문에 점수 턴을 기권한 담당이 생기면 팀장이 사기로 답해도 사지 않는다(지켜보기 · 합의 부족).
//! - 상한을 다 써도 전체 정지는 AI 없이 된다. (보유의 손절이 상한과 상관없이 나가는 것은 round22의
//!   `상한에_닿아도_손절은_그대로_나간다`가 같은 턴 단위로 계속 본다.)

use pi_agent::{Decision, WatchReason};
use pi_store::TeamState;
use price_inha::{DemoTurn, TEAM_STOP_PATH, TeamError};
use serde_json::json;

use crate::common::{BUY_VERDICT, Plan, discuss_result, morning, settled_prompts};

// 검증: V53 E27 E52 R1-5
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 상한_때문에_기권이_생기면_팀장이_사기로_답해도_사지_않는다() {
    let bench = Plan::demo().ai_limit(2).start(morning()).await;
    settled_prompts(&bench).await;
    bench.demo_ai().script(DemoTurn::Verdict, BUY_VERDICT);
    let done = discuss_result(&bench, "기권 시험")
        .await
        .expect("예비분이 남으면 논의는 열린다");
    assert!(
        matches!(
            done.outcome.decision,
            Decision::Watch(WatchReason::ConsensusBelow { .. })
        ),
        "점수를 못 낸 담당이 있으니 사지 않고 지켜본다: {:?}",
        done.outcome.decision
    );
    assert!(done.outcome.order.is_none(), "주문 안이 없다");
    assert!(done.placed.is_none(), "주문 길에 넣지 않는다");
    bench.close().await;
}

// 검증: E52 E30 P2
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 상한을_다_써도_전체_정지는_ai_없이_된다() {
    let bench = Plan::demo().ai_limit(1).start(morning()).await;
    settled_prompts(&bench).await;
    let _first = discuss_result(&bench, "첫째").await;
    let second = discuss_result(&bench, "둘째").await;
    assert!(matches!(second, Err(TeamError::AiLimit)), "상한에 닿았다");
    let asked = bench.prompts().len();
    let reply = bench.post(TEAM_STOP_PATH, &json!({})).await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    assert_eq!(bench.store().team_state().unwrap(), TeamState::FullStop);
    assert_eq!(
        bench.prompts().len(),
        asked,
        "전체 정지는 AI를 부르지 않는다"
    );
    bench.close().await;
}
