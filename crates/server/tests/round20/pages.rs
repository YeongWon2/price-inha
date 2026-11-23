//! 각 페이지 경로가 200 · 맞는 내용 형식 · CSP · 캐시를 주고, 내주는 HTML이 빌드 결과(dist) 그대로인가.

use price_inha::KEYS_PAGE_PATH;

use crate::common::{Bench, Reply, get, session};
use crate::screen_dist::dist_dir;

/// 경로 · 세션이 필요한가 · 내용 형식 · 캐시 · HTML 쪽인가(dist 안의 파일 이름).
struct Page {
    path: &'static str,
    session: bool,
    content_type: &'static str,
    cache: &'static str,
    html: Option<&'static str>,
}

const PAGES: [Page; 6] = [
    Page {
        path: "/",
        session: false,
        content_type: "text/html; charset=utf-8",
        cache: "no-store",
        html: Some("index.html"),
    },
    Page {
        path: KEYS_PAGE_PATH,
        session: true,
        content_type: "text/html; charset=utf-8",
        cache: "no-store",
        html: Some("keys.html"),
    },
    Page {
        path: "/office/",
        session: false,
        content_type: "text/html; charset=utf-8",
        cache: "no-cache",
        html: Some("office/index.html"),
    },
    Page {
        path: "/phone/",
        session: false,
        content_type: "text/html; charset=utf-8",
        cache: "no-cache",
        html: Some("phone/index.html"),
    },
    Page {
        path: "/phone/sw.js",
        session: false,
        content_type: "text/javascript; charset=utf-8",
        cache: "no-cache",
        html: None,
    },
    Page {
        path: "/phone/manifest.webmanifest",
        session: false,
        content_type: "application/manifest+json",
        cache: "no-cache",
        html: None,
    },
];

async fn fetch(bench: &Bench, page: &Page) -> Reply {
    let cookie = if page.session {
        Some(session(bench.addr()).await)
    } else {
        None
    };
    get(bench.addr(), page.path, cookie.as_deref()).await
}

/// CSP의 지시 하나의 값(없으면 `default-src`로 물러선다).
fn directive(policy: &str, name: &str) -> String {
    let find = |wanted: &str| {
        policy
            .split(';')
            .map(str::trim)
            .find(|part| part.split_whitespace().next() == Some(wanted))
            .map(|part| part[wanted.len()..].trim().to_owned())
    };
    find(name)
        .or_else(|| find("default-src"))
        .unwrap_or_default()
}

fn allows_self(policy: &str, name: &str) -> bool {
    directive(policy, name)
        .split_whitespace()
        .any(|s| s == "'self'")
}

/// 바깥 출처를 여는 원천이 없다(`*` · `http:` · `https:` · 다른 호스트). 웹소켓은 `ws://<그 Host>`만,
/// 망을 타지 않는 `data:` · `blob:`은 된다.
fn assert_no_outside_source(policy: &str, addr: &str, what: &str) {
    for part in policy.split(';') {
        for source in part.split_whitespace().skip(1) {
            let ok = source.starts_with('\'')
                || source == format!("ws://{addr}")
                || matches!(source, "data:" | "blob:");
            assert!(ok, "{what}: CSP가 바깥 원천을 연다: {source} ({policy})");
        }
    }
}

// 검증: D1 D11 V46
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 각_페이지_경로가_200과_맞는_내용_형식과_정책_머리를_준다() {
    let bench = Bench::start().await;
    let addr = bench.addr().to_string();
    for page in &PAGES {
        let reply = fetch(&bench, page).await;
        let what = page.path;
        assert_eq!(reply.status, 200, "{what} 이 열린다");
        assert_eq!(reply.header("content-type"), page.content_type, "{what}");
        assert_eq!(reply.header("x-content-type-options"), "nosniff", "{what}");
        assert_eq!(
            reply.header("cache-control"),
            page.cache,
            "{what}: 캐시는 지금처럼"
        );
        if page.path.ends_with(".webmanifest") {
            continue; // manifest는 문서가 아니라 정책 머리를 보지 않는다.
        }
        let policy = reply.header("content-security-policy");
        assert!(!policy.is_empty(), "{what}: CSP 머리가 있다");
        assert!(policy.contains("default-src 'none'"), "{what}: {policy}");
        assert_no_outside_source(&policy, &addr, what);
        if page.html.is_some() {
            assert!(
                policy.contains("frame-ancestors 'none'"),
                "{what}: {policy}"
            );
            assert!(policy.contains("base-uri 'none'"), "{what}: {policy}");
            assert!(
                allows_self(&policy, "connect-src"),
                "{what}: 같은 출처로 잇는다: {policy}"
            );
            assert_eq!(reply.header("x-frame-options"), "DENY", "{what}");
        }
    }
    bench.close().await;
}

/// HTML이 부르는 (태그, 속성 값) 목록: `<script src>` · `<link rel=… href>`.
fn references(html: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for (tag, attr) in [("<script", "src=\""), ("<link", "href=\"")] {
        for piece in html.split(tag).skip(1) {
            let head = piece.split('>').next().unwrap_or_default();
            let Some(value) = head
                .split(attr)
                .nth(1)
                .and_then(|rest| rest.split('"').next())
            else {
                continue;
            };
            let kind = if tag == "<script" {
                "script".to_owned()
            } else {
                head.split("rel=\"")
                    .nth(1)
                    .and_then(|rest| rest.split('"').next())
                    .unwrap_or_default()
                    .to_owned()
            };
            out.push((kind, value.to_owned()));
        }
    }
    out
}

// 검증: D1 D11
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 페이지가_부르는_파일은_같은_출처이고_정책이_막지_않는다() {
    let bench = Bench::start().await;
    for page in PAGES.iter().filter(|page| page.html.is_some()) {
        let reply = fetch(&bench, page).await;
        let policy = reply.header("content-security-policy");
        let html = reply.text();
        let refs = references(&html);
        assert!(
            refs.iter().any(|(kind, _)| kind == "script"),
            "{}: 스크립트를 부른다",
            page.path
        );
        for (kind, url) in &refs {
            assert!(
                !url.contains("://") && !url.starts_with("//"),
                "{}: 같은 출처만 부른다: {url}",
                page.path
            );
            let needed = match kind.as_str() {
                "script" | "modulepreload" => "script-src",
                "stylesheet" => "style-src",
                "manifest" => "manifest-src",
                "icon" | "apple-touch-icon" => "img-src",
                _ => continue,
            };
            assert!(
                allows_self(&policy, needed),
                "{}: {kind} {url}을 {needed}가 막는다: {policy}",
                page.path
            );
        }
    }
    bench.close().await;
}

// 검증: D1 D11
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 내주는_화면은_빌드한_dist_그대로다() {
    let bench = Bench::start().await;
    for page in PAGES.iter() {
        let reply = fetch(&bench, page).await;
        let file = page
            .html
            .map(str::to_owned)
            .unwrap_or_else(|| page.path.trim_start_matches('/').to_owned());
        let built = std::fs::read(dist_dir().join(&file))
            .unwrap_or_else(|error| panic!("dist/{file}이 있어야 한다: {error}"));
        assert_eq!(reply.bytes, built, "{} 는 dist/{file} 그대로다", page.path);
    }
    bench.close().await;
}
