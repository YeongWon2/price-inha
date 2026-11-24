//! 4번: 관리 화면 넷이 설정 · 사무실 화면과 같은 관문을 받는다.
//! 지금 설정(`/`) · 사무실(`/office/`)은 그 컴퓨터에서 세션 없이 열리고(round20 `pages` · `dist`), 다른 기기는
//! 원격 세션이 있어야 한다(`pi_gate` 2번 — 공개 경로도 같다). 관리 화면도 같은 규칙을 기대한다.

use price_inha::{
    ACCOUNT_PAGE_PATH, ADMIN_PAGE_PATHS, HISTORY_PAGE_PATH, RULES_PAGE_PATH, TEMPLATES_PAGE_PATH,
};

use crate::common::{Bench, Reply, first_refs, get};
use crate::screen_dist::dist_dir;

/// 관리 화면 주소와 dist 안의 HTML.
const ADMIN: [(&str, &str); 4] = [
    (ACCOUNT_PAGE_PATH, "account/index.html"),
    (TEMPLATES_PAGE_PATH, "templates/index.html"),
    (RULES_PAGE_PATH, "rules/index.html"),
    (HISTORY_PAGE_PATH, "history/index.html"),
];

/// 같은 관문의 기준(지금 화면): 설정 · 사무실.
const NOW: [(&str, &str); 2] = [("/", "index.html"), ("/office/", "office/index.html")];

fn built(rel: &str) -> Vec<u8> {
    std::fs::read(dist_dir().join(rel)).unwrap_or_else(|error| {
        panic!("dist/{rel}이 있어야 한다(npm --prefix crates/server/web run build): {error}")
    })
}

fn assert_page(reply: &Reply, path: &str, rel: &str) {
    assert_eq!(reply.status, 200, "{path} 가 열린다");
    assert!(
        reply.header("content-type").starts_with("text/html"),
        "{path} 는 HTML이다: {}",
        reply.header("content-type")
    );
    assert!(
        reply
            .header("content-security-policy")
            .contains("default-src 'none'"),
        "{path} 에 화면 정책 머리가 붙는다"
    );
    assert_eq!(reply.header("x-content-type-options"), "nosniff", "{path}");
    assert_eq!(reply.bytes, built(rel), "{path} 는 dist/{rel} 그대로다");
}

// 검증: V75 F12 D11 C15
#[test]
fn 관리_화면_주소는_넷이고_서로_다르며_슬래시로_끝난다() {
    assert_eq!(
        ADMIN_PAGE_PATHS,
        ["/account/", "/templates/", "/rules/", "/history/"],
        "관리 화면 넷의 주소(계좌 · 템플릿 · 규칙 · 히스토리 차례)"
    );
}

// 검증: V75 F12 D11 C15 D2
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 그_컴퓨터에서는_관리_화면이_설정_사무실처럼_세션_없이_열린다() {
    let bench = Bench::start().await;
    for (path, rel) in NOW.iter().chain(ADMIN.iter()) {
        let reply = get(bench.addr(), path, None).await;
        assert_page(&reply, path, rel);
    }
    bench.close().await;
}

// 검증: V75 F12 D11 C15 M6
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 다른_기기는_원격_세션이_없으면_관리_화면이_401이고_있으면_열린다() {
    let bench = Bench::start().await;
    bench.go_remote();
    for (path, _) in NOW.iter().chain(ADMIN.iter()) {
        let reply = get(bench.addr(), path, None).await;
        assert_eq!(
            reply.status, 401,
            "{path} 는 다른 기기에서 세션 없이 열리지 않는다"
        );
        let body = String::from_utf8_lossy(&reply.bytes).to_lowercase();
        assert!(
            !body.contains("<!doctype html"),
            "{path} 401 응답에 화면이 실리지 않는다"
        );
    }
    let cookie = bench.remote_login().await;
    for (path, rel) in NOW.iter().chain(ADMIN.iter()) {
        let reply = get(bench.addr(), path, Some(&cookie)).await;
        assert_page(&reply, path, rel);
    }
    bench.close().await;
}

// 검증: V75 V77 D11 D1
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 관리_화면이_부르는_묶음과_모양이_같은_출처에서_열린다() {
    let bench = Bench::start().await;
    for (path, rel) in ADMIN {
        let html = String::from_utf8(built(rel)).expect("UTF-8 HTML");
        let refs = first_refs(&html);
        assert!(
            refs.iter().any(|url| url.ends_with(".js")),
            "{path} 가 스크립트 묶음을 부른다: {refs:?}"
        );
        assert!(
            refs.iter().any(|url| url.ends_with(".css")),
            "{path} 가 모양(토큰 포함) 묶음을 부른다: {refs:?}"
        );
        for url in refs {
            let reply = get(bench.addr(), &url, None).await;
            assert_eq!(reply.status, 200, "{path} 가 부르는 {url} 가 열린다");
            let wanted = if url.ends_with(".css") {
                "text/css"
            } else {
                "text/javascript"
            };
            assert!(
                reply.header("content-type").starts_with(wanted),
                "{url} 의 내용 형식: {}",
                reply.header("content-type")
            );
            assert_eq!(
                reply.bytes,
                built(url.trim_start_matches('/')),
                "{url} 는 dist 그대로다"
            );
        }
    }
    bench.close().await;
}
