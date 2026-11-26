//! 발판 확인: 이 폴더의 띄우기가 지금 구현에서 서는가(서버가 뜨고, 시연 가짜 AI로 논의가 돌고, 잔량을 읽고,
//! 팀장의 답을 정해 둘 수 있다). 이것이 서지 않으면 아래 시험들의 실패는 기능이 아니라 발판 탓이다.
//!
//! 시연 판단이 사기면 이 발판에는 고른 템플릿이 없어 주문 길에서 오류로 끝난다(사기로 판단돼 주문 길까지 갔다는
//! 뜻). 그래서 논의 결과를 보는 시험은 팀장 답을 지켜보기로 정해 둔다.

use pi_agent::Decision;
use price_inha::{DemoTurn, TeamError};
use serde_json::Value;

use crate::common::{BUY_VERDICT, Bench, WATCH_VERDICT, budget, discuss_result, settled_prompts};

// 검증: C22 V64
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 발판_서버가_뜨고_논의가_돌고_잔량을_읽는다() {
    let bench = Bench::start().await;
    let before = settled_prompts(&bench).await;
    bench.demo_ai().script(DemoTurn::Verdict, WATCH_VERDICT);
    let done = discuss_result(&bench, "발판 확인").await;
    assert!(
        done.as_ref()
            .is_ok_and(|d| matches!(d.outcome.decision, Decision::Watch(_))),
        "상한이 없으면 논의가 열리고 팀장 답대로 지켜본다: {:?}",
        done.as_ref().map(|d| d.trade)
    );
    assert!(bench.prompts().len() > before, "논의가 가짜 AI를 불렀다");
    let now = budget(&bench).await;
    assert_eq!(now["limit"], Value::Null, "{now}");
    assert_eq!(
        bench.outside_ai_requests().await,
        0,
        "바깥 AI는 부르지 않는다"
    );
    bench.close().await;
}

// 검증: V53 E27
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 발판_팀장_사기_답을_정해_두면_상한이_없을_때_주문_길까지_간다() {
    let bench = Bench::start().await;
    settled_prompts(&bench).await;
    bench.demo_ai().script(DemoTurn::Verdict, BUY_VERDICT);
    match discuss_result(&bench, "사기 발판").await {
        Ok(done) => assert!(
            matches!(done.outcome.decision, Decision::Buy),
            "점수가 모두 나오면 팀장의 사기 답대로 사기다: {:?}",
            done.outcome.decision
        ),
        Err(TeamError::Agent(text)) => assert!(
            text.contains("템플릿"),
            "사기로 판단돼 주문 길(템플릿 없음)까지 갔다: {text}"
        ),
        Err(other) => panic!("사기로 판단돼야 한다: {other}"),
    }
    bench.close().await;
}
