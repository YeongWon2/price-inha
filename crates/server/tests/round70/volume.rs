//! V32: 시세 묶음의 종목마다 그날 누적 거래량(`volume`, 십진수 글자)이 실시간으로 실린다.

use rust_decimal::Decimal;
use std::str::FromStr;

use crate::common::{
    CANDLES_PATH, FakeToss, STEP, TestClock, Ws, connect, find_quotes, kst, query, quote_of,
    register_keys, settings, start_app, stop, volume_of, wiring,
};

fn today_candle(volume: &str) -> Option<(String, String)> {
    Some(("2026-03-25T00:00:00+09:00".to_string(), volume.to_string()))
}

/// 그 종목의 거래량이 `volume`인 시세 묶음이 올 때까지 기다린다.
async fn wait_volume(ws: &mut Ws, symbol: &str, volume: &str) {
    let seen = find_quotes(ws, STEP, |quotes| {
        (volume_of(quotes, symbol).as_deref() == Some(volume)).then_some(())
    })
    .await;
    assert!(
        seen.is_some(),
        "{symbol}의 거래량 {volume}이 실시간 연결로 와야 한다"
    );
}

// 검증: V32
#[tokio::test]
async fn 시연_시세에도_그날_거래량이_실리고_받을_때마다_는다() {
    let dir = tempfile::tempdir().unwrap();
    let app = start_app(
        settings(dir.path(), &["--demo"]),
        wiring(None, TestClock::at(kst(25, 10, 0))),
    )
    .await;
    let mut ws = connect(&app).await;
    let mut seen: Vec<Decimal> = Vec::new();
    let grew = find_quotes(&mut ws, STEP, |quotes| {
        let text = volume_of(quotes, "005930")?;
        let volume = Decimal::from_str(&text).expect("거래량은 십진수 글자다");
        assert!(volume >= Decimal::ZERO, "거래량은 0 이상이다: {text}");
        if let Some(last) = seen.last() {
            assert!(
                volume >= *last,
                "그날 누적이므로 줄지 않는다: {seen:?} → {volume}"
            );
        }
        seen.push(volume);
        (seen.len() >= 3 && seen.first() < seen.last()).then_some(())
    })
    .await;
    assert!(
        grew.is_some(),
        "시연 거래량은 받을 때마다 가짜 체결 수량만큼 는다: {seen:?}"
    );
    stop(app).await;
}

// 검증: V32
#[tokio::test]
async fn 증권사_시세_묶음에_일봉의_그날_거래량이_실린다() {
    let toss = FakeToss::start(today_candle("3521000")).await;
    let dir = tempfile::tempdir().unwrap();
    register_keys(dir.path(), &toss).await;
    let app = start_app(
        settings(dir.path(), &[]),
        wiring(Some(&toss), TestClock::at(kst(25, 10, 0))),
    )
    .await;
    let mut ws = connect(&app).await;
    wait_volume(&mut ws, "005930", "3521000").await;
    wait_volume(&mut ws, "000660", "3521000").await;

    let asked = toss.requests(CANDLES_PATH).await;
    assert!(
        asked
            .iter()
            .all(|r| query(r, "interval").as_deref() == Some("1d")),
        "일봉을 묻는다"
    );
    stop(app).await;
}

// 검증: V32
#[tokio::test]
async fn 거래량을_모르면_시세_묶음의_모양은_그대로다() {
    let toss = FakeToss::start(None).await;
    let dir = tempfile::tempdir().unwrap();
    register_keys(dir.path(), &toss).await;
    let app = start_app(
        settings(dir.path(), &[]),
        wiring(Some(&toss), TestClock::at(kst(25, 10, 0))),
    )
    .await;
    let mut ws = connect(&app).await;
    let mut count = 0;
    let done = find_quotes(&mut ws, STEP, |quotes| {
        let quote = quote_of(quotes, "005930")?;
        assert_eq!(quote["price"].as_str(), Some("72000"), "가격은 그대로 온다");
        assert!(
            quote.get("volume").is_none(),
            "모르는 거래량은 0으로 지어내지 않고 칸을 두지 않는다: {quote}"
        );
        count += 1;
        (count >= 5).then_some(())
    })
    .await;
    assert!(done.is_some(), "시세는 계속 온다");
    assert!(
        !toss.requests(CANDLES_PATH).await.is_empty(),
        "거래량을 물어는 봤다(가짜 서버가 404로 답했다)"
    );
    stop(app).await;
}

// 검증: V32
#[tokio::test]
async fn 날이_바뀌면_어제_거래량을_싣지_않고_0부터다() {
    let toss = FakeToss::start(today_candle("3521000")).await;
    let dir = tempfile::tempdir().unwrap();
    register_keys(dir.path(), &toss).await;
    let clock = TestClock::at(kst(25, 10, 0));
    let app = start_app(
        settings(dir.path(), &[]),
        wiring(Some(&toss), std::sync::Arc::clone(&clock)),
    )
    .await;
    let mut ws = connect(&app).await;
    wait_volume(&mut ws, "005930", "3521000").await;

    // 다음 날 아침. 증권사의 마지막 일봉은 아직 어제(25일) 것이다.
    clock.set(kst(26, 10, 0));
    wait_volume(&mut ws, "005930", "0").await;
    let mut after = 0;
    let stale = find_quotes(&mut ws, STEP, |quotes| {
        after += 1;
        if volume_of(quotes, "005930").as_deref() == Some("3521000") {
            return Some(true);
        }
        (after >= 5).then_some(false)
    })
    .await;
    assert_eq!(
        stale,
        Some(false),
        "날이 바뀐 뒤에는 어제 거래량을 싣지 않는다"
    );
    stop(app).await;
}

// 검증: V32
#[tokio::test]
async fn 거래량은_시세마다_묻지_않고_덜_자주_읽는다() {
    let toss = FakeToss::start(today_candle("3521000")).await;
    let dir = tempfile::tempdir().unwrap();
    register_keys(dir.path(), &toss).await;
    let app = start_app(
        settings(dir.path(), &[]),
        wiring(Some(&toss), TestClock::at(kst(25, 10, 0))),
    )
    .await;
    let mut ws = connect(&app).await;
    wait_volume(&mut ws, "000660", "3521000").await;
    let mut count = 0;
    find_quotes(&mut ws, STEP, |_| {
        count += 1;
        (count >= 15).then_some(())
    })
    .await
    .expect("시세는 20밀리초마다 계속 온다");
    let asked: Vec<String> = toss
        .requests(CANDLES_PATH)
        .await
        .iter()
        .filter_map(|r| query(r, "symbol"))
        .collect();
    let per_symbol = asked.iter().filter(|s| *s == "005930").count();
    assert_eq!(
        per_symbol, 1,
        "시세 묶음 열다섯 번 동안 거래량은 종목마다 한 번만 읽는다(한도 그룹 MARKET_DATA_CHART): {asked:?}"
    );
    stop(app).await;
}
