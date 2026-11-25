//! 발판 확인: 이 폴더의 띄우기가 지금 구현에서 서는가(서버가 뜨고, 시연 가짜 AI로 논의가 실제로 돌아
//! `office.talk`가 나오는가). 이것이 서지 않으면 아래 시험들의 실패는 기능이 아니라 발판 탓이다.

use crate::common::{Bench, discuss, events, settled_prompts};

// 검증: V63
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 발판_서버가_뜨고_시연_논의가_돌아_팀_대화가_나온다() {
    let bench = Bench::start().await;
    let before = settled_prompts(&bench).await;
    discuss(&bench, "발판 확인").await;
    assert!(
        bench.prompts().len() > before,
        "논의가 가짜 AI를 불렀어야 한다"
    );
    assert!(
        !events(&bench.store(), price_inha::KIND_OFFICE_TALK).is_empty(),
        "논의 글이 사무실 팀 대화(office.talk)로 나와야 한다"
    );
    assert_eq!(
        bench.outside_ai_requests().await,
        0,
        "바깥 AI는 부르지 않는다"
    );
    bench.close().await;
}
