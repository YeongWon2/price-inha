//! 설정 화면의 "접속 키" 칸(V40 · V41): 발급 · 잠금 풀기 · 접속 기록 경로를 부른다.

use price_inha::{ACCESS_KEY_PATH, ACCESS_LOG_PATH, ACCESS_UNLOCK_PATH};

use crate::common::{Bench, get_with};

#[tokio::test]
async fn settings_screen_has_access_key_section() {
    let bench = Bench::start().await;
    let page = get_with(bench.addr(), "/", None).await;
    assert_eq!(page.status, 200);
    // 11차(Vite 이전): 스크립트가 묶음으로 나가므로 받은 화면 뒤에 그 쪽이 부르는 묶음을 이어 본다.
    let text = crate::screen_served::served_page(bench.addr(), &page.text).await;
    for needle in [
        ACCESS_KEY_PATH,
        ACCESS_UNLOCK_PATH,
        ACCESS_LOG_PATH,
        "접속 키 만들기",
    ] {
        assert!(text.contains(needle), "첫 화면에 {needle}가 있어야 한다");
    }
    bench.close().await;
}
