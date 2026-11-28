//! 발판: 실행 파일의 `--demo` 조립으로 임시 보관 폴더에 서버가 뜨고 꺼진다(지금 구현에서 선다).

use price_inha::App;

use crate::common::{Outside, STEP, assembly, demo_run_settings};

// 검증: V54, C33
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 발판_시연_조립으로_뜨고_꺼진다() {
    let dir = tempfile::tempdir().unwrap();
    let outside = Outside::start().await;
    let settings = demo_run_settings(dir.path());
    let built = assembly(&settings, &outside);
    let app = tokio::time::timeout(STEP, App::launch_run(settings, built))
        .await
        .expect("서버가 시간 안에 떠야 한다")
        .expect("서버가 떠야 한다");
    assert_ne!(app.local_addr().port(), 0, "빈 포트로 열려야 한다");
    app.shutdown().await.expect("꺼져야 한다");
}
