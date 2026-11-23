//! 웹 자산 경로(T20 · T22): 사무실 화면은 `/office/`, 폰 화면은 `/phone/` 아래에서 세션 없이 열린다.
//!
//! 11차(Vite 이전): server는 이제 `pi_office::web_assets()` · `pi_notify::phone_assets()` 대신 빌드 결과
//! `web/dist/office/` · `web/dist/phone/`을 내준다. 같은 뜻(그 경로에서 세션 없이 그대로 열린다 · 첫 쪽 ·
//! 정한 종류 · 없는 파일은 404)을 dist 파일 목록으로 본다.

use price_inha::{OFFICE_ASSETS_PREFIX, PHONE_ASSETS_PREFIX};

use crate::common::{Bench, get_anonymous, kst, monday};
use crate::screen::{dist_dir, dist_files};

fn office_type(path: &str) -> &'static str {
    match path.rsplit('.').next().unwrap_or_default() {
        "html" => "text/html",
        "js" => "javascript",
        "json" => "application/json",
        "css" => "text/css",
        _ => "",
    }
}

/// 폰 묶음이 정했던 종류(옛 `pi_notify::phone_assets()`의 값 그대로).
fn phone_type(path: &str) -> Option<&'static str> {
    match path.rsplit('.').next().unwrap_or_default() {
        "html" => Some("text/html; charset=utf-8"),
        "js" => Some("text/javascript; charset=utf-8"),
        "webmanifest" => Some("application/manifest+json"),
        "png" => Some("image/png"),
        _ => None,
    }
}

// 검증: T20 D11 E3
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 사무실_화면_자산은_세션_없이_그_경로에서_그대로_열린다() {
    let bench = Bench::start(kst(monday(), 9, 5)).await;
    let assets = dist_files("office");
    assert!(!assets.is_empty());
    for (path, body) in &assets {
        let reply = get_anonymous(bench.addr(), &format!("{OFFICE_ASSETS_PREFIX}{path}")).await;
        assert_eq!(reply.status, 200, "{path} 이 열려야 한다");
        assert_eq!(&reply.bytes, body, "{path} 내용이 그대로다");
        let wanted = office_type(path);
        assert!(
            reply.content_type.contains(wanted),
            "{path} 의 Content-Type {} 에 {wanted}",
            reply.content_type
        );
    }
    let index =
        std::fs::read(dist_dir().join("office/index.html")).expect("dist/office/index.html");
    let reply = get_anonymous(bench.addr(), OFFICE_ASSETS_PREFIX).await;
    assert_eq!(reply.status, 200);
    assert_eq!(reply.bytes, index, "/office/ 는 첫 쪽이다");
    assert!(reply.content_type.starts_with("text/html"));
    let missing = get_anonymous(bench.addr(), "/office/없는-파일.js").await;
    assert_eq!(missing.status, 404);
    bench.close().await;
}

// 검증: T22 D11 N5
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 폰_화면_자산은_묶음이_정한_종류로_열린다() {
    let bench = Bench::start(kst(monday(), 9, 5)).await;
    let assets = dist_files("phone");
    // 옛 폰 경로는 그대로 남는다(앱 스크립트는 해시 붙은 묶음으로 옮겨 갈 수 있다).
    for kept in [
        "index.html",
        "sw.js",
        "manifest.webmanifest",
        "icon-192.png",
        "icon-512.png",
    ] {
        assert!(
            assets.iter().any(|(path, _)| path == kept),
            "dist/phone/{kept}"
        );
    }
    for (path, body) in &assets {
        let reply = get_anonymous(bench.addr(), &format!("{PHONE_ASSETS_PREFIX}{path}")).await;
        assert_eq!(reply.status, 200, "{path} 이 열려야 한다");
        assert_eq!(&reply.bytes, body);
        if let Some(wanted) = phone_type(path) {
            assert_eq!(reply.content_type, wanted, "{path}");
        }
    }
    let index = std::fs::read(dist_dir().join("phone/index.html")).expect("dist/phone/index.html");
    let reply = get_anonymous(bench.addr(), PHONE_ASSETS_PREFIX).await;
    assert_eq!(reply.status, 200);
    assert_eq!(reply.bytes, index, "/phone/ 는 폰 첫 쪽이다");
    let missing = get_anonymous(bench.addr(), "/phone/없음.png").await;
    assert_eq!(missing.status, 404);
    // 2차까지의 첫 화면은 그대로 있다.
    let root = get_anonymous(bench.addr(), "/").await;
    assert_eq!(root.status, 200);
    bench.close().await;
}
