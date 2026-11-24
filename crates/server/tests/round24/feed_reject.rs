//! 21차 3번: 증권사가 거절한 보유 종목도 이상 감지에 남는다.
//!
//! 가짜 시세 서버가 종목을 거절(404 `stock-not-found`)하게 만든다. 거절된 종목은 시세 흐름이 묻는 목록에서
//! 빠지지만, 보유 중이면 그 종목의 시세가 끊긴 것으로 세어 시계가 30초 넘게 가면 이상 감지 멈춤이 된다.
//! (이상 감지 멈춤 중에는 비정상 바퀴를 세지 않는 것은 `src/trading/money_safety.rs`의 단위 시험이 본다.)

use std::time::{Duration, Instant};

use pi_store::TeamState;

use crate::common::{ENTRY, HYNIX, Plan, STEP, WATCHED, kst, monday, settle};

// 검증: N6 V11 P11 E14
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 증권사가_거절한_보유_종목의_시세가_끊기면_이상_감지_멈춤이_된다() {
    let bench = Plan::live()
        .watching(&[WATCHED])
        .start(kst(monday(), 10, 0))
        .await;
    bench.ready().await;
    bench.set_price(HYNIX, ENTRY);
    let _held = bench.holding(HYNIX).await;
    let deadline = Instant::now() + STEP;
    while !bench
        .asked_symbols()
        .await
        .iter()
        .any(|symbols| symbols.contains(HYNIX))
    {
        assert!(
            Instant::now() < deadline,
            "시간 안에 되지 않았다: 시세 흐름이 보유 종목을 묻는다"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(bench.team(), TeamState::Operating, "시작은 운용 중이다");

    // 증권사가 보유 종목을 거절한다. 지켜보는 종목의 시세는 계속 온다.
    bench.reject_symbol(HYNIX);
    settle().await;
    let deadline = Instant::now() + STEP;
    while bench.team() != TeamState::AnomalyHalt {
        assert!(
            Instant::now() < deadline,
            "시간 안에 되지 않았다: 거절된 보유 종목의 시세 끊김으로 이상 감지 멈춤"
        );
        bench.advance(20).await;
    }
    bench.close().await;
}

// 검증: N6 V11
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 보유하지_않은_종목을_증권사가_거절해도_이상_감지_멈춤이_되지_않는다() {
    let bench = Plan::live()
        .watching(&[WATCHED, HYNIX])
        .priced(HYNIX, ENTRY)
        .start(kst(monday(), 10, 0))
        .await;
    bench.ready().await;
    settle().await;
    bench.reject_symbol(HYNIX);
    settle().await;
    for _ in 0..4 {
        bench.advance(20).await;
    }
    assert_eq!(
        bench.team(),
        TeamState::Operating,
        "보유하지 않은 거절 종목은 지켜보기에서 빠져 멈추지 않는다"
    );
    bench.close().await;
}
