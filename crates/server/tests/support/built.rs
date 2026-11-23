//! 빌드한 쪽 글(server 11차): `dist/<쪽>` 뒤에 그 쪽이 부르는 묶음 글을 이어 붙인다.
//! 쓰는 바이너리는 `screen_refs`(refs.rs) · `screen_dist`(dist.rs)도 넣는다.

use std::collections::BTreeSet;

use crate::screen_dist::dist_dir;
use crate::screen_refs::{bundle_refs, relative_imports};

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
