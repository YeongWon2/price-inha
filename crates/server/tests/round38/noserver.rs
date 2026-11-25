//! 도는 서버가 없을 때: 마법사는 "지금 띄울까요?"를 묻고(기본 예) 뒤에서 띄운 서버의 설정 경로로 저장한 뒤,
//! 서비스로 등록하지 않았으니 끝날 때 끈다. 아니오면 아무것도 저장하지 않고 `price-inha run`을 안내한다.
//! 띄운 서버는 키가 없어 바깥을 부르지 않는다(이 시험은 키 구역을 돌지 않는다).

use std::net::TcpStream;
use std::time::Duration;

use price_inha::{
    SETUP_DONE, SETUP_FAILED_EXIT, SETUP_RUN_HINT, SETUP_START_SERVER_QUESTION, WATCHLIST_FILE,
    load_watchlist,
};

use crate::common::{FakeCli, Login, Root, free_port};
use crate::run::{Env, run_binary, strings};

fn symbols_args(root: &Root, port: u16) -> Vec<String> {
    strings(&[
        "setup",
        "symbols",
        "--stdin-answers",
        "--home",
        root.home_str(),
        "--port",
        &port.to_string(),
    ])
}

/// 그 루프백 포트에 아무도 듣지 않는다.
fn nobody_listens(port: u16) -> bool {
    TcpStream::connect_timeout(
        &std::net::SocketAddr::from(([127, 0, 0, 1], port)),
        Duration::from_millis(500),
    )
    .is_err()
}

// 검증: D1 V31
#[tokio::test(flavor = "multi_thread")]
async fn 서버가_없으면_띄울지_묻고_띄운_서버로_저장한_뒤_끈다() {
    let root = Root::new();
    let cli = FakeCli::new(Login::Missing);
    let port = free_port();
    let ran = run_binary(
        Env::new(&root, &cli),
        symbols_args(&root, port),
        Some("y\n005930\n".to_string()),
    )
    .await;

    assert!(
        ran.stdout.contains(SETUP_START_SERVER_QUESTION),
        "띄울지 묻는다: {}",
        ran.all()
    );
    assert_eq!(ran.code, Some(0), "{}", ran.all());
    assert_eq!(
        ran.result_of("symbols").as_deref(),
        Some(SETUP_DONE),
        "{}",
        ran.stdout
    );
    let saved: Vec<String> = load_watchlist(&root.home)
        .unwrap()
        .iter()
        .map(|s| s.as_str().to_string())
        .collect();
    assert_eq!(saved, ["005930"], "띄운 서버가 저장했다");
    assert!(
        nobody_listens(port),
        "서비스로 등록하지 않았으니 마법사가 띄운 서버는 끝날 때 꺼진다"
    );
}

// 검증: D1
#[tokio::test(flavor = "multi_thread")]
async fn 서버를_띄우지_않으면_저장하지_않고_run을_안내한다() {
    let root = Root::new();
    let cli = FakeCli::new(Login::Missing);
    let port = free_port();
    let ran = run_binary(
        Env::new(&root, &cli),
        symbols_args(&root, port),
        Some("n\n005930\n".to_string()),
    )
    .await;

    assert!(
        ran.stdout.contains(SETUP_START_SERVER_QUESTION),
        "{}",
        ran.all()
    );
    assert_eq!(
        ran.code,
        Some(i32::from(SETUP_FAILED_EXIT)),
        "{}",
        ran.all()
    );
    assert!(ran.stdout.contains(SETUP_RUN_HINT), "{}", ran.all());
    assert!(
        !root.home.join(WATCHLIST_FILE).exists(),
        "마법사는 파일을 직접 쓰지 않는다"
    );
    assert!(nobody_listens(port), "서버를 띄우지 않았다");
}
