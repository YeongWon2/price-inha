//! 10a 보강(검토 되돌림) 1 · 2 · 3번: 주문 · 상태 쪽.
//!
//! - 1번: 승인 답과 답 없음의 다시 점검은 보관해 둔 요청의 상태 추정이 아니라 그 자리의 추정으로 한다
//!   (결정 2026-11-21 5 · 14번). 추정이 '정상'일 때 승인대기가 된 사기라도, 다시 점검할 때 '모름'이면 나가지 않는다.
//! - 2번: 팀장 팔기는 같은 종목의 다른 보유 매도를 취소하지 않는다(결정 13번, exec 7차 `SellPurpose::TeamExit`).
//! - 3번: 자료 키가 없어 못 받은 것은 자료 이상이 아니다(결정 8번). 키가 있는 출처가 연속으로 실패한 것만 센다.
//!
//! 1 · 3번은 시연이 아닌 모드(로컬 가짜 시세 · AI 서버)로, 2번은 시연 모드로 띄운다.

use chrono::{DateTime, Utc};
use pi_agent::Decision;
use pi_approval::{ApprovalNumber, DueHandling};
use pi_paper::{Fault, Order, OrderGroup, Side};
use pi_rules::{Purpose, Reason, Violation};
use pi_store::{TeamCause, TeamState, TradeId, TradeState};
use price_inha::{APPROVAL_ANSWER_PATH, DemoTurn, KIND_DAY_CLOSE, KIND_DAY_START, Placed};
use serde_json::json;

use crate::common::{
    Bench, ENTRY, HYNIX, STEP, TARGET, d, kst, monday, on_date, order, plus, post, settle,
    subscribe, tuesday, verdict, wait_for,
};

const TOPIC: &str = "자기주식취득결정 공시";
/// 규칙 엔진이 VI · 거래정지 여부를 몰라 막을 때 남기는 글(`pi_rules::Violation::StatusUnknown`).
const STATUS_UNKNOWN_TEXT: &str = "VI · 거래정지 여부를 알 수 없다";

/// 모의계좌에 낸 [`HYNIX`]의 그 방향 주문들(걸린 것과 끝난 것).
async fn sent(bench: &Bench, side: Side) -> Vec<Order> {
    let mut orders = bench.orders(OrderGroup::Open).await;
    orders.extend(bench.orders(OrderGroup::Closed).await);
    orders.retain(|order| order.side == side && order.symbol.as_str() == HYNIX);
    orders
}

// ───────────── 1번: 다시 점검은 지금의 상태 추정으로 ─────────────

/// 추정 '정상'으로 승인대기가 된 사는 주문 하나.
struct Waiting {
    trade: TradeId,
    number: ApprovalNumber,
    fingerprint: String,
    /// 승인을 요청한 시각.
    asked: DateTime<Utc>,
}

/// 월요일 정규장 끝까지 시세를 흘리고 마감한 뒤 화요일 장을 열어 운용 중으로 만든다(월요일의 마지막 정규장
/// 시세가 화요일의 기준가다). 화요일 09:01을 돌려준다.
async fn into_second_day(bench: &Bench) -> DateTime<Utc> {
    bench.stream(100, plus(bench.now(), 20), 60).await;
    bench.set(kst(monday(), 15, 31));
    let store = bench.store();
    wait_for("월요일 마감", || {
        on_date(&store, KIND_DAY_CLOSE, monday()) == 1
    })
    .await;
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

/// 자동 주문 금액 한도를 2.5%(25만 원)로 낮추고 다음 날 추정이 '정상'일 때 AI가 사는 주문 안을 내게 해
/// 승인대기를 만든다(종목 비중 10% = 10주 · 100만 원이 한도를 넘는다).
async fn awaiting(bench: &Bench) -> Waiting {
    bench
        .ready_with(json!({"symbol_weight_pct": "10", "auto_order_limit_pct": "2.5"}))
        .await;
    let open = into_second_day(bench).await;
    // 기준가가 있고 60초 넘게 끊김 없이 움직였다 → 추정은 '정상'이다.
    bench.stream(100, open, 80).await;
    let discussed = bench.crew().discuss(HYNIX, TOPIC).await;
    let discussed = discussed.expect("논의가 끝나야 한다");
    let Some(Placed::AwaitingApproval {
        number,
        fingerprint,
    }) = discussed.placed
    else {
        panic!(
            "추정이 정상이고 금액 한도를 넘으면 승인을 기다린다: {:?}",
            discussed.placed
        );
    };
    Waiting {
        trade: discussed.trade,
        number,
        fingerprint: fingerprint.as_str().to_string(),
        asked: bench.now(),
    }
}

// 검증: E22 N2 V24 V25 E31 P12 P20
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 답_없는_사기는_그때_추정이_모름이면_줄여서도_내지_않고_막혔다고_남긴다() {
    let bench = Bench::live_with_push(kst(monday(), 15, 25)).await;
    subscribe(bench.addr()).await;
    let before = bench.pushes().await;
    let Waiting {
        trade,
        number,
        asked,
        ..
    } = awaiting(&bench).await;
    // 승인 요청 알림이 간 것까지 기다린다(알림이 가야 답 없음이 "줄여 내기" 갈래로 온다).
    let deadline = std::time::Instant::now() + STEP;
    while bench.pushes().await <= before {
        assert!(
            std::time::Instant::now() < deadline,
            "승인 요청 알림이 가야 한다"
        );
        settle().await;
    }
    settle().await;

    // 기다릴 시간(5분)이 지날 때까지 값이 한 번도 바뀌지 않는 시세만 온다: 그때의 추정은 '모름'이다.
    bench.stream(0, plus(asked, 20), 300).await;
    wait_for("답 없음 처리 기록", || {
        bench.approvals().due_handling(&number).unwrap().is_some()
    })
    .await;
    let done = bench.approvals().due_handling(&number).unwrap();
    assert!(
        matches!(&done, Some(DueHandling::Blocked { reason }) if reason.contains(STATUS_UNKNOWN_TEXT)),
        "다시 점검은 그 자리의 추정('모름')으로 해 막힌다(보관해 둔 '정상'을 쓰지 않는다): {done:?}"
    );
    settle().await;
    let buys = sent(&bench, Side::Buy).await;
    assert!(buys.is_empty(), "사는 주문이 0건이다: {buys:?}");
    assert_ne!(bench.trade_state(trade), TradeState::Ordered);
    bench.close().await;
}

// 검증: E7 V24 V25 E31 P12
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 승인해도_그때_추정이_모름이면_사는_주문이_나가지_않는다() {
    let bench = Bench::live(kst(monday(), 15, 25)).await;
    let Waiting {
        trade,
        number,
        fingerprint,
        asked,
    } = awaiting(&bench).await;

    // 기다릴 시간 안에(100초 뒤) 승인한다. 그 사이 60초 넘게 값이 그대로인 시세만 왔다 → 그때의 추정은 '모름'이다.
    bench.stream(0, plus(asked, 20), 80).await;
    let body = json!({"number": number.as_str(), "fingerprint": fingerprint, "answer": "approve"});
    let reply = post(bench.addr(), APPROVAL_ANSWER_PATH, &body).await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert_eq!(
        reply.body["result"],
        json!("rejected"),
        "승인해도 다시 점검이 그 자리의 추정('모름')으로 막는다: {}",
        reply.body
    );
    settle().await;
    let buys = sent(&bench, Side::Buy).await;
    assert!(buys.is_empty(), "사는 주문이 0건이다: {buys:?}");
    assert_ne!(bench.trade_state(trade), TradeState::Ordered);
    bench.close().await;
}

// ───────────── 2번: 팀장 팔기는 다른 보유의 매도를 취소하지 않는다 ─────────────

/// 지금 시세 [`ENTRY`]로 `quantity`주를 사서 보유중까지 만든 매매.
async fn hold(bench: &Bench, quantity: &str) -> TradeId {
    let at = plus(bench.now(), 1);
    bench.quote(HYNIX, ENTRY, at).await;
    let trade = bench.judged(HYNIX);
    let mut request = order(trade, HYNIX, Purpose::NewEntry, ENTRY, at);
    request.draft.ai_quantity = Some(d(quantity));
    let placed = bench.place(request).await;
    assert!(matches!(placed, Placed::Ordered(_)), "산다: {placed:?}");
    bench.quote(HYNIX, ENTRY, plus(at, 1)).await;
    assert_eq!(bench.trade_state(trade), TradeState::Holding);
    trade
}

/// 팀장은 팔기로 판단하고 매매 담당은 팔기 주문 안을 낸다고 정해 둔 뒤, 지금 시세로 논의한다.
async fn discuss_sell(bench: &Bench) -> Result<price_inha::Discussed, price_inha::TeamError> {
    bench.script(DemoTurn::Verdict, &verdict("sell"));
    let draft = json!({"side": "sell", "note": "보유 정리"});
    bench.script(DemoTurn::Draft, &draft.to_string());
    bench.quote(HYNIX, ENTRY, plus(bench.now(), 1)).await;
    bench.crew().discuss(HYNIX, "실적 전망 하향 공시").await
}

// 검증: V4 E30 P7 F4
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 같은_종목_보유가_둘이면_팀장_팔기_매도가_둘_다_남아_모두_팔린다() {
    let bench = Bench::start(kst(monday(), 10, 0)).await;
    bench.ready_with(json!({"symbol_weight_pct": "20"})).await;
    let big = hold(&bench, "10").await;
    let small = hold(&bench, "5").await;

    let discussed = discuss_sell(&bench).await.expect("논의가 끝나야 한다");
    assert_eq!(discussed.outcome.decision, Decision::Sell);
    assert_eq!(
        discussed.outcome.sells.len(),
        2,
        "보유 매매마다 팔기 주문 안 하나"
    );

    let mut open = bench.orders(OrderGroup::Open).await;
    open.retain(|order| order.side == Side::Sell && order.symbol.as_str() == HYNIX);
    assert_eq!(
        open.len(),
        2,
        "뒤에 낸 매도가 앞의 매도를 취소하지 않는다(열린 매도 둘): {open:?}"
    );
    let total = open
        .iter()
        .map(|order| order.quantity)
        .sum::<rust_decimal::Decimal>();
    assert_eq!(total, d("15"), "두 보유(10주 · 5주)를 모두 판다");
    let canceled = sent(&bench, Side::Sell)
        .await
        .into_iter()
        .filter(|order| order.canceled_at.is_some())
        .count();
    assert_eq!(canceled, 0, "취소된 매도가 없다");

    // 목표값 시세로 둘 다 체결된다.
    bench.quote(HYNIX, TARGET, plus(bench.now(), 1)).await;
    assert_eq!(bench.trade_state(big), TradeState::Sold);
    assert_eq!(bench.trade_state(small), TradeState::Sold);
    bench.close().await;
}

// 검증: V4 E13 E30 F4
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 팀장_팔기가_실패해_다시_내기를_기다릴_때_실행기_기록의_목적이_팀장_팔기다() {
    let bench = Bench::start(kst(monday(), 10, 0)).await;
    bench.ready().await;
    let held = bench.holding(HYNIX).await;

    // 모의계좌가 주문을 받지 않게 한다: 실패한 파는 주문은 실행기가 목적과 함께 계좌 스냅샷에 남긴다(E30).
    bench.trading().paper().set_fault(Some(Fault::RejectOrders));
    let _failed = discuss_sell(&bench).await;
    bench.trading().paper().set_fault(None);

    let snapshot = bench.store().latest_paper_snapshot().unwrap();
    let state = snapshot.expect("계좌 스냅샷이 있다").state;
    let pending = state["pi_exec"]["pending"].as_array().cloned();
    let pending = pending.unwrap_or_default();
    let mine: Vec<_> = pending
        .iter()
        .filter(|item| item["trade_id"] == json!(held.0))
        .collect();
    assert_eq!(
        mine.len(),
        1,
        "실패한 팀장 팔기가 다시 내기를 기다린다: {pending:?}"
    );
    assert_eq!(
        mine[0]["purpose"],
        json!("team_exit"),
        "실행기에 청산이 아니라 팀장 팔기로 넘긴다"
    );
    assert_eq!(
        bench.trade_state(held),
        TradeState::Holding,
        "보유중 그대로다"
    );
    bench.close().await;
}

// ───────────── 3번: 자료 키가 없는 것은 자료 이상이 아니다 ─────────────

/// 지금 시세로 [`HYNIX`]를 `times`번 논의한다(논의마다 자료를 받으러 간다).
async fn discuss_times(bench: &Bench, times: usize) {
    for _ in 0..times {
        bench.quote(HYNIX, ENTRY, plus(bench.now(), 1)).await;
        let discussed = bench.crew().discuss(HYNIX, TOPIC).await;
        discussed.expect("자료를 받지 못해도 논의는 끝난다(지켜보기, E32)");
    }
}

/// 근거가 다 찬 사는 주문 안 하나를 지금 시세로 주문 길에 넣는다.
async fn buy_now(bench: &Bench) -> (TradeId, Placed) {
    let at = plus(bench.now(), 1);
    bench.quote(HYNIX, ENTRY, at).await;
    let trade = bench.judged(HYNIX);
    let placed = bench
        .place(order(trade, HYNIX, Purpose::NewEntry, ENTRY, at))
        .await;
    (trade, placed)
}

/// 자료 이상으로 이상 감지 멈춤이 된 적이 있는가.
fn anomaly_detected(bench: &Bench) -> bool {
    let history = bench.store().team_history().unwrap();
    history
        .iter()
        .any(|record| record.cause == TeamCause::AnomalyDetected)
}

// 검증: V11 E13 E32 N6 P11
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 자료_키가_없으면_논의를_세_번_해도_멈추지_않고_매수가_자료_이상으로_막히지_않는다() {
    let bench = Bench::live_without_data_keys(kst(monday(), 10, 0)).await;
    bench.ready().await;

    discuss_times(&bench, 3).await;
    // 뒤에서 도는 이상 감지가 몇 바퀴 돌 만큼 기다린다.
    settle().await;
    settle().await;
    assert!(
        !anomaly_detected(&bench),
        "키가 없어 묻지 못한 것은 자료 이상이 아니다(멈추지 않는다)"
    );
    assert_eq!(bench.team(), TeamState::Operating, "운용 중 그대로다");

    let (trade, placed) = buy_now(&bench).await;
    let by_data = matches!(&placed, Placed::Blocked(blocked)
        if matches!(&blocked.reason, Reason::Violations(found)
            if found.contains(&Violation::DataAnomaly)));
    assert!(
        !by_data,
        "매수 점검이 자료 이상으로 막히지 않는다: {placed:?}"
    );
    assert!(
        matches!(placed, Placed::Ordered(_)),
        "근거가 찬 사는 주문은 나간다: {placed:?}"
    );
    assert_eq!(bench.trade_state(trade), TradeState::Ordered);
    bench.close().await;
}

// 검증: V11 E13 N6 P11
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 자료_키가_있는데_세_번_이어서_실패하면_자료_이상으로_멈추고_매수가_막힌다() {
    let bench = Bench::live(kst(monday(), 10, 0)).await;
    bench.ready().await;
    bench.fail_data().await;

    discuss_times(&bench, 3).await;
    wait_for("자료 이상으로 이상 감지 멈춤", || {
        bench.team() == TeamState::AnomalyHalt
    })
    .await;
    assert!(anomaly_detected(&bench), "이상 감지로 멈췄다");

    let (trade, placed) = buy_now(&bench).await;
    assert!(
        matches!(placed, Placed::Blocked(_)),
        "키가 있는 출처가 이어서 실패하면 사는 주문이 막힌다: {placed:?}"
    );
    assert!(
        sent(&bench, Side::Buy).await.is_empty(),
        "사는 주문이 0건이다"
    );
    assert_ne!(bench.trade_state(trade), TradeState::Ordered);
    bench.close().await;
}
