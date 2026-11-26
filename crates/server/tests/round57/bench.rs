//! 발판이 지금 구현에서 선다(구현 전에도 통과한다): 팀을 잇고 논의를 돌리면 가짜 AI가 담당 턴마다 요청의 `model`을
//! 받아 남긴다. 아무것도 바꾸지 않았으면 모두 조립의 기본 모델이다.

use crate::common::{Bench, default_model, models_of};

// 검증: R18-4 C32
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn 발판이_선다_논의를_돌리면_가짜_ai가_담당마다_기본_모델_이름을_받는다() {
    let bench = Bench::start().await;
    let from = bench.ai().settled().await;
    bench.discuss("발판 논의").await;
    let asked = bench.ai().asked_from(from);
    for role in ["scout", "strategy", "risk", "leader"] {
        let models = models_of(&asked, role);
        assert!(!models.is_empty(), "{role} 턴이 AI를 불렀다: {asked:?}");
        assert!(
            models.iter().all(|model| *model == default_model()),
            "{role}은 기본 모델: {models:?}"
        );
    }
    bench.close().await;
}
