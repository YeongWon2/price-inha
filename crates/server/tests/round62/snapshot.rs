//! (결정 32) 시연 하루의 모의계좌 스냅샷 기록량: 시세만 바뀌는 줄은 남기지 않고, 장 마감 뒤에 그날 끝값 한 줄을 남긴다.
//!
//! 이 차수 전(같은 발판, 시연 하루 00:00 → 23:58 2분씩): 1,319줄 · 약 678KB — 줄 사이의 바뀐 칸은 모두
//! `ledger.last_now` · `ledger.last_quotes` · `ledger.trade_marks`(시세 관찰 칸)뿐이었다. 이 차수 뒤: 2줄 · 808바이트.

use std::time::Duration;

use chrono::{DateTime, Utc};
use price_inha::DB_FILE;

use crate::common::{Bench, count, kst, today};

/// 시연 하루(시연 정규장 00:05 ~ 23:55 한국 — 00:00 → 23:58, 2분씩)를 돌린다.
async fn run_demo_day(bench: &Bench) {
    let mut at = kst(today(), 0, 0);
    let end = kst(today(), 23, 58);
    while at < end {
        bench.set_clock(at);
        tokio::time::sleep(Duration::from_millis(25)).await;
        at += chrono::Duration::minutes(2);
    }
    bench.set_clock(end);
}

fn millis(at: DateTime<Utc>) -> i64 {
    at.timestamp_millis()
}

/// 조건이 될 때까지 기다린다(가장 길게 20초).
async fn wait_until(mut done: impl FnMut() -> bool) -> bool {
    for _ in 0..400 {
        if done() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    done()
}

// 검증: V6 D6
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn 시연_하루의_스냅샷은_시세만_바뀐_줄을_남기지_않고_마감_끝값을_남긴다() {
    let bench = Bench::start(kst(today(), 0, 0), |_| {}).await;
    run_demo_day(&bench).await;
    let path = bench.home().join(DB_FILE);
    let close = millis(kst(today(), 23, 55));
    let day_end = format!("SELECT COUNT(*) FROM paper_snapshots WHERE at >= {close}");
    assert!(
        wait_until(|| count(&path, &day_end) >= 1).await,
        "장 마감 뒤에 그날 끝값 한 줄이 있다"
    );
    let rows = count(&path, "SELECT COUNT(*) FROM paper_snapshots");
    let bytes = count(
        &path,
        "SELECT COALESCE(SUM(LENGTH(state)), 0) FROM paper_snapshots",
    );
    eprintln!("시연 하루 스냅샷: {rows}줄 · {bytes}바이트 (이 차수 전 1,319줄 · 약 678KB)");
    assert!(rows <= 20, "시세만 바뀐 줄은 남지 않는다: {rows}줄");
    bench.close().await;
}
