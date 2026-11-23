//! server가 더는 크레이트마다의 화면 자산(`pi_office::web_assets()` · `pi_notify::phone_asset(s)`)과
//! `web/*.html` 직접 include를 쓰지 않고, 빌드 결과 `web/dist`를 넣는다(지시서 11차 3번). 소스 글을 읽어 본다.

use std::path::{Path, PathBuf};

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("src 폴더") {
        let path = entry.expect("폴더 항목").path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

// 검증: D1
#[test]
fn server는_크레이트_화면_자산과_옛_html_include를_쓰지_않고_dist를_넣는다() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    rust_files(&root.join("src"), &mut files);
    let build = root.join("build.rs");
    if build.is_file() {
        files.push(build);
    }
    let mut dist_used = false;
    for file in &files {
        let text = std::fs::read_to_string(file).expect("소스 글");
        for old in [
            "pi_office::web_assets",
            "pi_office::INDEX_PATH",
            "pi_notify::phone_asset",
            "\"../web/index.html\"",
            "\"../web/keys.html\"",
        ] {
            assert!(
                !text.contains(old),
                "{}가 아직 {old}를 쓴다",
                file.display()
            );
        }
        dist_used |= text.contains("web/dist");
    }
    assert!(
        dist_used,
        "server(또는 build.rs)가 web/dist를 실행 파일에 넣는다"
    );
}
