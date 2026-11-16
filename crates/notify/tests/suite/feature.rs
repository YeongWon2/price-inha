//! 시험용 `redirect_to`는 `test-redirect` feature 뒤에만 있다(notify 5차). 기본 빌드에는 없다.
//! 기본 빌드를 직접 컴파일할 수는 없으므로 소스와 Cargo.toml이 그렇게 묶였는지 본다.

use std::fs;
use std::path::Path;

fn read(rel: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(rel);
    fs::read_to_string(path).unwrap()
}

#[test]
fn 시험용_주소_바꾸기는_시험_feature_뒤에만_있다() {
    let src = read("src/transport.rs");
    let lines: Vec<&str> = src.lines().collect();
    let at = lines
        .iter()
        .position(|l| l.contains("pub fn redirect_to"))
        .unwrap();
    let guarded = lines[..at]
        .iter()
        .rev()
        .take_while(|l| {
            let t = l.trim();
            t.starts_with("#[") || t.starts_with("///") || t.is_empty()
        })
        .any(|l| l.contains("#[cfg(feature = \"test-redirect\")]"));
    assert!(
        guarded,
        "redirect_to 위에 #[cfg(feature = \"test-redirect\")]가 없다"
    );
}

#[test]
fn 시험_feature는_기본으로_꺼져_있다() {
    let manifest = read("Cargo.toml");
    assert!(
        manifest.contains("test-redirect = []"),
        "features에 test-redirect가 없다"
    );
    assert!(
        !manifest.contains("default = ["),
        "기본 feature가 있으면 안 된다"
    );
}

#[test]
fn 자기_시험은_feature를_켜서_자신을_쓴다() {
    let manifest = read("Cargo.toml");
    let dev = manifest.split("[dev-dependencies]").nth(1).unwrap();
    assert!(
        dev.contains("pi-notify") && dev.contains("test-redirect"),
        "dev-dependencies에서 pi-notify를 test-redirect로 켜야 한다"
    );
}
