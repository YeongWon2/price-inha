//! 빌드 결과(dist)의 모든 파일이 경로로 닿고, 묶음은 Vite 빌드가 만든 해시 이름이며,
//! 서비스 워커 · manifest는 해시 없는 고정 이름이고, three는 npm에서 와 라이선스 글이 함께 있다.
//! 12차 1번 · 4번: 폰 manifest 모양 · 아이콘(PNG 192 · 512) · three 없음을 dist에서 본다(다른 크레이트 폴더를 읽지 않는다).

use price_inha::KEYS_PAGE_PATH;

use crate::common::{Bench, get, session};
use crate::screen_built::built_page;
use crate::screen_dist::dist_dir;
use crate::screen_files::dist_files;

/// 파일 끝으로 정한 내용 형식(앞부분).
fn wanted_type(path: &str) -> &'static str {
    match path.rsplit('.').next().unwrap_or_default() {
        "html" => "text/html",
        "js" | "mjs" => "text/javascript",
        "css" => "text/css",
        "json" => "application/json",
        "webmanifest" => "application/manifest+json",
        "png" => "image/png",
        "svg" => "image/svg+xml",
        "txt" | "md" => "text/plain",
        _ => "",
    }
}

/// dist 안의 상대 경로 → 내주는 경로. 꼭대기 두 쪽만 이름이 다르다.
fn served_path(rel: &str) -> String {
    match rel {
        "index.html" => "/".to_owned(),
        "keys.html" => KEYS_PAGE_PATH.to_owned(),
        _ => format!("/{rel}"),
    }
}

// 검증: D1 D11
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn dist의_모든_파일이_그_경로로_그대로_닿는다() {
    let bench = Bench::start().await;
    let cookie = session(bench.addr()).await;
    let files = dist_files("");
    for page in [
        "index.html",
        "keys.html",
        "office/index.html",
        "phone/index.html",
    ] {
        assert!(
            files.iter().any(|(rel, _)| rel == page),
            "dist/{page}이 있다"
        );
    }
    for (rel, body) in &files {
        let path = served_path(rel);
        // 키 화면만 세션 경로다. 나머지(묶음 포함)는 세션 없이 열린다.
        let with = (rel == "keys.html").then_some(cookie.as_str());
        let reply = get(bench.addr(), &path, with).await;
        assert_eq!(reply.status, 200, "dist/{rel} 이 {path} 에서 열린다");
        assert_eq!(&reply.bytes, body, "{path} 는 dist/{rel} 그대로다");
        assert_eq!(reply.header("x-content-type-options"), "nosniff", "{path}");
        let wanted = wanted_type(rel);
        assert!(
            reply.header("content-type").starts_with(wanted),
            "{path} 의 내용 형식 {} 은 {wanted}",
            reply.header("content-type")
        );
    }
    bench.close().await;
}

/// `/assets/<이름>-<해시>.js` 꼴인가(해시는 Vite가 내용으로 만든 8자 이상).
fn is_hashed_bundle(url: &str) -> bool {
    let Some(file) = url.strip_prefix("/assets/") else {
        return false;
    };
    let Some(stem) = file.strip_suffix(".js") else {
        return false;
    };
    let Some((name, hash)) = stem.rsplit_once('-') else {
        return false;
    };
    !name.is_empty()
        && hash.len() >= 8
        && hash
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

// 검증: D1
#[test]
fn 화면_묶음은_vite가_만든_해시_이름이고_소스_경로를_부르지_않는다() {
    for page in [
        "index.html",
        "keys.html",
        "office/index.html",
        "phone/index.html",
    ] {
        let html = std::fs::read_to_string(dist_dir().join(page))
            .unwrap_or_else(|error| panic!("dist/{page}이 있어야 한다: {error}"));
        let entries: Vec<String> = crate::screen_refs::bundle_refs(&html);
        assert!(
            entries.iter().any(|url| is_hashed_bundle(url)),
            "dist/{page}는 해시 붙은 묶음(/assets/<이름>-<해시>.js)을 부른다: {entries:?}"
        );
        assert!(
            html.contains("<script type=\"module\" crossorigin src=\"/assets/"),
            "dist/{page}의 스크립트는 Vite가 넣은 모듈 태그다"
        );
        for url in &entries {
            assert!(
                dist_dir().join(url.trim_start_matches('/')).is_file(),
                "dist/{page}가 부르는 {url}이 dist에 있다"
            );
        }
        for dev in ["/src/", ".ts\"", "/@vite/", "vendor/three"] {
            assert!(
                !html.contains(dev),
                "dist/{page}가 개발 경로 {dev}를 부르지 않는다"
            );
        }
    }
}

/// PNG 시그니처(8바이트).
const PNG_SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];

/// PNG면 첫 덩어리 IHDR의 너비 · 높이를 돌려준다(아니면 `None`).
fn png_size(bytes: &[u8]) -> Option<(u32, u32)> {
    if bytes.len() < 24 || bytes[..8] != PNG_SIGNATURE || &bytes[12..16] != b"IHDR" {
        return None;
    }
    let width = u32::from_be_bytes(bytes[16..20].try_into().ok()?);
    let height = u32::from_be_bytes(bytes[20..24].try_into().ok()?);
    Some((width, height))
}

fn dist_phone(name: &str) -> Vec<u8> {
    std::fs::read(dist_dir().join("phone").join(name))
        .unwrap_or_else(|error| panic!("dist/phone/{name}이 있어야 한다: {error}"))
}

// 검증: D1 D11 V10
#[test]
fn 폰_아이콘은_192와_512_크기의_png다() {
    // 검사기 자체가 PNG가 아닌 것 · 짧은 것을 거르는지 먼저 본다.
    assert_eq!(png_size(b"GIF89a not a png at all......"), None);
    assert_eq!(png_size(&PNG_SIGNATURE), None);
    for (name, side) in [("icon-192.png", 192), ("icon-512.png", 512)] {
        let bytes = dist_phone(name);
        assert_eq!(
            png_size(&bytes),
            Some((side, side)),
            "dist/phone/{name}은 {side}x{side} PNG다"
        );
    }
}

// 검증: D1 D11 V10
#[test]
fn 폰_manifest는_설치할_수_있는_모양이고_아이콘이_묶음_안에_있다() {
    let manifest: serde_json::Value =
        serde_json::from_slice(&dist_phone("manifest.webmanifest")).expect("manifest는 JSON이다");
    for key in ["name", "id", "scope", "start_url"] {
        assert!(
            manifest[key].as_str().is_some_and(|v| !v.trim().is_empty()),
            "manifest의 {key}가 비어 있지 않다"
        );
    }
    assert_eq!(manifest["display"], "standalone");
    let icons = manifest["icons"].as_array().expect("manifest의 icons 목록");
    let sizes: Vec<&str> = icons.iter().filter_map(|i| i["sizes"].as_str()).collect();
    assert!(
        sizes.contains(&"192x192") && sizes.contains(&"512x512"),
        "192 · 512 아이콘: {sizes:?}"
    );
    for icon in icons {
        let src = icon["src"].as_str().expect("아이콘 src");
        assert!(!src.contains("://"), "아이콘은 바깥 주소가 아니다: {src}");
        let rel = src
            .trim_start_matches("/phone/")
            .trim_start_matches("./")
            .trim_start_matches('/');
        assert!(
            dist_dir().join("phone").join(rel).is_file(),
            "아이콘 {src}가 폰 묶음(dist/phone) 안에 있다"
        );
    }
}

/// `text` 안에서 `prefix` 다음에 이어지는 파일 이름(영문 · 숫자 · `.` `-` `_`)을 모두 뽑는다.
fn names_after(text: &str, prefix: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(at) = rest.find(prefix) {
        rest = &rest[at + prefix.len()..];
        let end = rest
            .find(|c: char| !(c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_')))
            .unwrap_or(rest.len());
        let name = &rest[..end];
        if name.ends_with(".js") || name.ends_with(".css") {
            out.push(name.to_owned());
        }
    }
    out
}

/// 폰 html이 부르는 `assets/` 안의 파일과, 그 js가 `./이름.js`로 끌어오는 조각을 모두(되풀이해서) 모은다.
fn phone_loaded_assets() -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    let mut todo = names_after(&built_page("phone/index.html"), "assets/");
    while let Some(name) = todo.pop() {
        if found.contains(&name) {
            continue;
        }
        if name.ends_with(".js") {
            let body = std::fs::read_to_string(dist_dir().join("assets").join(&name))
                .unwrap_or_else(|error| panic!("dist/assets/{name}이 있어야 한다: {error}"));
            todo.extend(names_after(&body, "./"));
        }
        found.push(name);
    }
    found
}

// 검증: V10 V12
#[test]
fn 폰_묶음에는_three가_없고_바깥_주소도_부르지_않는다() {
    let mut texts = vec![("phone 묶음".to_owned(), built_page("phone/index.html"))];
    for name in ["sw.js", "manifest.webmanifest"] {
        let body = String::from_utf8(dist_phone(name)).expect("글 파일");
        texts.push((format!("dist/phone/{name}"), body));
    }
    // 폰이 불러오는 묶음(assets/phone-*.js · css와 그것이 끌어오는 조각)도 본다(15차 5번 가).
    let loaded = phone_loaded_assets();
    assert!(
        loaded
            .iter()
            .any(|n| n.starts_with("phone-") && n.ends_with(".js")),
        "폰 html이 phone-*.js를 부른다: {loaded:?}"
    );
    assert!(
        loaded
            .iter()
            .any(|n| n.starts_with("phone-") && n.ends_with(".css")),
        "폰 html이 phone-*.css를 부른다: {loaded:?}"
    );
    for name in loaded {
        let body = std::fs::read_to_string(dist_dir().join("assets").join(&name))
            .unwrap_or_else(|error| panic!("dist/assets/{name}이 있어야 한다: {error}"));
        texts.push((format!("dist/assets/{name}"), body));
    }
    for (what, body) in texts {
        let lower = body.to_lowercase();
        assert!(!lower.contains("three"), "{what}에 three가 있다(결정 26)");
        assert!(
            !lower.contains("http://") && !lower.contains("https://"),
            "{what}가 바깥 주소를 부른다"
        );
    }
}

// 검증: D1 D11
#[test]
fn 서비스_워커와_manifest와_아이콘은_해시_없는_고정_이름이고_범위가_그대로다() {
    for name in ["manifest.webmanifest", "icon-192.png", "icon-512.png"] {
        assert!(
            dist_dir().join("phone").join(name).is_file(),
            "dist/phone/{name}(해시 없음)"
        );
    }
    let manifest: serde_json::Value =
        serde_json::from_slice(&dist_phone("manifest.webmanifest")).expect("manifest는 JSON이다");
    assert_eq!(
        manifest["scope"], "./",
        "manifest 범위는 그대로(./ = /phone/)"
    );
    assert_eq!(manifest["start_url"], "./", "시작 주소는 그대로");
    assert!(
        dist_dir().join("phone/sw.js").is_file(),
        "dist/phone/sw.js(해시 없음)"
    );
    let phone = built_page("phone/index.html");
    assert!(
        phone.contains("href=\"manifest.webmanifest\"")
            || phone.contains("href=\"/phone/manifest.webmanifest\""),
        "폰 쪽이 manifest를 그 이름으로 부른다"
    );
    assert!(
        phone.contains("\"sw.js\"") && phone.contains("scope: \"./\""),
        "폰 묶음이 sw.js를 지금과 같은 범위(./ = /phone/)로 등록한다"
    );
    let worker = std::fs::read_to_string(dist_dir().join("phone/sw.js")).expect("sw.js");
    for event in ["\"push\"", "\"notificationclick\""] {
        assert!(
            worker.contains(event) || worker.contains(&event.replace('"', "'")),
            "서비스 워커가 {event}를 받는다"
        );
    }
}

// 검증: D1
#[test]
fn three는_npm에서_묶이고_라이선스_글이_dist에_있다() {
    let office = built_page("office/index.html");
    assert!(
        office.contains("WebGLRenderer"),
        "사무실 묶음(동적 import 포함)에 three가 들어 있다"
    );
    let files = dist_files("");
    assert!(
        !files
            .iter()
            .any(|(rel, _)| rel.ends_with("three.module.min.js")),
        "옛 vendor 파일을 내주지 않는다(three는 npm에서 묶는다)"
    );
    let license = files.iter().any(|(_, body)| {
        let text = String::from_utf8_lossy(body);
        text.contains("three.js authors") && text.contains("Permission is hereby granted")
    });
    assert!(license, "three의 MIT 라이선스 글이 dist에 함께 있다");
}
