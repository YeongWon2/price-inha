//! 7번 승격(결정 2026-11-21 6번, F6 5~8): 장 마감 뒤 모의 결과를 템플릿의 승격 기준에 재고, 넘으면 승격대기와
//! 승인 요청을 만든다. 내가 승인해야 실전중이 되고(V30 · V42 · E36), 실전 주문은 만들지 않으므로 실전중
//! 템플릿의 주문은 나가지 않는다(E25 · V29).
//!
//! 승격 기준은 화면에서 바꿀 수 없어 테스트가 저장소에 느슨한 기준의 템플릿을 직접 만든다(가장 나중에 만든
//! 모의중 템플릿을 쓴다). 승인 답은 주문 승인과 같은 경로([`price_inha::APPROVAL_ANSWER_PATH`])로 보낸다.

use pi_approval::{ApprovalNumber, ApprovalView, Subject};
use pi_paper::{OrderGroup, Side};
use pi_rules::Purpose;
use pi_store::{ApprovalStatus, NewTemplate, TemplateId, TemplateState, TradeState};
use pi_template::{PromotionCriteria, Style, Template};
use price_inha::{APPROVAL_ANSWER_PATH, KIND_DAY_CLOSE, KIND_DAY_START, Placed};
use rust_decimal::Decimal;
use serde_json::{Value, json};

use crate::common::{
    Bench, ENTRY, HYNIX, d, kst, monday, ms, on_date, order, plus, post, settle, tuesday, wait_for,
};

/// 매매 `min_trades`건 · 거래일 1일 · 손익 0 초과만 보는 스윙 템플릿을 저장소에 만들고 오늘 매매하기로 한다.
fn loose_template(bench: &Bench, min_trades: u32) -> TemplateId {
    let mut template = Template::new(Style::Swing);
    template.params.promotion = PromotionCriteria {
        min_trades,
        min_trading_days: 1,
        net_pnl_above: Decimal::ZERO,
        max_drawdown_pct: d("100"),
        out_of_sample_pnl_above: None,
    };
    let value: Value = serde_json::from_str(&template.to_json()).unwrap();
    let saved = bench
        .store()
        .create_template(NewTemplate {
            value,
            at: ms(bench.now()),
        })
        .unwrap();
    bench.trade_today();
    saved.id
}

/// 5주를 10만 원에 사서 10만 5천 원에 익절로 판다(수수료 · 세금을 빼도 손익이 0보다 크다).
async fn profitable_trade(bench: &Bench) {
    let trade = bench.holding(HYNIX).await;
    let at = plus(bench.now(), 1);
    bench.quote(HYNIX, ENTRY + 5_000, at).await;
    let placed = bench
        .place(order(trade, HYNIX, Purpose::TakeProfit, ENTRY + 5_000, at))
        .await;
    assert!(matches!(placed, Placed::Ordered(_)), "판다: {placed:?}");
    bench.quote(HYNIX, ENTRY + 5_000, plus(at, 1)).await;
    assert_eq!(bench.trade_state(trade), TradeState::Sold);
}

/// 월요일 장을 마감한다.
async fn close_monday(bench: &Bench) {
    bench.set(kst(monday(), 15, 31));
    let store = bench.store();
    wait_for("월요일 마감", || {
        on_date(&store, KIND_DAY_CLOSE, monday()) == 1
    })
    .await;
}

fn state(bench: &Bench, id: TemplateId) -> TemplateState {
    bench.store().get_template(id).unwrap().state
}

/// 그 템플릿의 실전 승격을 묻는, 기다리는 승인 요청.
fn promotion_request(bench: &Bench, id: TemplateId) -> Option<ApprovalView> {
    let approvals = bench.approvals();
    bench
        .store()
        .approvals_in_status(ApprovalStatus::Pending)
        .unwrap()
        .iter()
        .filter_map(|row| row.summary["number"].as_str())
        .filter_map(|number| approvals.get(&ApprovalNumber::new(number)).unwrap())
        .find(|view| matches!(view.subject, Subject::LivePromotion { template, .. } if template == id))
}

/// 기준을 넘겨 승격대기가 되고 승인 요청이 생길 때까지 간다.
async fn pending(bench: &Bench) -> (TemplateId, ApprovalView) {
    let id = loose_template(bench, 1);
    profitable_trade(bench).await;
    close_monday(bench).await;
    wait_for("승격대기", || {
        state(bench, id) == TemplateState::PromotionPending
    })
    .await;
    wait_for("승격 승인 요청", || {
        promotion_request(bench, id).is_some()
    })
    .await;
    (id, promotion_request(bench, id).unwrap())
}

async fn answer(bench: &Bench, number: &str, fingerprint: &str, answer: &str) -> Value {
    let body = json!({"number": number, "fingerprint": fingerprint, "answer": answer});
    let reply = post(bench.addr(), APPROVAL_ANSWER_PATH, &body).await;
    assert_eq!(
        reply.status, 200,
        "답하기는 결과를 돌려준다: {}",
        reply.body
    );
    reply.body
}

// 검증: V29 V30 V42 E25 E36
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 기준을_넘기면_승격대기가_되고_내가_승인해야_실전중이며_실전_주문은_나가지_않는다() {
    let bench = Bench::start(kst(monday(), 10, 0)).await;
    let (id, request) = pending(&bench).await;
    let waiting = bench.store().get_template(id).unwrap();
    assert!(waiting.criteria_pass.is_some(), "기준 통과 기록이 남는다");
    assert_eq!(
        request.subject,
        Subject::LivePromotion {
            template: id,
            revision: waiting.revision
        },
        "승인받을 것은 그 템플릿의 지금 판이다"
    );

    // 승인 전에는 며칠이 지나도 실전중이 되지 않는다(V42: 실전 승격은 답을 계속 기다린다).
    settle().await;
    assert_eq!(state(&bench, id), TemplateState::PromotionPending);
    assert_eq!(request.wait_until, None, "기다릴 시간이 없다");

    let result = answer(
        &bench,
        request.number.as_str(),
        request.fingerprint.as_str(),
        "approve",
    )
    .await;
    assert_eq!(result["result"], json!("approved"), "{result}");
    let live = bench.store().get_template(id).unwrap();
    assert_eq!(live.state, TemplateState::Live, "승인하면 실전중이다");
    assert!(
        live.criteria_pass.is_some() && live.approval.is_some(),
        "기준 통과 기록과 승인 번호가 함께 있다(E36)"
    );

    // 다음 날: 실전중 템플릿의 주문은 모의계좌로도 실제 증권사로도 나가지 않는다(E25).
    let open = kst(tuesday(), 9, 1);
    bench.set(open);
    let store = bench.store();
    wait_for("화요일 시작", || {
        on_date(&store, KIND_DAY_START, tuesday()) >= 1
    })
    .await;
    settle().await;
    let at = plus(open, 1);
    bench.quote(HYNIX, ENTRY, at).await;
    let trade = bench.judged(HYNIX);
    let buys = |orders: Vec<pi_paper::Order>| {
        orders
            .into_iter()
            .filter(|o| o.side == Side::Buy && o.symbol.as_str() == HYNIX)
            .count()
    };
    let before =
        buys(bench.orders(OrderGroup::Open).await) + buys(bench.orders(OrderGroup::Closed).await);
    let placed = bench
        .trading()
        .place(order(trade, HYNIX, Purpose::NewEntry, ENTRY, at))
        .await;
    assert!(
        !matches!(placed, Ok(Placed::Ordered(_))),
        "실전 주문은 내지 않는다: {placed:?}"
    );
    let after =
        buys(bench.orders(OrderGroup::Open).await) + buys(bench.orders(OrderGroup::Closed).await);
    assert_eq!(after, before, "주문이 하나도 늘지 않는다");
    assert_ne!(bench.trade_state(trade), TradeState::Ordered);
    bench.close().await;
}

// 검증: V30 V42 E36
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 지문이_다른_승인이나_거절로는_실전중이_되지_않는다() {
    let bench = Bench::start(kst(monday(), 10, 0)).await;
    let (id, request) = pending(&bench).await;

    let wrong = "0".repeat(64);
    let result = answer(&bench, request.number.as_str(), &wrong, "approve").await;
    assert_eq!(result["result"], json!("rejected"), "{result}");
    assert_ne!(state(&bench, id), TemplateState::Live, "바뀌지 않는다");

    // 같은 번호로 다시 승인해도(이미 처리된 번호) 실전중이 되지 않는다.
    let again = answer(
        &bench,
        request.number.as_str(),
        request.fingerprint.as_str(),
        "approve",
    )
    .await;
    assert_eq!(again["result"], json!("rejected"), "{again}");
    settle().await;
    assert_ne!(state(&bench, id), TemplateState::Live);
    let live = bench
        .store()
        .templates_in_state(TemplateState::Live)
        .unwrap();
    assert!(live.is_empty(), "실전중 템플릿이 없다");
    bench.close().await;
}

// 검증: V30 E26
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 기준에_못_미치면_승격대기가_되지_않는다() {
    let bench = Bench::start(kst(monday(), 10, 0)).await;
    // 매매 2건이 기준인데 1건뿐이다.
    let id = loose_template(&bench, 2);
    profitable_trade(&bench).await;
    close_monday(&bench).await;
    settle().await;
    settle().await;
    assert_eq!(
        state(&bench, id),
        TemplateState::Simulating,
        "모의중 그대로다"
    );
    assert!(
        promotion_request(&bench, id).is_none(),
        "승인 요청도 만들지 않는다"
    );
    bench.close().await;
}
