//! `price-inha demo`의 띄우기: 시연 모드로 뜨고, 안내와 사무실 주소를 찍고, 브라우저를 연다(가짜 실행기).

use price_inha::{DEMO_NOTICE, DemoRequest, start_demo};

use crate::common::{Outside, QUIET, Recorder, STEP, assembly, demo_run_settings};

/// 띄우고 (찍은 글, 부른 명령들, 사무실 주소)를 돌려준다. 바깥 요청이 0인지도 본다.
async fn start(no_open: bool) -> (String, Vec<price_inha::CommandLine>, String) {
    let dir = tempfile::tempdir().unwrap();
    let outside = Outside::start().await;
    let settings = demo_run_settings(dir.path());
    let built = assembly(&settings, &outside);
    let recorder = Recorder::default();
    let mut printed = Vec::new();
    let request = DemoRequest { no_open };
    let app = tokio::time::timeout(
        STEP,
        start_demo(settings, built, &request, &mut printed, &recorder),
    )
    .await
    .expect("시간 안에 떠야 한다")
    .expect("시연 서버가 떠야 한다");
    let office = format!("http://127.0.0.1:{}/office/", app.local_addr().port());
    tokio::time::sleep(QUIET).await;
    assert_eq!(outside.total().await, 0, "시연은 바깥으로 요청하지 않는다");
    app.shutdown().await.expect("꺼져야 한다");
    (
        String::from_utf8(printed).unwrap(),
        recorder.calls(),
        office,
    )
}

// 검증: V54, E56
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 시연_안내와_사무실_주소를_찍고_브라우저로_연다() {
    let (printed, calls, office) = start(false).await;
    assert!(
        DEMO_NOTICE.contains("시연 모드입니다") && DEMO_NOTICE.contains("Ctrl+C"),
        "안내 글: {DEMO_NOTICE}"
    );
    assert!(
        printed.contains(DEMO_NOTICE),
        "안내를 찍어야 한다: {printed}"
    );
    assert!(
        printed.contains(&office),
        "사무실 주소를 찍어야 한다: {printed}"
    );
    assert_eq!(
        calls.len(),
        1,
        "브라우저 여는 명령을 한 번 부른다: {calls:?}"
    );
    assert_eq!(
        calls[0].args.last().map(String::as_str),
        Some(office.as_str()),
        "사무실 주소를 연다"
    );
}

// 검증: V54, E56
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn no_open이면_브라우저를_부르지_않고_주소만_찍는다() {
    let (printed, calls, office) = start(true).await;
    assert!(
        printed.contains(DEMO_NOTICE),
        "안내를 찍어야 한다: {printed}"
    );
    assert!(
        printed.contains(&office),
        "사무실 주소를 찍어야 한다: {printed}"
    );
    assert!(calls.is_empty(), "브라우저를 부르지 않는다: {calls:?}");
}
