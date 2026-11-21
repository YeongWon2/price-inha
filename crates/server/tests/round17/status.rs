//! 1번 VI 추정 잇기(결정 2026-11-21 5 · 11 · 14번): 상태를 주는 공급자가 없으면(시연이 아닌 모드) 종목별 최근
//! 시세로 거래 상태를 추정해 점검에 넣는다. 추정이 정상일 때만 AI가 낸 사는 주문이 점검을 지난다.
//!
//! 시연이 아닌 모드로 띄운다(로컬 가짜 시세 · AI 서버). 논의하는 종목의 시세는 테스트가
//! [`price_inha::Trading::on_quote`]로 넣는다(받은 시각 = 가짜 시계).

use chrono::{DateTime, Utc};
use pi_rules::{Reason, Violation};
use pi_store::TradeState;
use price_inha::{KIND_DAY_CLOSE, KIND_DAY_START, Placed};

use crate::common::{
    Bench, ENTRY, HYNIX, STOP, kst, monday, on_date, plus, settle, tuesday, wait_for,
};

const TOPIC: &str = "자기주식취득결정 공시";

/// 논의 하나의 주문 길 결과.
async fn discuss(bench: &Bench) -> Option<Placed> {
    let discussed = bench.crew().discuss(HYNIX, TOPIC).await;
    discussed.expect("논의가 끝나야 한다").placed
}

/// VI · 거래정지 여부를 몰라 막혔는가.
fn status_unknown(placed: &Option<Placed>) -> bool {
    matches!(placed, Some(Placed::Blocked(blocked))
        if matches!(&blocked.reason, Reason::Violations(found)
            if found.contains(&Violation::StatusUnknown)))
}

/// 월요일 정규장 끝까지 시세를 흘리고 마감한 뒤, 화요일 장을 열어 운용 중으로 만든다.
/// 화요일 09:01을 돌려준다. 월요일의 마지막 정규장 시세가 화요일의 기준가다.
async fn into_second_day(bench: &Bench) -> DateTime<Utc> {
    bench.stream(100, plus(bench.now(), 20), 60).await;
    bench.set(kst(monday(), 15, 31));
    let store = bench.store();
    wait_for("월요일 마감", || {
        on_date(&store, KIND_DAY_CLOSE, monday()) == 1
    })
    .await;
    // 마감 일이 끝난 뒤에 밤을 건넌다.
    settle().await;
    let open = kst(tuesday(), 9, 1);
    bench.set(open);
    wait_for("화요일 시작", || {
        on_date(&store, KIND_DAY_START, tuesday()) >= 1
    })
    .await;
    settle().await;
    bench.trade_today();
    open
}

// 검증: V24 E31 E34 N6 P12
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 기준가가_없는_첫날은_매수가_막히고_다음_날_추정이_정상이면_지나며_손절은_막히지_않는다() {
    let start = kst(monday(), 15, 25);
    let bench = Bench::live(start).await;
    bench.ready().await;

    // 첫날: 80초 동안 끊김 없이 움직이는 시세를 봤어도 지난 정규장 시세(기준가)가 없다(결정 11번).
    bench.stream(100, plus(start, 20), 80).await;
    let first = discuss(&bench).await;
    assert!(
        status_unknown(&first),
        "기준가가 없으면 '모름'으로 막힌다: {first:?}"
    );

    // 다음 날: 기준가(월요일 마지막 정규장 시세)가 있고 60초 넘게 끊김 없이 움직였다 → 정상.
    let open = into_second_day(&bench).await;
    bench.stream(100, open, 80).await;
    let second = discuss(&bench).await;
    assert!(
        matches!(second, Some(Placed::Ordered(_))),
        "추정이 정상이면 AI가 낸 사는 주문이 점검을 지난다: {second:?}"
    );

    // 산 것이 체결되고, 시세가 손절값 아래로 가면 추정과 상관없이 손절이 나간다.
    let at = bench.now();
    bench.quote(HYNIX, ENTRY, plus(at, 1)).await;
    let holding = bench
        .store()
        .trades_in_state(TradeState::Holding)
        .unwrap()
        .into_iter()
        .find(|trade| trade.symbol == HYNIX)
        .expect("산 매매가 보유중이다");
    // 30초 넘게 끊겼다 온 시세(추정은 '모름')로도 손절은 막히지 않는다. 20초씩 두 번 가서 지켜보는 종목은 늦지 않다.
    bench.set(plus(at, 21));
    settle().await;
    bench.quote(HYNIX, STOP - 1_000, plus(at, 41)).await;
    bench.quote(HYNIX, STOP - 1_000, plus(at, 42)).await;
    assert_eq!(
        bench.trade_state(holding.id),
        TradeState::Sold,
        "손절은 VI · 거래정지를 몰라도 나간다"
    );
    bench.close().await;
}

// 검증: V24 E31 N6 P12
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 가격이_그대로거나_시세가_30초_넘게_끊기면_막히고_다시_60초를_지켜보면_지난다() {
    let bench = Bench::live(kst(monday(), 15, 25)).await;
    bench.ready().await;
    let open = into_second_day(&bench).await;

    // 80초 동안 값이 한 번도 바뀌지 않았다(단일가 매매 중일 수 있다).
    bench.stream(0, open, 80).await;
    let still = discuss(&bench).await;
    assert!(
        status_unknown(&still),
        "가격이 그대로면 '모름'으로 막힌다: {still:?}"
    );

    // 움직이는 시세를 80초 본 뒤 40초 끊겼다(20초씩 두 번 가서 지켜보는 종목은 늦지 않다).
    let moving = bench.stream(100, plus(open, 100), 80).await;
    bench.set(plus(moving, 20));
    settle().await;
    let back = plus(moving, 40);
    bench.quote(HYNIX, ENTRY + 100, back).await;
    let gap = discuss(&bench).await;
    assert!(
        status_unknown(&gap),
        "30초 넘는 틈이 있으면 지켜본 것으로 치지 않는다: {gap:?}"
    );

    // 끊긴 뒤부터 다시 틈 없이 딱 60초를 지켜봤다(끊긴 뒤 첫 시세에서 20 · 40 · 60초).
    bench.stream(100, plus(back, 20), 40).await;
    let again = discuss(&bench).await;
    assert!(
        matches!(again, Some(Placed::Ordered(_))),
        "다시 60초를 지켜보면 지난다: {again:?}"
    );
    bench.close().await;
}
