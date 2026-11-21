//! 10a 보강(검토 되돌림) 4 · 5 · 6번: 템플릿 쪽.
//!
//! - 4번: 전체 정지로 실전중 템플릿이 멈춤이 돼도 손절 · 청산은 "마지막으로 쓰던 템플릿"의 값으로 나간다
//!   (결정 2026-11-21 12번 · F4). 더 오래된 다른 모의중 템플릿의 값으로 조용히 넘어가지 않는다.
//! - 5번: 승격대기에서 빠져나오지 못하는 길을 닫는다(F6 · V30 · V42 · E36): 전체 정지 중의 승인은 다시 시작한
//!   뒤 실전중이 되고, 쓸 수 있는 승인 요청이 없는 승격대기는 모의중으로 돌아온다.
//! - 6번: 값이 잠긴 템플릿(승격대기 · 실전중)에는 규칙 개선안을 반영하지 않는다(V21 · E28).
//!
//! 승격 기준은 화면에서 바꿀 수 없어 테스트가 저장소에 느슨한 기준의 템플릿을 직접 만든다(가장 나중에 만든
//! 템플릿을 쓴다). 모두 시연 모드다.

use std::time::Duration;

use chrono::{DateTime, Utc};
use pi_approval::{ApprovalNumber, ApprovalView, Subject};
use pi_paper::{Order, OrderGroup, OrderType, Side};
use pi_rules::Purpose;
use pi_store::{
    ApprovalStatus, NewTemplate, ProposalId, ProposalState, TeamCause, TeamChange, TeamState,
    TemplateId, TemplateState, TradeState,
};
use pi_template::{PromotionCriteria, Style, Template, UnfilledCancel};
use price_inha::{
    APPROVAL_ANSWER_PATH, DemoTurn, KIND_DAY_CLOSE, KIND_DAY_START, PROPOSAL_ANSWER_PATH,
    PROPOSALS_PATH, Placed, TEAM_RESTART_PATH,
};
use rust_decimal::Decimal;
use serde_json::{Value, json};

use crate::common::{
    Bench, ENTRY, HYNIX, STOP, d, get, kst, monday, ms, on_date, order, plus, post, settle,
    tuesday, wait_for,
};

/// 스윙 기본값을 `edit`로 고친 템플릿을 저장소에 만든다(모의중).
fn template_with(bench: &Bench, edit: impl FnOnce(&mut Template)) -> TemplateId {
    let mut template = Template::new(Style::Swing);
    edit(&mut template);
    let value: Value = serde_json::from_str(&template.to_json()).unwrap();
    let saved = bench
        .store()
        .create_template(NewTemplate {
            value,
            at: ms(bench.now()),
        })
        .unwrap();
    saved.id
}

/// 매매 1건 · 거래일 1일 · 손익 0 초과만 보는 스윙 템플릿을 저장소에 만들고 오늘 매매하기로 한다.
fn loose_template(bench: &Bench) -> TemplateId {
    let id = template_with(bench, |template| {
        template.params.promotion = PromotionCriteria {
            min_trades: 1,
            min_trading_days: 1,
            net_pnl_above: Decimal::ZERO,
            max_drawdown_pct: d("100"),
            out_of_sample_pnl_above: None,
        };
    });
    bench.trade_today();
    id
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

/// 화요일 장을 연다(09:01). 그 시각을 돌려준다.
async fn open_tuesday(bench: &Bench) -> DateTime<Utc> {
    // 마감 일이 끝난 뒤에 밤을 건넌다.
    settle().await;
    let open = kst(tuesday(), 9, 1);
    bench.set(open);
    let store = bench.store();
    wait_for("화요일 시작", || {
        on_date(&store, KIND_DAY_START, tuesday()) >= 1
    })
    .await;
    settle().await;
    open
}

fn saved(bench: &Bench, id: TemplateId) -> pi_store::Template {
    bench.store().get_template(id).unwrap()
}

fn state(bench: &Bench, id: TemplateId) -> TemplateState {
    saved(bench, id).state
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

/// 월요일을 마감해 그 템플릿이 승격대기가 되고 승인 요청이 생길 때까지 간다.
async fn into_pending(bench: &Bench, id: TemplateId) -> ApprovalView {
    close_monday(bench).await;
    wait_for("승격대기", || {
        state(bench, id) == TemplateState::PromotionPending
    })
    .await;
    wait_for("승격 승인 요청", || {
        promotion_request(bench, id).is_some()
    })
    .await;
    promotion_request(bench, id).unwrap()
}

/// 실전 승격 승인 요청에 승인으로 답하고 답의 본문을 돌려준다.
async fn approve_promotion(bench: &Bench, request: &ApprovalView) -> Value {
    let body = json!({
        "number": request.number.as_str(),
        "fingerprint": request.fingerprint.as_str(),
        "answer": "approve",
    });
    let reply = post(bench.addr(), APPROVAL_ANSWER_PATH, &body).await;
    assert_eq!(
        reply.status, 200,
        "답하기는 결과를 돌려준다: {}",
        reply.body
    );
    reply.body
}

/// 운용 중에서 최대 손실로 전체 정지가 된다.
fn full_stop(bench: &Bench) {
    bench
        .store()
        .change_team_state(TeamChange {
            to: TeamState::FullStop,
            at: ms(bench.now()),
            cause: TeamCause::MaxLoss,
            detail: "시험: 최대 손실 도달".to_string(),
            owner_approved: false,
        })
        .expect("운용 중에서 최대 손실로 전체 정지가 된다");
}

// ───────────── 4번: 멈춘 템플릿으로도 손절 · 청산은 나간다 ─────────────

/// `since`부터 낸 [`HYNIX`] 파는 주문들(걸린 것과 끝난 것).
async fn sells_since(bench: &Bench, since: DateTime<Utc>) -> Vec<Order> {
    let mut orders = bench.orders(OrderGroup::Open).await;
    orders.extend(bench.orders(OrderGroup::Closed).await);
    orders.retain(|order| {
        order.side == Side::Sell && order.symbol.as_str() == HYNIX && order.ordered_at >= since
    });
    orders
}

/// 그 템플릿을 실전중으로 올리고 보유 하나(5주)를 든 채 화요일에 전체 정지가 된다. 그 템플릿은 멈춤이 된다.
/// 보유 매매와 전체 정지가 된 시각을 돌려준다.
async fn stopped_while_holding(
    bench: &Bench,
    id: TemplateId,
) -> (pi_store::TradeId, DateTime<Utc>) {
    profitable_trade(bench).await;
    let held = bench.holding(HYNIX).await;
    let request = into_pending(bench, id).await;
    let result = approve_promotion(bench, &request).await;
    assert_eq!(result["result"], json!("approved"), "{result}");
    assert_eq!(state(bench, id), TemplateState::Live, "실전중이다");

    let open = open_tuesday(bench).await;
    bench.trade_today();
    full_stop(bench);
    assert_eq!(
        state(bench, id),
        TemplateState::Stopped,
        "전체 정지가 되면 실전중 템플릿은 멈춤이 된다"
    );
    assert_eq!(bench.trade_state(held), TradeState::Holding);
    (held, open)
}

// 검증: E24 E34 V9 F4
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 실전중_템플릿이_전체_정지로_멈춰도_손절값에_닿으면_손절이_나간다() {
    let bench = Bench::start(kst(monday(), 10, 0)).await;
    let id = loose_template(&bench);
    let (held, open) = stopped_while_holding(&bench, id).await;

    let at = plus(bench.now(), 1);
    bench.quote(HYNIX, STOP - 1_000, at).await;
    let sent = sells_since(&bench, open).await;
    assert_eq!(
        sent.len(),
        1,
        "쓸 템플릿이 멈춤이어도 손절은 그 값으로 나간다: {sent:?}"
    );
    assert_eq!(sent[0].quantity, d("5"), "보유 수량을 모두 판다");
    assert_eq!(sent[0].order_type, OrderType::Market, "손절은 시장가다");

    bench.quote(HYNIX, STOP - 1_000, plus(at, 1)).await;
    assert_eq!(bench.trade_state(held), TradeState::Sold, "팔린다");
    assert_eq!(bench.team(), TeamState::FullStop, "팀은 그대로 전체 정지다");
    bench.close().await;
}

// 검증: E24 E34 N4 F4
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 다른_모의중_템플릿이_있어도_손절은_마지막으로_쓰던_템플릿의_값으로_나간다() {
    let bench = Bench::start(kst(monday(), 10, 0)).await;
    // 더 오래된 다른 모의중 템플릿: 미체결 취소 시간이 1초다. 쓰던 템플릿(스윙 기본값)은 10분이다.
    let other = template_with(&bench, |template| {
        template.params.unfilled_cancel = UnfilledCancel::After(Duration::from_secs(1));
    });
    let id = loose_template(&bench);
    let (held, open) = stopped_while_holding(&bench, id).await;
    assert_eq!(state(&bench, other), TemplateState::Simulating);

    let at = plus(bench.now(), 1);
    bench.quote(HYNIX, STOP - 1_000, at).await;
    assert_eq!(
        sells_since(&bench, open).await.len(),
        1,
        "손절 주문 하나가 나간다"
    );

    // 시세 없이 5초가 간다. 쓰던 템플릿의 값(10분)이면 주문이 걸려 있고, 다른 템플릿의 값(1초)이면 취소된다.
    bench.set(plus(at, 5));
    settle().await;
    let sent = sells_since(&bench, open).await;
    assert_eq!(sent.len(), 1, "손절을 다시 내지 않았다: {sent:?}");
    assert!(
        sent[0].status.is_open() && sent[0].canceled_at.is_none(),
        "다른 모의중 템플릿의 값으로 조용히 넘어가지 않는다(5초 뒤에도 걸려 있다): {sent:?}"
    );

    bench.quote(HYNIX, STOP - 1_000, plus(at, 6)).await;
    assert_eq!(bench.trade_state(held), TradeState::Sold, "팔린다");
    bench.close().await;
}

// ───────────── 5번: 승격대기에서 빠져나오지 못하는 길 ─────────────

// 검증: V30 V42 E36 E24
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 전체_정지_중에_승인한_승격은_다시_시작한_뒤_실전중이_된다() {
    let bench = Bench::start(kst(monday(), 10, 0)).await;
    let id = loose_template(&bench);
    profitable_trade(&bench).await;
    let request = into_pending(&bench, id).await;

    open_tuesday(&bench).await;
    bench.trade_today();
    full_stop(&bench);
    // 전체 정지 중의 승인: 거절되든(같은 번호로 다시 답한다) 받아 두든(맞춰 본다) 지금은 실전중이 아니다.
    let _during = approve_promotion(&bench, &request).await;
    settle().await;
    assert_ne!(
        state(&bench, id),
        TemplateState::Live,
        "전체 정지 중에는 실전중이 되지 않는다"
    );

    let reply = post(bench.addr(), TEAM_RESTART_PATH, &json!(null)).await;
    assert_eq!(reply.status, 200, "다시 시작: {}", reply.body);
    // 같은 번호로 한 번 더 답한다(이미 받아 둔 승인이면 이 답은 거절로 남고, 맞춰 보기가 올린다).
    let _after = approve_promotion(&bench, &request).await;
    wait_for("다시 시작한 뒤 실전중", || {
        state(&bench, id) == TemplateState::Live
    })
    .await;
    let live = saved(&bench, id);
    assert!(
        live.criteria_pass.is_some() && live.approval.is_some(),
        "기준 통과 기록과 승인 번호가 함께 있다(E36)"
    );
    bench.close().await;
}

/// 스윙 템플릿(기본 승격 기준)을 고른 뒤, 승인 요청을 만들지 않고 저장소에서 바로 승격대기로 옮긴다
/// (승격대기로 옮긴 직후 꺼진 것과 같다).
async fn pending_without_request(bench: &Bench) -> TemplateId {
    bench.ready().await;
    let store = bench.store();
    let chosen = store.templates_in_state(TemplateState::Simulating).unwrap();
    let id = chosen.iter().map(|template| template.id).max().unwrap();
    let record = json!({"note": "시험: 승인 요청 없이 남은 승격대기"});
    store
        .submit_template_for_promotion(id, &record, ms(bench.now()))
        .unwrap();
    assert_eq!(state(bench, id), TemplateState::PromotionPending);
    assert!(promotion_request(bench, id).is_none(), "승인 요청이 없다");
    id
}

// 검증: V30 V42 E36 E26
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 승인_요청이_없는_승격대기는_다시_켜면_모의중으로_돌아온다() {
    let bench = Bench::start(kst(monday(), 10, 0)).await;
    let id = pending_without_request(&bench).await;

    let bench = bench.reopen(kst(monday(), 10, 5)).await;
    wait_for("모의중으로 되돌림", || {
        state(&bench, id) == TemplateState::Simulating
    })
    .await;
    assert!(
        saved(&bench, id).criteria_pass.is_none(),
        "기준 통과 기록은 지운다(다음 마감에 다시 잰다)"
    );
    bench.close().await;
}

// 검증: V30 V42 E36 E26
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 승인_요청이_없는_승격대기는_하루_마감을_지나면_모의중으로_돌아온다() {
    let bench = Bench::start(kst(monday(), 10, 0)).await;
    let id = pending_without_request(&bench).await;

    close_monday(&bench).await;
    wait_for("모의중으로 되돌림", || {
        state(&bench, id) == TemplateState::Simulating
    })
    .await;
    // 되돌린 뒤 같은 마감에서 다시 재도 기본 기준(매매 없음)에는 못 미친다: 승인 요청이 생기지 않는다.
    settle().await;
    assert_eq!(state(&bench, id), TemplateState::Simulating);
    assert!(promotion_request(&bench, id).is_none());
    bench.close().await;
}

// ───────────── 6번: 잠긴 템플릿에는 개선안을 반영하지 않는다 ─────────────

/// 익절로 판 매매 둘을 같은 실수로 복기해, 규칙을 `change`로 바꾸자는 개선안 하나를 낸다.
async fn propose(bench: &Bench, change: Value) -> ProposalId {
    profitable_trade(bench).await;
    profitable_trade(bench).await;
    let reply = json!({"mistakes": ["late_stop"], "note": "복기", "change": change});
    bench.script(DemoTurn::Review, &reply.to_string());
    let reviewed = bench.crew().review_day().await.expect("복기한다");
    assert_eq!(reviewed.proposals.len(), 1, "같은 실수 2건이면 개선안 하나");
    reviewed.proposals[0]
}

/// 개선안에 승인으로 답한다. 응답과, 그 개선안의 승인 번호를 돌려준다.
async fn approve_proposal(bench: &Bench, id: ProposalId) -> (crate::common::Reply, ApprovalNumber) {
    let list = get(bench.addr(), PROPOSALS_PATH).await;
    assert_eq!(list.status, 200, "{}", list.body);
    let found = list.body["proposals"]
        .as_array()
        .unwrap()
        .iter()
        .find(|proposal| proposal["id"] == json!(id.0))
        .expect("목록에 있다")
        .clone();
    let number = found["number"].as_str().expect("승인 번호").to_string();
    let body = json!({
        "id": id.0, "number": number, "fingerprint": found["fingerprint"], "answer": "approve",
    });
    let reply = post(bench.addr(), PROPOSAL_ANSWER_PATH, &body).await;
    (reply, ApprovalNumber::new(number))
}

fn proposal_state(bench: &Bench, id: ProposalId) -> ProposalState {
    bench.store().get_proposal(id).unwrap().state
}

// 검증: V21 E28 E35 V30
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 승격대기_중에_개선안을_승인하면_잠겼다고_거절하고_아무것도_바꾸지_않는다() {
    let bench = Bench::start(kst(monday(), 10, 0)).await;
    let id = loose_template(&bench);
    // 위험을 줄이는 개선안이다(잠기지 않았다면 승인하는 그 자리에서 반영된다).
    let proposal = propose(&bench, json!({"risk_per_trade_pct": "0.5"})).await;
    into_pending(&bench, id).await;
    let before = saved(&bench, id);

    let (reply, number) = approve_proposal(&bench, proposal).await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert_eq!(reply.body["result"], json!("rejected"), "{}", reply.body);
    assert_eq!(
        reply.body["reason"],
        json!("template_locked"),
        "{}",
        reply.body
    );
    let view = bench.approvals().get(&number).unwrap().expect("승인 요청");
    assert_eq!(
        view.status,
        ApprovalStatus::Pending,
        "승인 번호는 그대로 기다린다(승인을 소비하기 전에 거절한다)"
    );
    assert!(!view.consumed, "승인을 소비하지 않았다");
    assert_eq!(
        proposal_state(&bench, proposal),
        ProposalState::Proposed,
        "개선안 상태 그대로다"
    );
    let after = saved(&bench, id);
    assert_eq!(after.state, TemplateState::PromotionPending);
    assert_eq!(
        (after.revision, &after.value),
        (before.revision, &before.value),
        "쓰는 템플릿의 값 그대로다"
    );
    bench.close().await;
}

// 검증: V21 E28 E35 V5
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 냉각이_끝난_개선안은_템플릿이_잠겨_있으면_건너뛰고_하루_판단이_실패하지_않는다() {
    let bench = Bench::start(kst(monday(), 10, 0)).await;
    let id = loose_template(&bench);
    // 위험을 키우는 개선안: 승인하면 냉각중이 되고 다음 거래일 시작 뒤에 반영된다(E28).
    let proposal = propose(&bench, json!({"risk_per_trade_pct": "1.5"})).await;
    let (reply, _number) = approve_proposal(&bench, proposal).await;
    assert_eq!(reply.body["result"], json!("cooling"), "{}", reply.body);
    // 그 뒤 마감에서 기준을 넘겨 템플릿이 승격대기(값이 잠김)가 된다.
    into_pending(&bench, id).await;
    let before = saved(&bench, id);

    open_tuesday(&bench).await;
    let plan = bench.crew().plan_day().await;
    assert!(
        plan.is_ok(),
        "잠긴 템플릿 때문에 하루 판단이 실패하지 않는다: {plan:?}"
    );
    assert_eq!(
        proposal_state(&bench, proposal),
        ProposalState::Cooling,
        "반영을 건너뛰고 냉각중으로 둔다"
    );
    let after = saved(&bench, id);
    assert_eq!(after.state, TemplateState::PromotionPending);
    assert_eq!(
        (after.revision, &after.value),
        (before.revision, &before.value),
        "잠긴 템플릿의 값 그대로다"
    );
    bench.close().await;
}
