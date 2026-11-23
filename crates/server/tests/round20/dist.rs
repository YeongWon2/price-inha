//! 빌드 결과(dist)의 모든 파일이 경로로 닿고, 묶음은 Vite 빌드가 만든 해시 이름이며,
//! 서비스 워커 · manifest는 해시 없는 고정 이름이고, three는 npm에서 와 라이선스 글이 함께 있다.

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

// 검증: D1 D11
#[test]
fn 서비스_워커와_manifest와_아이콘은_해시_없는_고정_이름이고_범위가_그대로다() {
    let original = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../notify/web/phone");
    for name in ["manifest.webmanifest", "icon-192.png", "icon-512.png"] {
        let built = std::fs::read(dist_dir().join("phone").join(name))
            .unwrap_or_else(|error| panic!("dist/phone/{name}이 있어야 한다: {error}"));
        let before = std::fs::read(original.join(name)).expect("옛 폰 파일");
        assert_eq!(
            built, before,
            "dist/phone/{name}은 지금 것과 같다(범위 · 아이콘 그대로)"
        );
    }
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
