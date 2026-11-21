//! 10b 보강 2번(검토 B-2, 10번 그대로): 설정 화면(`web/index.html`)과 `/keys`(`web/keys.html`)가 새 칸 목록 경로
//! (`/api/setup/providers`)를 읽어 공급자 칸을 그리고, 옛 "자료 키"(`item: "data"`) 칸은 두 화면에서 빠진다.
//!
//! 실행 파일과 같은 길(`App`)로 띄워 받은 두 화면의 글을 문자열로 본다.

use price_inha::{KEYS_PAGE_PATH, SETUP_PROVIDERS_PATH};

use crate::common::{Plan, get, kst, monday};

/// 옛 자료 키 등록 item이 화면 글에 있나(따옴표 두 가지 · 옛 입력 칸 id).
fn has_old_data_item(page: &str) -> bool {
    page.contains("\"data\"") || page.contains("'data'") || page.contains("f-data-key")
}

// 검증: C3 E4 D9
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 설정_화면과_키_화면은_새_칸_목록_경로를_읽고_옛_자료_키_칸이_없다() {
    let bench = Plan::demo().start(kst(monday(), 10, 0)).await;
    for path in ["/", KEYS_PAGE_PATH] {
        let page = get(bench.addr(), path).await;
        assert_eq!(page.status, 200, "{path} 화면을 받는다");
        assert!(
            page.text.contains(SETUP_PROVIDERS_PATH),
            "{path} 화면이 공급자 칸 목록({SETUP_PROVIDERS_PATH})을 읽는다"
        );
        assert!(
            !has_old_data_item(&page.text),
            "{path} 화면에 옛 자료 키(item \"data\") 칸이 없다"
        );
    }
    bench.close().await;
}
