//! 화면 글 읽기(server 11차, Vite 이전)의 공용 조각: HTML · 묶음이 부르는 같은 출처 스크립트 경로.
//! 빌드 뒤 화면의 스크립트는 HTML 밖의 묶음 파일(`/assets/…js`)로 나간다. 옛 시험이 HTML 글자를 직접
//! 보던 뜻을 지키려고 쪽 HTML 뒤에 그 묶음을 이어 붙일 때 쓴다(`docs/dev/modules/server.md` "11차" 5번).
//! 테스트 바이너리마다 쓰는 함수만 들어가게 파일을 나눴다(`screen_*.rs`).

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
pub fn relative_imports(url: &str, body: &str) -> Vec<String> {
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
