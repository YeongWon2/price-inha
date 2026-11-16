//! 손실 기준(그날 시작 평가액 · 최고 평가액)을 저장소에 남겨 다시 켜도 그대로다(P3 · P4 · V8 · V9 · V6).
//!
//! 다시 켜면 시세가 아직 없어 보유 종목은 평균 단가로 셈한다. 그래서 기준을 되읽지 않으면 이미 낸 손실 ·
//! 오른 뒤 내린 몫이 기준에서 빠져 한도가 새로 시작한다. 이 파일은 그 빈틈을 본다.

use chrono::Duration;
use pi_rules::Purpose;
use pi_store::{TeamCause, TeamState, TradeId, TradeState};
use price_inha::{KIND_DAY_START, Placed};
use serde_json::json;

use crate::common::{
    Bench, HYNIX, NAVER, kst, monday, on_date, sell, swing_with, tuesday, wait_for,
};

/// 보유중인 매매를 그 값에 손절로 판다: 시세를 넣고, 내고, 1초 뒤 같은 값의 시세로 체결시킨다.
async fn stop_out(bench: &Bench, trade: TradeId, symbol: &str, price: i64) {
    let at = bench.now() + Duration::minutes(1);
    bench.quote(symbol, price, at).await;
    let placed = bench
        .place(sell(trade, symbol, Purpose::StopLoss, price, at))
        .await;
    assert!(
        matches!(placed, Placed::Ordered(_)),
        "손절은 바로 나간다: {placed:?}"
    );
    bench.quote(symbol, price, at + Duration::seconds(1)).await;
    assert_eq!(bench.trade_state(trade), TradeState::Sold, "체결돼 팔렸다");
}

/// 마지막 팀 상태 기록의 (이전 상태, 까닭).
fn last_change(bench: &Bench) -> (TeamState, TeamCause) {
    let last = bench.store().team_history().unwrap().pop().unwrap();
    (last.from, last.cause)
}

// 검증: V8 V6 T13
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 손실을_낸_뒤_다시_띄워도_그날_시작_평가액이_그대로라_남은_한도만큼만_더_잃으면_멈춘다() {
    let bench = Bench::demo(kst(monday(), 10, 0)).await;
    swing_with(
        bench.addr(),
        json!({"daily_loss_limit_pct": "0.5", "max_drawdown_pct": "5"}),
    )
    .await;
    bench.trade_today();
    let hynix = bench.hold(HYNIX).await;
    // 5주 × 6,000원 손절 + 수수료 · 세금 ≈ 31,085원(평가액의 약 0.31%): 하루 한도 0.5%(50,000원) 안이다.
    stop_out(&bench, hynix, HYNIX, 94_000).await;
    assert_eq!(bench.team(), TeamState::Operating, "아직 한도 안이다");

    let dir = bench.close().await;
    let bench = Bench::open(dir, kst(monday(), 10, 20)).await;
    assert_eq!(bench.team(), TeamState::Operating, "다시 켜도 운용 중이다");
    bench.hold(NAVER).await;

    // 5주 × 3,000원 + 수수료 ≈ 15,075원 더: 합 약 46,160원(0.46%)으로 아직 한도 안이다.
    bench.quote(NAVER, 97_000, kst(monday(), 10, 25)).await;
    assert_eq!(
        bench.team(),
        TeamState::Operating,
        "남은 한도 안이면 멈추지 않는다"
    );
    // 5주 × 4,500원 + 수수료 ≈ 22,575원 더: 합 약 53,660원(0.54%)으로 그날 시작 평가액의 한도를 넘는다.
    // 다시 켠 때의 평가액을 기준으로 잡으면 0.23%라 멈추지 않는다.
    bench.quote(NAVER, 95_500, kst(monday(), 10, 26)).await;
    assert_eq!(
        bench.team(),
        TeamState::DailyLimitHalt,
        "다시 켜기 전에 낸 손실까지 합해 남은 한도만 더 잃으면 멈춘다"
    );
    assert_eq!(
        last_change(&bench),
        (TeamState::Operating, TeamCause::DailyLossLimit)
    );
    bench.close().await;
}

// 검증: V9 V6 T13
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 오른_뒤_다시_띄워도_최고_평가액이_그대로라_그만큼_내리면_전체_정지가_된다() {
    let bench = Bench::demo(kst(monday(), 10, 0)).await;
    swing_with(
        bench.addr(),
        json!({"daily_loss_limit_pct": "0.5", "max_drawdown_pct": "1"}),
    )
    .await;
    bench.trade_today();
    bench.hold(HYNIX).await;
    // 5주 × 20,000원 오른다: 최고 평가액 약 10,099,925원.
    bench.quote(HYNIX, 120_000, kst(monday(), 10, 5)).await;
    assert_eq!(bench.team(), TeamState::Operating);

    let dir = bench.close().await;
    let bench = Bench::open(dir, kst(monday(), 10, 20)).await;
    assert_eq!(bench.team(), TeamState::Operating, "다시 켜도 운용 중이다");

    // 최고에서 50,000원(0.5%) 내림: 최대 낙폭 1% 안이다.
    bench.quote(HYNIX, 110_000, kst(monday(), 10, 21)).await;
    assert_eq!(bench.team(), TeamState::Operating, "아직 최대 낙폭 안이다");
    // 최고에서 105,000원(약 1.04%) 내림. 그날 시작(10,000,000원)보다는 약 5,075원만 적어 하루 한도와는 멀다.
    // 다시 켠 뒤의 평가액을 최고로 잡으면 0.55%라 멈추지 않는다.
    bench.quote(HYNIX, 99_000, kst(monday(), 10, 22)).await;
    assert_eq!(
        bench.team(),
        TeamState::FullStop,
        "다시 켜기 전의 최고 평가액에서 최대 낙폭만큼 내리면 전체 정지다"
    );
    assert_eq!(
        last_change(&bench),
        (TeamState::Operating, TeamCause::MaxLoss)
    );
    bench.close().await;
}

// 검증: V9 V6 T13
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 잃은_다음_날_다시_띄워도_최고_평가액은_처음_돈_아래로_내려가지_않는다() {
    let bench = Bench::demo(kst(monday(), 10, 0)).await;
    swing_with(
        bench.addr(),
        json!({"daily_loss_limit_pct": "0.9", "max_drawdown_pct": "1"}),
    )
    .await;
    bench.trade_today();
    let hynix = bench.hold(HYNIX).await;
    // 약 31,085원(0.31%)을 잃고 판다. 평가액 약 9,968,915원.
    stop_out(&bench, hynix, HYNIX, 94_000).await;
    bench.set(kst(monday(), 15, 40));
    wait_for("마감 뒤 쉬는 중", || {
        bench.team() == TeamState::Resting
    })
    .await;

    let dir = bench.close().await;
    let bench = Bench::open(dir, kst(tuesday(), 9, 30)).await;
    let store = bench.store();
    wait_for("둘째 날 시작", || {
        on_date(&store, KIND_DAY_START, tuesday()) == 1
    })
    .await;
    bench.trade_today();
    bench.hold(NAVER).await;

    // 5주 × 10,000원 + 수수료 ≈ 50,075원 더: 처음 돈에서 약 0.81%, 그날 시작에서 약 0.50%.
    bench.quote(NAVER, 90_000, kst(tuesday(), 9, 40)).await;
    assert_eq!(bench.team(), TeamState::Operating, "아직 두 한도 안이다");
    // 5주 × 14,500원 + 수수료 ≈ 72,575원 더: 처음 돈(최고 평가액)에서 약 1.04%, 그날 시작에서 약 0.73%.
    // 다시 켠 때의 평가액을 최고로 잡으면 0.73%라 멈추지 않는다.
    bench.quote(NAVER, 85_500, kst(tuesday(), 9, 41)).await;
    assert_eq!(
        bench.team(),
        TeamState::FullStop,
        "최고 평가액은 처음 돈 아래로 내려가지 않아 처음 돈에서 최대 낙폭만큼 잃으면 전체 정지다"
    );
    assert_eq!(
        last_change(&bench),
        (TeamState::Operating, TeamCause::MaxLoss)
    );
    bench.close().await;
}
