//! 체험 몇 분 안에 하루 흐름(논의 → 판단 → 모의 주문 → 체결)이 보이게: 템플릿을 하나도 고르지 않은 시연 보관 폴더면
//! `demo`가 스윙 템플릿(모의중)을 하나 골라 둔다(쓰는 템플릿이 없으면 팀이 아무것도 걸지 않는다 — team/poll 문서).
//! 이미 고른 템플릿이 있으면 그대로 둔다.

use pi_store::TemplateState;
use price_inha::{DEMO_TEMPLATE_NOTICE, DemoRequest, start_demo};

use crate::common::{Outside, Recorder, STEP, assembly, demo_run_settings};

/// 같은 보관 폴더로 한 번 띄웠다 끄고 (찍은 글, 모든 템플릿 수, 모의중 템플릿의 스타일들)을 돌려준다.
async fn start_once(home: &std::path::Path) -> (String, usize, Vec<String>) {
    let outside = Outside::start().await;
    let settings = demo_run_settings(home);
    let built = assembly(&settings, &outside);
    let recorder = Recorder::default();
    let mut printed = Vec::new();
    let request = DemoRequest { no_open: true };
    let app = tokio::time::timeout(
        STEP,
        start_demo(settings, built, &request, &mut printed, &recorder),
    )
    .await
    .expect("시간 안에 떠야 한다")
    .expect("시연 서버가 떠야 한다");
    let store = app.store();
    let mut total = 0;
    for state in TemplateState::ALL {
        total += store.templates_in_state(state).unwrap().len();
    }
    let styles = store
        .templates_in_state(TemplateState::Simulating)
        .unwrap()
        .iter()
        .map(|template| template.value["style"].as_str().unwrap_or("").to_string())
        .collect();
    app.shutdown().await.expect("꺼져야 한다");
    (String::from_utf8(printed).unwrap(), total, styles)
}

// 검증: V54, C4
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 템플릿이_없는_시연_폴더면_스윙_템플릿을_하나_골라_둔다() {
    let dir = tempfile::tempdir().unwrap();
    let (printed, total, styles) = start_once(dir.path()).await;
    assert_eq!(total, 1, "템플릿 하나를 골라 둔다");
    assert_eq!(styles, vec!["swing".to_string()], "모의중 스윙 템플릿");
    assert!(
        printed.contains(DEMO_TEMPLATE_NOTICE),
        "골라 둔 것을 알린다: {printed}"
    );
}

// 검증: V54, C4
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 이미_고른_템플릿이_있으면_더_만들지_않는다() {
    let dir = tempfile::tempdir().unwrap();
    let _ = start_once(dir.path()).await;
    let (printed, total, _) = start_once(dir.path()).await;
    assert_eq!(total, 1, "다시 띄워도 템플릿은 하나다");
    assert!(
        !printed.contains(DEMO_TEMPLATE_NOTICE),
        "새로 고르지 않았으면 알리지 않는다: {printed}"
    );
}
