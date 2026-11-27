//! 발판 시험: 이 폴더의 도우미가 지금 구현에서 서는가.

use price_inha::{CheckItem, InstallOptions, Os, SETUP_SEMANTIC_PATH, Service, doctor};

use crate::common::{Bench, ScriptedRunner, plain_settings, setup};

// 검증: D10 V78
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn 발판은_시연_서버를_띄워_뜻_검색_설정을_읽고_끈다() {
    let bench = Bench::start().await;
    let shown = bench.get(SETUP_SEMANTIC_PATH).await;
    assert_eq!(shown.status, 200, "{}", shown.text);
    assert_eq!(shown.body["enabled"], false);
    bench.close().await;
}

// 검증: V78
#[test]
fn 발판은_가짜_실행기로_정의를_만들고_진단한다() {
    let home = tempfile::tempdir().unwrap();
    let defs = tempfile::tempdir().unwrap();
    let runner = ScriptedRunner::ok();
    let service = Service::new(setup(Os::Linux, home.path(), defs.path(), &[]), &runner);
    let report = service
        .install(InstallOptions {
            start_now: false,
            dry_run: true,
        })
        .unwrap();
    assert!(report.definition.unwrap().contents.contains("ExecStart="));
    let report = doctor(&plain_settings(home.path()), &service);
    assert!(report.check(CheckItem::Service).is_some());
}
