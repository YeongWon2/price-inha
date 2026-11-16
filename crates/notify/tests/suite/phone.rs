//! 폰 화면 묶음(PWA): 승인 카드 · 알림 목록 · 하루 정리 · 3D 사무실 단추, manifest, sw.js,
//! 그리고 3D 라이브러리를 불러오지 않음(결정 26번).

use pi_notify::{Asset, phone_asset, phone_assets};

const REQUIRED: [&str; 6] = [
    "index.html",
    "app.js",
    "sw.js",
    "manifest.webmanifest",
    "icon-192.png",
    "icon-512.png",
];

fn text(path: &str) -> String {
    let asset = phone_asset(path).unwrap_or_else(|| panic!("{path}가 없다"));
    String::from_utf8(asset.body.to_vec()).unwrap()
}

fn is_text(asset: &Asset) -> bool {
    !asset.content_type.starts_with("image/")
}

fn listens(js: &str, event: &str) -> bool {
    js.contains(&format!("'{event}'")) || js.contains(&format!("\"{event}\""))
}

// 검증: V10 V12
#[test]
fn 폰_묶음에_필요한_파일이_모두_있고_없는_경로는_없다() {
    for path in REQUIRED {
        let asset = phone_asset(path).unwrap_or_else(|| panic!("{path}가 없다"));
        assert_eq!(asset.path, path);
        assert!(!asset.body.is_empty(), "{path}가 비었다");
    }
    assert!(phone_asset("three.module.js").is_none());
    assert!(phone_asset("").is_none());
    let listed: Vec<&str> = phone_assets().iter().map(|a| a.path).collect();
    for path in REQUIRED {
        assert!(listed.contains(&path));
    }
    let mut sorted = listed.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(sorted.len(), listed.len(), "경로가 겹친다");
}

// 검증: V10 V12
#[test]
fn 내용_종류가_파일에_맞다() {
    let kind = |path| phone_asset(path).unwrap().content_type;
    assert!(kind("index.html").starts_with("text/html"));
    assert!(kind("app.js").contains("javascript"));
    assert!(kind("sw.js").contains("javascript"));
    assert!(kind("manifest.webmanifest").starts_with("application/manifest+json"));
    assert_eq!(kind("icon-192.png"), "image/png");
    assert_eq!(kind("icon-512.png"), "image/png");
}

// 검증: V10 V12
#[test]
fn 폰_묶음_어디에도_three_글자가_없고_바깥_주소도_부르지_않는다() {
    for asset in phone_assets().iter().filter(|a| is_text(a)) {
        let body = String::from_utf8_lossy(asset.body).to_lowercase();
        assert!(!body.contains("three"), "{}에 three가 있다", asset.path);
        assert!(
            !body.contains("http://") && !body.contains("https://"),
            "{}가 바깥 주소를 부른다",
            asset.path
        );
    }
}

// 검증: V10
#[test]
fn 승인_카드에_승인_거절_단추와_6자리_코드_칸과_승인_번호_지문_자리가_있다() {
    let html = text("index.html");
    let whole = format!("{html}\n{}", text("app.js"));
    assert!(html.contains("data-card=\"approval\""));
    for needle in [
        "data-action=\"approve\"",
        "data-action=\"reject\"",
        "data-field=\"code\"",
        "inputmode=\"numeric\"",
        "maxlength=\"6\"",
        "data-approval-id",
        "data-fingerprint",
    ] {
        assert!(whole.contains(needle), "{needle}가 없다");
    }
}

// 검증: V10
#[test]
fn 알림_목록과_3d_사무실_단추가_있다() {
    let html = text("index.html");
    assert!(html.contains("id=\"notice-list\""));
    assert!(html.contains("data-action=\"open-office\""));
    assert!(html.contains("3D 사무실"));
    assert!(html.contains("name=\"viewport\""));
    assert!(html.contains("manifest.webmanifest"));
}

// 검증: V12
#[test]
fn 하루_정리는_한_구역에_한_일과_손익과_복기가_함께_있다() {
    let html = text("index.html");
    let start = html
        .find("id=\"daily-summary\"")
        .expect("하루 정리 구역이 없다");
    let section_end = html[start..]
        .find("</section>")
        .expect("구역이 닫히지 않았다")
        + start;
    let section = &html[start..section_end];
    for part in ["done", "pnl", "review"] {
        assert!(
            section.contains(&format!("data-part=\"{part}\"")),
            "하루 정리 구역에 {part}가 없다"
        );
    }
}

// 검증: V10 V11
#[test]
fn 앱_스크립트는_서비스_워커를_등록하고_푸시를_구독한다() {
    let js = text("app.js");
    for needle in [
        "serviceWorker",
        "pushManager",
        "subscribe",
        "applicationServerKey",
        "userVisibleOnly",
    ] {
        assert!(js.contains(needle), "{needle}가 없다");
    }
}

// 검증: V10 V11
#[test]
fn 서비스_워커는_푸시를_받아_알림을_띄우고_눌러서_폰_화면을_연다() {
    let js = text("sw.js");
    assert!(listens(&js, "push"));
    assert!(listens(&js, "notificationclick"));
    assert!(js.contains("showNotification"));
    assert!(js.contains("json"), "푸시 본문(JSON)을 읽어야 한다");
    assert!(js.contains("title") && js.contains("body"));
}

// 검증: V10
#[test]
fn 매니페스트는_설치할_수_있는_모양이고_아이콘이_묶음_안에_있다() {
    let manifest: serde_json::Value = serde_json::from_str(&text("manifest.webmanifest")).unwrap();
    for key in ["name", "id", "scope", "start_url"] {
        assert!(
            manifest[key].as_str().is_some_and(|v| !v.is_empty()),
            "{key}"
        );
    }
    assert_eq!(manifest["display"], "standalone");
    let icons = manifest["icons"].as_array().unwrap();
    let sizes: Vec<&str> = icons.iter().filter_map(|i| i["sizes"].as_str()).collect();
    assert!(sizes.contains(&"192x192") && sizes.contains(&"512x512"));
    for icon in icons {
        let src = icon["src"]
            .as_str()
            .unwrap()
            .trim_start_matches("./")
            .trim_start_matches('/');
        assert!(phone_asset(src).is_some(), "{src}가 묶음에 없다");
    }
}
