//! `price-inha setup ai` 구역 끝에 "담당 모델은 설정 화면에서 바꿀 수 있습니다" 한 줄만 보인다(마법사에서 고르게
//! 하지는 않는다). 마법사는 도는 서버(이 보관 폴더 · 이 포트)에 붙어 같은 프로세스에서 돈다.

use std::io::Cursor;

use price_inha::{ANSWER_SKIP, SETUP_MODELS_HINT, SetupContext, SetupRequest, SetupSection, setup};

use crate::common::{Bench, settings};

// 검증: R18-4 C32
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn 마법사_ai_구역_끝에_담당_모델은_설정_화면에서_바꾼다는_한_줄이_나온다() {
    let bench = Bench::start().await;
    let settings = settings(bench.home(), bench.addr().port());
    let request = SetupRequest {
        section: Some(SetupSection::Ai),
        stdin_answers: true,
        dry_run: false,
        args: Vec::new(),
    };
    let mut input = Cursor::new(format!("{ANSWER_SKIP}\n").into_bytes());
    let mut output: Vec<u8> = Vec::new();
    let env = |_: &str| -> Option<String> { None };
    let context = SetupContext {
        input: &mut input,
        output: &mut output,
        env: &env,
        interactive: false,
        server_exe: std::path::PathBuf::from("/nonexistent/price-inha"),
    };
    let code = tokio::time::timeout(crate::common::STEP, setup(&settings, &request, context))
        .await
        .expect("마법사가 시간 안에 끝나야 한다");
    let shown = String::from_utf8_lossy(&output).to_string();
    assert_eq!(code, 0, "{shown}");
    assert_eq!(
        shown.matches(SETUP_MODELS_HINT).count(),
        1,
        "AI 구역 끝에 한 줄: {shown}"
    );
    let hint_at = shown.find(SETUP_MODELS_HINT).unwrap();
    let ai_title = shown.find("AI 연결").expect("AI 연결 구역이 돌았다");
    assert!(hint_at > ai_title, "구역 안(뒤)에 보인다: {shown}");
    bench.close().await;
}
