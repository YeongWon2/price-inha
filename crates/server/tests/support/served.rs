//! 서버가 내준 쪽 글(server 11차): 받은 HTML 뒤에 그 쪽이 부르는 묶음을 같은 서버에서 받아 이어 붙인다.
//! 쓰는 바이너리는 `screen_refs`(refs.rs)도 넣는다.

use std::collections::BTreeSet;
use std::net::SocketAddr;
use std::time::Duration;

use crate::screen_refs::{bundle_refs, relative_imports};

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
