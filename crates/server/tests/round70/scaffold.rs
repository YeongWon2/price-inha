//! 발판: 지금 구현에서 가짜 토스증권의 시세가 실시간 연결로 온다.

use crate::common::{
    FakeToss, STEP, TestClock, connect, find_quotes, kst, quote_of, register_keys, settings,
    start_app, stop, wiring,
};

// 검증: V32
#[tokio::test]
async fn 발판_증권사_시세가_실시간_연결로_온다() {
    let toss = FakeToss::start(None).await;
    let dir = tempfile::tempdir().unwrap();
    register_keys(dir.path(), &toss).await;
    let app = start_app(
        settings(dir.path(), &[]),
        wiring(Some(&toss), TestClock::at(kst(25, 10, 0))),
    )
    .await;
    let mut ws = connect(&app).await;
    let price = find_quotes(&mut ws, STEP, |quotes| {
        quote_of(quotes, "005930")?["price"]
            .as_str()
            .map(str::to_string)
    })
    .await;
    assert_eq!(price.as_deref(), Some("72000"));
    stop(app).await;
}
