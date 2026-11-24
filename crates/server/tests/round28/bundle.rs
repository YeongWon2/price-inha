//! 6번(N21 · V77): 관리 화면 묶음에는 three가 실리지 않고, 관리 화면 넷의 첫 화면 JS + CSS는 gzip 앞 60KB 안이다.
//! round23 `bundle`과 같은 방법으로 커밋한 빌드 결과(dist)의 글을 읽는다.

use crate::common::first_refs;
use crate::screen_built::built_page;
use crate::screen_dist::dist_dir;

/// 관리 화면 넷의 dist 안 HTML.
const ADMIN_PAGES: [&str; 4] = [
    "account/index.html",
    "templates/index.html",
    "rules/index.html",
    "history/index.html",
];

/// 첫 화면 JS + CSS 상한(gzip 앞, 바이트). 60KB를 보수적으로 60,000바이트로 본다.
const FIRST_SCREEN_LIMIT: usize = 60_000;

/// three 묶음에만 있는 글자(사무실 3D 묶음에 있는지로 뜻을 확인한다).
const THREE_MARKS: [&str; 3] = ["WebGLRenderer", "PerspectiveCamera", "BufferGeometry"];

fn read(rel: &str) -> String {
    std::fs::read_to_string(dist_dir().join(rel)).unwrap_or_else(|error| {
        panic!("dist/{rel}이 있어야 한다(npm --prefix crates/server/web run build): {error}")
    })
}

/// 묶음 안의 정적 상대 import(`from "./x.js"`, `import "./x.js"`) — 늦게 부르는 `import("./x.js")`는 뺀다.
fn static_imports(url: &str, body: &str) -> Vec<String> {
    let dir = &url[..url.rfind('/').map_or(0, |at| at + 1)];
    let mut out = Vec::new();
    for marker in ["from \"./", "import \"./"] {
        for piece in body.split(marker).skip(1) {
            let name = piece.split('"').next().unwrap_or_default();
            if !name.is_empty() {
                out.push(format!("{dir}{name}"));
            }
        }
    }
    out
}

/// 첫 화면이 받는 JS · CSS 파일(같은 파일은 한 번)과 그 크기.
fn first_screen(page: &str) -> Vec<(String, usize)> {
    let mut queue = first_refs(&read(page));
    let mut seen: Vec<(String, usize)> = Vec::new();
    while let Some(url) = queue.pop() {
        if seen.iter().any(|(done, _)| *done == url) {
            continue;
        }
        let body = read(url.trim_start_matches('/'));
        if url.ends_with(".js") {
            queue.extend(static_imports(&url, &body));
        }
        seen.push((url, body.len()));
    }
    seen
}

// 검증: V77 N21 D1
#[test]
fn 관리_화면_묶음에는_three가_실리지_않는다() {
    // 이 검사가 뜻을 가지려면 사무실 3D 묶음에는 그 글자가 있어야 한다.
    let office = built_page("office/index.html");
    for mark in THREE_MARKS {
        assert!(office.contains(mark), "사무실 3D 묶음에 {mark}가 있다");
    }
    for page in ADMIN_PAGES {
        // 늦게 부르는 묶음까지 따라간 글(built_page는 import("./…")도 따라간다).
        let text = built_page(page);
        for mark in THREE_MARKS {
            assert!(!text.contains(mark), "{page} 묶음에 three({mark})가 있다");
        }
        assert!(
            !text.contains("scene3d"),
            "{page} 묶음이 사무실 3D 묶음(scene3d)을 부른다"
        );
        assert!(
            !text.contains("three-LICENSE"),
            "{page} 묶음이 three 라이선스 글을 부른다"
        );
    }
}

// 검증: V77 N21
#[test]
fn 관리_화면_넷의_첫_화면_js와_css는_60kb_안이다() {
    for page in ADMIN_PAGES {
        let files = first_screen(page);
        assert!(
            files.iter().any(|(url, _)| url.ends_with(".js")),
            "{page} 첫 화면에 스크립트 묶음이 있다: {files:?}"
        );
        assert!(
            files.iter().any(|(url, _)| url.ends_with(".css")),
            "{page} 첫 화면에 모양 묶음이 있다: {files:?}"
        );
        let total: usize = files.iter().map(|(_, size)| size).sum();
        assert!(
            total <= FIRST_SCREEN_LIMIT,
            "{page} 첫 화면 JS + CSS {total}바이트가 상한 {FIRST_SCREEN_LIMIT}을 넘는다: {files:?}"
        );
    }
}
