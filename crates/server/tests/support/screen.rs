//! 화면 글 읽기(server 11차, Vite 이전): 빌드 뒤 화면의 스크립트는 HTML 밖의 묶음 파일(`/assets/…js`)로 나간다.
//! 옛 시험이 HTML 글자를 직접 보던 뜻을 그대로 지키려고, 쪽 HTML 뒤에 그 쪽이 부르는 묶음(스크립트 ·
//! modulepreload · 묶음 안의 상대 import)을 이어 붙인 글을 만든다. 쪽은 늘 빌드 결과 `web/dist`에서 읽는다
//! (server가 내주는 것이 그것이다). 지시서 `docs/dev/modules/server.md` "11차 Vite 이전" 5번.

// 테스트 바이너리마다 이 파일의 일부만 쓴다.
#![allow(dead_code)]

use std::collections::BTreeSet;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

/// 빌드 결과 폴더(`crates/server/web/dist`).
pub fn dist_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("web/dist")
}

/// 빌드한 쪽(`dist/<page>`, 예: `index.html`) 글 뒤에 그 쪽이 부르는 묶음 글을 이어 붙인다.
pub fn built_page(page: &str) -> String {
    let path = dist_dir().join(page);
    let html = std::fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!(
            "빌드한 화면 {}이 있어야 한다(npm --prefix crates/server/web run build): {error}",
            path.display()
        )
    });
    let mut text = html.clone();
    let mut seen = BTreeSet::new();
    let mut queue = bundle_refs(&html);
    while let Some(url) = queue.pop() {
        if !seen.insert(url.clone()) {
            continue;
        }
        let file = dist_dir().join(url.trim_start_matches('/'));
        let body = std::fs::read_to_string(&file)
            .unwrap_or_else(|error| panic!("화면이 부르는 {url}이 dist에 있어야 한다: {error}"));
        queue.extend(relative_imports(&url, &body));
        text.push('\n');
        text.push_str(&body);
    }
    text
}

/// 서버가 내준 쪽 글(`html`) 뒤에, 그 쪽이 부르는 묶음을 같은 서버에서 (세션 없이) 받아 이어 붙인다.
pub async fn served_page(addr: SocketAddr, html: &str) -> String {
    let client = reqwest::Client::new();
    let mut text = html.to_owned();
    let mut seen = BTreeSet::new();
    let mut queue = bundle_refs(html);
    while let Some(url) = queue.pop() {
        if !seen.insert(url.clone()) {
            continue;
        }
        let reply = tokio::time::timeout(
            Duration::from_secs(15),
            client.get(format!("http://{addr}{url}")).send(),
        )
        .await
        .expect("묶음 응답이 시간 안에 와야 한다")
        .expect("묶음 요청이 닿아야 한다");
        assert_eq!(
            reply.status().as_u16(),
            200,
            "화면이 부르는 {url}이 열려야 한다"
        );
        let body = reply.text().await.expect("묶음 글");
        queue.extend(relative_imports(&url, &body));
        text.push('\n');
        text.push_str(&body);
    }
    text
}

/// HTML이 부르는 같은 출처 스크립트: `<script … src="/…">`와 `<link rel="modulepreload" href="/…">`.
pub fn bundle_refs(html: &str) -> Vec<String> {
    let mut out = Vec::new();
    for (tag, attr) in [("<script", "src=\""), ("<link", "href=\"")] {
        for piece in html.split(tag).skip(1) {
            let head = piece.split('>').next().unwrap_or_default();
            if tag == "<link" && !head.contains("modulepreload") {
                continue;
            }
            if let Some(rest) = head.split(attr).nth(1) {
                let url = rest.split('"').next().unwrap_or_default();
                if url.starts_with('/') && !url.starts_with("//") {
                    out.push(url.to_owned());
                }
            }
        }
    }
    out
}

/// 묶음 안의 상대 import(`from "./x.js"`, `import "./x.js"`, `import("./x.js")`)를 절대 경로로.
fn relative_imports(url: &str, body: &str) -> Vec<String> {
    let dir = &url[..url.rfind('/').map_or(0, |at| at + 1)];
    let mut out = Vec::new();
    for marker in ["from \"./", "import \"./", "import(\"./"] {
        for piece in body.split(marker).skip(1) {
            let name = piece.split('"').next().unwrap_or_default();
            if !name.is_empty() {
                out.push(format!("{dir}{name}"));
            }
        }
    }
    out
}

/// `dist/<sub>` 아래 모든 파일(하위 폴더 포함): (`sub` 기준 상대 경로 `/` 구분, 내용).
pub fn dist_files(sub: &str) -> Vec<(String, Vec<u8>)> {
    let root = dist_dir().join(sub);
    let mut out = Vec::new();
    let mut stack = vec![root.clone()];
    while let Some(dir) = stack.pop() {
        let entries = std::fs::read_dir(&dir)
            .unwrap_or_else(|error| panic!("{}이 있어야 한다: {error}", dir.display()));
        for entry in entries {
            let path = entry.expect("폴더 항목").path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            let rel = path
                .strip_prefix(&root)
                .expect("dist 아래")
                .components()
                .map(|part| part.as_os_str().to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join("/");
            out.push((rel, std::fs::read(&path).expect("dist 파일")));
        }
    }
    out.sort();
    out
}
