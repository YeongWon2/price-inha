//! 발판이 지금 구현에서 서는가: 서버가 뜨고, 그 컴퓨터 세션으로 키 화면이 열리고, 다른 기기로 보이게 할 수 있다.

use price_inha::KEYS_PAGE_PATH;

use crate::common::{Bench, fake_ai, get, session_cookie};

// 검증: C16
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 발판이_서고_그_컴퓨터_세션으로_키_화면이_열린다() {
    let ai = fake_ai().await;
    let bench = Bench::start(&ai.uri()).await;
    let addr = bench.addr();
    let cookie = session_cookie(addr, None)
        .await
        .expect("그 컴퓨터는 키 없이 세션을 받는다");
    let page = get(addr, KEYS_PAGE_PATH, Some(&cookie), false).await;
    assert_eq!(page.status, 200, "{}", page.text());
    assert!(
        page.content_type.starts_with("text/html"),
        "{}",
        page.content_type
    );
    // 다른 기기로 보이게 하면 그 컴퓨터 세션으로는 열리지 않는다(관문이 원격 세션만 친다).
    bench.go_remote();
    let outside = get(addr, KEYS_PAGE_PATH, Some(&cookie), false).await;
    assert_eq!(outside.status, 401, "{}", outside.text());
    bench.close().await;
}
