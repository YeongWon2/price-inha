//! 옛 폰 화면 정리(결정 1 · server 11차 뒤): 화면은 `crates/server/web`(Vite)으로 옮겼으니
//! 이 크레이트에는 화면 자산 폴더(`web/`)가 남지 않는다.

#[test]
fn 크레이트_아래에_옛_화면_폴더_web이_없다() {
    let web = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("web");
    assert!(!web.exists(), "옛 화면 폴더가 남아 있다: {}", web.display());
}
