//! 실제 시세 모드의 결정 메모 정산: 기록된 거래일로 5거래일을 센다.

use serde_json::Value;

use crate::common::{Bench, day, monday};
use crate::outside::LESSON;

/// 결정 날 종가.
const DECIDED: i64 = 100_000;
/// 정산 날 종가(수익률 10%).
const SETTLED: i64 = 110_000;
/// 그 사이 날의 종가.
const BETWEEN: i64 = 103_000;

fn assert_settled(memo: &Value) {
    assert_eq!(memo["state"], "settled", "{memo}");
    assert_eq!(memo["return_pct"].as_f64(), Some(10.0), "{memo}");
    assert_eq!(memo["hold_days"], 5, "{memo}");
    assert_eq!(memo["lesson"], LESSON, "{memo}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn 실제_시세_모드에서도_6거래일을_지나면_정산되고_주말은_세지_않는다() {
    // 검증: V70
    let bench = Bench::start(&[]).await;
    let trade = bench.judged_buy().await;
    bench.live_day(monday(), DECIDED).await;
    for n in 1..=4 {
        bench.live_day(day(n), BETWEEN).await;
    }
    // 토 · 일(달력이 휴장이라 한다).
    bench.live_day(day(5), BETWEEN).await;
    bench.live_day(day(6), BETWEEN).await;
    assert!(
        bench.pending_through(monday()).await >= 1,
        "4거래일 뒤에는 기다린다"
    );
    assert_eq!(bench.memo_of(trade).await["state"], "pending");

    // 다음 월요일 = 결정 날에서 5거래일 뒤.
    bench.live_day(day(7), SETTLED).await;
    assert_settled(&bench.memo_of(trade).await);
    assert_eq!(
        bench.pending_through(monday()).await,
        0,
        "결정 날의 정산 대기 수가 줄어든다"
    );
    bench.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn 달력이_휴장이라_한_날과_서버가_꺼져_있던_날은_거래일로_세지_않는다() {
    // 검증: V70
    // 수요일(11-18)은 달력이 휴장, 목요일(11-19)은 서버가 꺼져 있다.
    let mut bench = Bench::start(&[day(2)]).await;
    let trade = bench.judged_buy().await;
    bench.live_day(monday(), DECIDED).await;
    bench.live_day(day(1), BETWEEN).await;
    bench.live_day(day(2), BETWEEN).await;
    bench.stop().await;
    bench.restart_on(day(4), BETWEEN).await;
    // 기록된 거래일: 화(1) · 금(2) · 월(3) · 화(4) · 수(5).
    for n in [4, 5, 6, 7, 8] {
        bench.live_day(day(n), BETWEEN).await;
    }
    assert_eq!(
        bench.memo_of(trade).await["state"],
        "pending",
        "휴장일 · 꺼져 있던 날을 빼면 아직 4거래일이다"
    );
    bench.live_day(day(9), SETTLED).await;
    assert_settled(&bench.memo_of(trade).await);
    bench.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn 실제_시세_모드에서_정산_날_종가가_없으면_5거래일_더_지나_정산_못_함이_된다() {
    // 검증: E54
    let bench = Bench::start(&[]).await;
    let trade = bench.judged_buy().await;
    bench.live_day(monday(), DECIDED).await;
    // 그 종목 시세를 받지 못한 날들(그 날 종가가 없다). 정산 날은 다음 월요일(7).
    for n in 1..=13 {
        bench.silent_day(day(n)).await;
    }
    assert_eq!(
        bench.memo_of(trade).await["state"],
        "pending",
        "정산 날에서 4거래일 뒤에는 시세를 기다린다"
    );
    bench.silent_day(day(14)).await;
    let memo = bench.memo_of(trade).await;
    assert_eq!(memo["state"], "unsettled", "{memo}");
    assert_eq!(memo["unsettled_reason"], "no_price", "{memo}");
    assert_eq!(bench.pending_through(monday()).await, 0);
    bench.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn 판단_뒤_도구가_실패해_논의가_오류로_끝나도_저장된_판단으로_메모를_남긴다() {
    // 검증: V70
    // 템플릿을 고르지 않아 사기 판단 뒤 주문 도구가 실패한다.
    let bench = Bench::start_bare(&[]).await;
    let result = bench.discuss().await;
    assert!(
        result.is_err(),
        "판단 뒤 주문 도구가 실패해 논의가 오류로 끝난다"
    );
    let memos = bench.memos().await;
    assert_eq!(
        memos.len(),
        1,
        "저장된 판단(사기)으로 메모가 남는다: {memos:?}"
    );
    assert_eq!(memos[0]["decision"], "buy");
    assert_eq!(memos[0]["state"], "pending");
    bench.close().await;
}
