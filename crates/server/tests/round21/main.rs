//! 15차(`docs/dev/modules/server.md` "15차 남은 후보") 3번: 화면 머리(CSP)의 `connect-src`.
//!
//! https 뒤(역방향 프록시 · Tailscale serve)에서 화면이 `wss://`로 붙어도 막히지 않게, 같은 Host의
//! `wss://<Host>`를 `ws://<Host>`와 함께 적는다. 다른 주소는 더하지 않는다. 서버는 임시 보관 폴더 ·
//! `127.0.0.1:0` · 시연 모드로 띄운다(바깥으로 나가지 않는다). 이상한 Host는 접속 관문이 403으로 막아 화면까지
//! 오지 못하므로, "이상하면 `'self'`만"은 `src/page.rs`의 단위 시험이 본다.

#[path = "../round20/common.rs"]
mod common;

use common::Bench;

/// 응답 머리 `content-security-policy`에서 `connect-src`의 항목들.
fn connect_src(policy: &str) -> Vec<String> {
    let directive = policy
        .split(';')
        .map(str::trim)
        .find(|d| d.starts_with("connect-src"))
        .unwrap_or_else(|| panic!("connect-src가 있어야 한다: {policy}"));
    directive
        .split_whitespace()
        .skip(1)
        .map(str::to_owned)
        .collect()
}

async fn page_policy(addr: std::net::SocketAddr, path: &str) -> (u16, String) {
    let request = reqwest::Client::new().get(format!("http://{addr}{path}"));
    let reply = tokio::time::timeout(common::STEP, request.send())
        .await
        .expect("응답이 시간 안에 와야 한다")
        .expect("요청이 닿아야 한다");
    let status = reply.status().as_u16();
    let policy = reply
        .headers()
        .get("content-security-policy")
        .map(|v| v.to_str().unwrap().to_owned())
        .unwrap_or_default();
    (status, policy)
}

// 검증: D11 V46
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 화면_머리의_connect_src에는_ws와_wss가_같은_host로_함께_있다() {
    let bench = Bench::start().await;
    let addr = bench.addr();
    let host = format!("127.0.0.1:{}", addr.port());
    for path in ["/", "/office/", "/phone/"] {
        let (status, policy) = page_policy(addr, path).await;
        assert_eq!(status, 200, "{path}");
        let mut items = connect_src(&policy);
        items.sort();
        let mut want = vec![
            "'self'".to_owned(),
            format!("ws://{host}"),
            format!("wss://{host}"),
        ];
        want.sort();
        assert_eq!(
            items, want,
            "{path}: connect-src는 'self'와 같은 Host의 ws · wss뿐이다: {policy}"
        );
    }
    bench.close().await;
}
