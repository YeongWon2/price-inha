//! 15번 메모리(N1): 시연 모드로 띄워 논의 · 시세 · 트리거(깨우기)를 돌린 뒤 프로세스 RSS가 500MB 아래다.
//!
//! 유닉스만 잰다. 리눅스는 `/proc/self/status`의 `VmRSS`를 읽고, `/proc`이 없는 유닉스(macOS 등)는 운영체제에
//! 딸린 `ps -o rss= -p <pid>`로 잰다(시험에 새 크레이트를 더하지 않으려고 — `getrusage`는 `unsafe`나 새 의존이
//! 필요하다). 서버는 시험 프로세스 안에서 돌므로 이 프로세스의 RSS가 서버 메모리다(시험 도구의 몫을 더해 넉넉하다).

use std::time::Duration;

use crate::common::{Bench, ENTRY, HYNIX, WATCHED, kst, monday, settle};

/// 상한(N1): 500MB.
const LIMIT_KB: u64 = 500 * 1024;

/// 이 프로세스의 지금 RSS(킬로바이트).
fn rss_kb() -> u64 {
    if let Ok(status) = std::fs::read_to_string("/proc/self/status") {
        let line = status
            .lines()
            .find(|line| line.starts_with("VmRSS:"))
            .expect("VmRSS 줄");
        return line
            .split_whitespace()
            .nth(1)
            .and_then(|kb| kb.parse().ok())
            .expect("VmRSS 값");
    }
    let pid = std::process::id().to_string();
    let output = std::process::Command::new("ps")
        .args(["-o", "rss=", "-p", &pid])
        .output()
        .expect("ps를 부를 수 있어야 한다");
    String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse()
        .expect("ps의 RSS 값")
}

// 검증: N1
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 시연_모드로_논의와_시세와_깨우기를_돌린_뒤_메모리가_500mb_아래다() {
    let bench = Bench::start(kst(monday(), 10, 0)).await;
    bench.ready().await;
    for round in 0..3 {
        bench.set(bench.now() + chrono::Duration::minutes(5));
        bench.quote(HYNIX, ENTRY, bench.now()).await;
        let _ = bench.crew().discuss(HYNIX, "자기주식취득결정 공시").await;
        let _ = bench.crew().discuss(WATCHED, "시세 3% 움직임").await;
        for tick in 0..50 {
            let price = ENTRY + (tick % 7) * 100 + round * 1_000;
            let at = bench.now() + chrono::Duration::seconds(1);
            bench.quote(HYNIX, price, at).await;
        }
        // 시연 시세 흐름과 깨우기(트리거)가 뒤에서 돌게 둔다.
        tokio::time::sleep(Duration::from_millis(500)).await;
        bench.advance(120).await;
    }
    settle().await;
    let used = rss_kb();
    assert!(used > 0, "RSS를 읽었다");
    assert!(
        used < LIMIT_KB,
        "서버 메모리는 500MB 아래다(N1): {} MB",
        used / 1024
    );
    bench.close().await;
}
