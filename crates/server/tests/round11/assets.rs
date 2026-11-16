//! 웹 자산 경로(T20 · T22): 사무실 화면은 `/office/`, 폰 화면은 `/phone/` 아래에서 세션 없이 열린다.

use price_inha::{OFFICE_ASSETS_PREFIX, PHONE_ASSETS_PREFIX};

use crate::common::{Bench, get_anonymous, kst, monday};

fn office_type(path: &str) -> &'static str {
    match path.rsplit('.').next().unwrap_or_default() {
        "html" => "text/html",
        "js" => "javascript",
        "json" => "application/json",
        "css" => "text/css",
        _ => "",
    }
}

// 검증: T20 D11 E3
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 사무실_화면_자산은_세션_없이_그_경로에서_그대로_열린다() {
    let bench = Bench::start(kst(monday(), 9, 5)).await;
    let assets = pi_office::web_assets();
    assert!(!assets.is_empty());
    for asset in assets {
        let reply = get_anonymous(
            bench.addr(),
            &format!("{OFFICE_ASSETS_PREFIX}{}", asset.path),
        )
        .await;
        assert_eq!(reply.status, 200, "{} 이 열려야 한다", asset.path);
        assert_eq!(reply.bytes, asset.body, "{} 내용이 그대로다", asset.path);
        let wanted = office_type(asset.path);
        assert!(
            reply.content_type.contains(wanted),
            "{} 의 Content-Type {} 에 {wanted}",
            asset.path,
            reply.content_type
        );
    }
    let index = pi_office::web_assets()
        .iter()
        .find(|a| a.path == pi_office::INDEX_PATH)
        .unwrap();
    let reply = get_anonymous(bench.addr(), OFFICE_ASSETS_PREFIX).await;
    assert_eq!(reply.status, 200);
    assert_eq!(reply.bytes, index.body, "/office/ 는 첫 쪽이다");
    assert!(reply.content_type.starts_with("text/html"));
    let missing = get_anonymous(bench.addr(), "/office/없는-파일.js").await;
    assert_eq!(missing.status, 404);
    bench.close().await;
}

// 검증: T22 D11 N5
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 폰_화면_자산은_묶음이_정한_종류로_열린다() {
    let bench = Bench::start(kst(monday(), 9, 5)).await;
    for asset in pi_notify::phone_assets() {
        let reply = get_anonymous(
            bench.addr(),
            &format!("{PHONE_ASSETS_PREFIX}{}", asset.path),
        )
        .await;
        assert_eq!(reply.status, 200, "{} 이 열려야 한다", asset.path);
        assert_eq!(reply.bytes, asset.body);
        assert_eq!(reply.content_type, asset.content_type, "{}", asset.path);
    }
    let index = pi_notify::phone_asset("index.html").unwrap();
    let reply = get_anonymous(bench.addr(), PHONE_ASSETS_PREFIX).await;
    assert_eq!(reply.status, 200);
    assert_eq!(reply.bytes, index.body, "/phone/ 는 폰 첫 쪽이다");
    let missing = get_anonymous(bench.addr(), "/phone/없음.png").await;
    assert_eq!(missing.status, 404);
    // 2차까지의 첫 화면은 그대로 있다.
    let root = get_anonymous(bench.addr(), "/").await;
    assert_eq!(root.status, 200);
    bench.close().await;
}
