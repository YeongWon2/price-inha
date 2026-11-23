//! 화면 단위 시험: 설정 화면(`/`)과 다른 기기의 키 화면(`/keys`)에 바깥 키 칸이 있고,
//! 발급한 접속 키는 일정 시간 뒤 화면에서 지운다.

use price_inha::{
    KEY_BACKUP_GITHUB_TOKEN, KEY_DART, KEY_ECOS, KEY_FRED, KEYS_PAGE_PATH, REMOTE_KEYS_PATH,
    SETUP_KEYS_PATH, SETUP_SLOTS_PATH,
};

use crate::common::{Bench, get_with, session};

const ITEMS: [&str; 4] = [KEY_DART, KEY_ECOS, KEY_FRED, KEY_BACKUP_GITHUB_TOKEN];

/// 값 입력 칸: 가려 쓰고, 브라우저가 기억하거나 맞춤법을 보지 않는다.
fn secret_input(prefix: &str, item: &str) -> String {
    format!(
        "<input id=\"{prefix}-slot-{item}\" type=\"password\" autocomplete=\"new-password\" spellcheck=\"false\">"
    )
}

#[tokio::test]
async fn settings_screen_has_input_and_button_for_each_slot() {
    let bench = Bench::start(true).await;
    let page = get_with(bench.addr(), "/", None).await;
    assert_eq!(page.status, 200);
    // 11차(Vite 이전): 스크립트가 묶음으로 나가므로 받은 화면 뒤에 그 쪽이 부르는 묶음을 이어 본다.
    let text = crate::screen_served::served_page(bench.addr(), &page.text).await;
    assert!(text.contains(SETUP_KEYS_PATH));
    assert!(text.contains(SETUP_SLOTS_PATH), "칸별 등록 여부를 읽는다");
    for item in ITEMS {
        assert!(text.contains(&secret_input("f", item)), "{item} 입력 칸");
        assert!(
            text.contains(&format!("id=\"b-slot-{item}\"")),
            "{item} 등록 단추"
        );
        assert!(
            text.contains(&format!("id=\"reg-slot-{item}\"")),
            "{item} 등록 여부"
        );
    }
    bench.close().await;
}

#[tokio::test]
async fn remote_keys_screen_has_input_and_button_for_each_slot() {
    let bench = Bench::start(true).await;
    let cookie = session(bench.addr(), None).await;
    let page = get_with(bench.addr(), KEYS_PAGE_PATH, Some(&cookie)).await;
    assert_eq!(page.status, 200);
    // 11차(Vite 이전): 스크립트가 묶음으로 나가므로 받은 화면 뒤에 그 쪽이 부르는 묶음을 이어 본다.
    let text = crate::screen_served::served_page(bench.addr(), &page.text).await;
    assert!(text.contains(REMOTE_KEYS_PATH));
    assert!(text.contains(SETUP_SLOTS_PATH), "칸별 등록 여부를 읽는다");
    for item in ITEMS {
        assert!(text.contains(&secret_input("k", item)), "{item} 입력 칸");
        assert!(
            text.contains(&format!("id=\"k-slot-{item}-button\"")),
            "{item} 등록 단추"
        );
        assert!(
            text.contains(&format!("id=\"k-slot-{item}-reg\"")),
            "{item} 등록 여부"
        );
    }
    bench.close().await;
}

#[tokio::test]
async fn issued_access_key_is_wiped_from_screen_after_sixty_seconds() {
    let bench = Bench::start(true).await;
    let page = get_with(bench.addr(), "/", None).await;
    // 11차(Vite 이전): 스크립트가 묶음으로 나가므로 받은 화면 뒤에 그 쪽이 부르는 묶음을 이어 본다.
    // 빌드는 숫자 60000을 같은 값 `6e4`로 쓴다(값은 같다).
    let text = crate::screen_served::served_page(bench.addr(), &page.text).await;
    assert!(
        text.contains("ACCESS_KEY_SHOW_MS = 60000;") || text.contains("ACCESS_KEY_SHOW_MS = 6e4;"),
        "발급한 키를 보여 두는 시간은 60초"
    );
    assert!(
        text.contains("setTimeout(hideAccessKey, ACCESS_KEY_SHOW_MS)"),
        "그 시간이 지나면 화면에서 지운다"
    );
    bench.close().await;
}
