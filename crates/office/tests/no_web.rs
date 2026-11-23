//! 옛 화면 정리(office.md "옛 화면 정리" 4): 화면은 server(`crates/server/web`)로 옮겼고
//! 이 크레이트에는 화면 자산이 남지 않는다.

use std::path::Path;

#[test]
fn 크레이트에_옛_화면_폴더가_없다() {
    let web = Path::new(env!("CARGO_MANIFEST_DIR")).join("web");
    assert!(
        !web.exists(),
        "옛 화면 폴더가 남아 있다: {} — 화면은 crates/server/web에 있다",
        web.display()
    );
}
