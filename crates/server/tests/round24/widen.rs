//! 21차 1번: 위험을 키우는 템플릿 값 바꾸기는 승인 · 냉각을 거친다(결정 9, R14-1 · E28). 실전중 템플릿에만 적용한다.
//!
//! 준비는 결정적으로 한다: 실전중 템플릿은 월요일 마감 → 승격 승인 → 화요일 장 시작을 각각 조건으로 기다린 뒤
//! 쓴다(시계 · 뒤에서 도는 일이 겹쳐도 준비가 어긋나지 않게, 기다림은 `wait_for`뿐이다).
//! 모의중 템플릿은 지금처럼 바로 반영한다(대조 표 "유지").

use pi_approval::{ApprovalNumber, ApprovalView, Subject};
use pi_rules::Purpose;
use pi_store::{ApprovalStatus, NewTemplate, TemplateId, TemplateState};
use pi_template::{PromotionCriteria, Style, Template};
use price_inha::{
    APPROVAL_ANSWER_PATH, KIND_DAY_CLOSE, KIND_DAY_START, PROPOSAL_ANSWER_PATH, PROPOSALS_PATH,
    TEMPLATE_ADJUST_PATH,
};
use rust_decimal::Decimal;
use serde_json::{Value, json};

use crate::common::{
    Bench, ENTRY, HYNIX, Plan, Reply, d, get, kst, monday, ms, on_date, order, plus, post, settle,
    tuesday, wait_for,
};

/// 매매 1건 · 거래일 1일 · 손익 0 초과만 보는 스윙 템플릿을 저장소에 만들고 오늘 매매하기로 한다.
fn loose_template(bench: &Bench) -> TemplateId {
    let mut template = Template::new(Style::Swing);
    template.params.promotion = PromotionCriteria {
        min_trades: 1,
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
    assert!(
        matches!(placed, price_inha::Placed::Ordered(_)),
        "판다: {placed:?}"
    );
    bench.quote(HYNIX, ENTRY + 5_000, plus(at, 1)).await;
    wait_for("익절이 팔림", || {
        bench.trade_state(trade) == pi_store::TradeState::Sold
    })
    .await;
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

/// 월요일을 마감해 승격대기가 되게 하고, 승격 승인 요청에 승인으로 답해 실전중으로 만든 뒤 화요일 장을 연다.
async fn live_on_tuesday(bench: &Bench, id: TemplateId) {
    bench.set(kst(monday(), 15, 31));
    let store = bench.store();
    wait_for("월요일 마감", || {
        on_date(&store, KIND_DAY_CLOSE, monday()) == 1
    })
    .await;
    wait_for("승격 승인 요청", || {
        promotion_request(bench, id).is_some()
    })
    .await;
    let request = promotion_request(bench, id).unwrap();
    let body = json!({
        "number": request.number.as_str(),
        "fingerprint": request.fingerprint.as_str(),
        "answer": "approve",
    });
    let reply = post(bench.addr(), APPROVAL_ANSWER_PATH, &body).await;
    assert_eq!(reply.body["result"], json!("approved"), "{}", reply.text);
    assert_eq!(template_state(bench, id), TemplateState::Live, "실전중이다");

    settle().await;
    bench.set(kst(tuesday(), 9, 1));
    wait_for("화요일 시작", || {
        on_date(&store, KIND_DAY_START, tuesday()) >= 1
    })
    .await;
    settle().await;
}

fn template_state(bench: &Bench, id: TemplateId) -> TemplateState {
    bench.store().get_template(id).unwrap().state
}

/// 실전중 템플릿 하나가 있는 화요일 장 중의 서버.
async fn live_bench() -> (Bench, TemplateId) {
    let bench = Plan::demo().start(kst(monday(), 10, 0)).await;
    let id = loose_template(&bench);
    profitable_trade(&bench).await;
    live_on_tuesday(&bench, id).await;
    (bench, id)
}

async fn adjust(bench: &Bench, id: TemplateId, changes: Value) -> Reply {
    let body = json!({"id": id.0, "changes": changes});
    post(bench.addr(), TEMPLATE_ADJUST_PATH, &body).await
}

async fn proposals(bench: &Bench) -> Vec<Value> {
    let reply = get(bench.addr(), PROPOSALS_PATH).await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    reply.body["proposals"]
        .as_array()
        .cloned()
        .unwrap_or_default()
}

/// 템플릿의 1회 위험 비율.
fn risk_of(bench: &Bench, id: TemplateId) -> Decimal {
    let saved = bench.store().get_template(id).unwrap();
    Template::from_json(&saved.value.to_string())
        .unwrap()
        .params
        .risk_per_trade_pct
}

// 검증: R14-1 E28 V9
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 실전중_템플릿의_손절_폭을_넓히면_템플릿은_그대로이고_개선안_제안됨이_생긴다() {
    let (bench, id) = live_bench().await;
    let before = bench.store().get_template(id).unwrap();

    let reply = adjust(&bench, id, json!({"risk_per_trade_pct": "1.5"})).await;
    assert_eq!(
        reply.status, 200,
        "승인을 기다리는 것은 오류가 아니다: {}",
        reply.text
    );
    assert_eq!(
        reply.body["state"],
        json!("needs_approval"),
        "{}",
        reply.text
    );
    assert_eq!(
        reply.body["widened"],
        json!(["risk_per_trade_pct"]),
        "{}",
        reply.text
    );
    let number = reply.body["proposal"].as_i64().expect("개선안 번호");

    let after = bench.store().get_template(id).unwrap();
    assert_eq!(after.revision, before.revision, "템플릿의 판이 늘지 않는다");
    assert_eq!(after.value, before.value, "템플릿 값은 그대로다");
    assert_eq!(after.state, TemplateState::Live);

    let list = proposals(&bench).await;
    let mine = list
        .iter()
        .find(|proposal| proposal["id"] == json!(number))
        .unwrap_or_else(|| panic!("개선안 목록에 {number}번이 있다: {list:?}"));
    assert_eq!(mine["state"], json!("proposed"), "제안됨이다: {mine}");
    assert_eq!(mine["increases_risk"], json!(true), "{mine}");
    assert!(
        mine["number"].is_string() && mine["fingerprint"].is_string(),
        "사용자가 답할 승인 요청이 같이 생긴다: {mine}"
    );
    bench.close().await;
}

// 검증: R14-1 E28 V9
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 승인하면_냉각중이고_템플릿은_다음_거래일_시작에_바뀐다() {
    let (bench, id) = live_bench().await;
    let before = bench.store().get_template(id).unwrap();
    let reply = adjust(&bench, id, json!({"risk_per_trade_pct": "1.5"})).await;
    assert_eq!(
        reply.body["state"],
        json!("needs_approval"),
        "{}",
        reply.text
    );
    let number = reply.body["proposal"].as_i64().expect("개선안 번호");
    let list = proposals(&bench).await;
    let mine = list
        .iter()
        .find(|proposal| proposal["id"] == json!(number))
        .expect("제안된 개선안");

    let body = json!({
        "id": number,
        "number": mine["number"],
        "fingerprint": mine["fingerprint"],
        "answer": "approve",
    });
    let answered = post(bench.addr(), PROPOSAL_ANSWER_PATH, &body).await;
    assert_eq!(
        answered.body["result"],
        json!("cooling"),
        "{}",
        answered.text
    );
    let state = |list: &[Value]| {
        list.iter()
            .find(|proposal| proposal["id"] == json!(number))
            .map(|proposal| proposal["state"].clone())
    };
    assert_eq!(state(&proposals(&bench).await), Some(json!("cooling")));

    // 같은 날에는 아직 반영하지 않는다.
    settle().await;
    let same_day = bench.store().get_template(id).unwrap();
    assert_eq!(same_day.revision, before.revision, "냉각 중에는 그대로다");
    assert_eq!(risk_of(&bench, id), d("1"));

    // 다음 거래일(수요일) 시작에 반영된다.
    let wednesday = tuesday().succ_opt().unwrap();
    bench.set(kst(wednesday, 9, 1));
    let store = bench.store();
    wait_for("수요일 시작", || {
        on_date(&store, KIND_DAY_START, wednesday) >= 1
    })
    .await;
    wait_for("다음 거래일 시작에 템플릿이 바뀐다", || {
        risk_of(&bench, id) == d("1.5")
    })
    .await;
    assert_eq!(
        state(&proposals(&bench).await),
        Some(json!("applied")),
        "개선안은 반영됨이다"
    );
    bench.close().await;
}

// 검증: R14-1 E28
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 한_칸이라도_키우면_묶음_전체가_승인을_기다리고_줄이는_칸도_그대로다() {
    let (bench, id) = live_bench().await;
    let before = bench.store().get_template(id).unwrap();
    let changes = json!({"risk_per_trade_pct": "1.5", "daily_loss_limit_pct": "2"});
    let reply = adjust(&bench, id, changes).await;
    assert_eq!(
        reply.body["state"],
        json!("needs_approval"),
        "{}",
        reply.text
    );
    assert_eq!(
        reply.body["widened"],
        json!(["risk_per_trade_pct"]),
        "키운 칸만 알린다: {}",
        reply.text
    );
    let after = bench.store().get_template(id).unwrap();
    assert_eq!(
        after.value, before.value,
        "줄이는 칸도 승인 전에는 바뀌지 않는다"
    );
    bench.close().await;
}

// 검증: R14-1 E28
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 실전중_템플릿의_위험을_줄이는_값은_승인을_거치지_않는다() {
    let (bench, id) = live_bench().await;
    let reply = adjust(&bench, id, json!({"risk_per_trade_pct": "0.5"})).await;
    assert_ne!(
        reply.body["state"],
        json!("needs_approval"),
        "줄이기만이면 승인 대기가 아니다: {}",
        reply.text
    );
    assert!(
        proposals(&bench).await.is_empty(),
        "줄이기는 개선안을 만들지 않는다"
    );
    bench.close().await;
}

// 검증: E5 R14-1
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 실전중_템플릿도_절대_상한을_넘는_값은_개선안이_아니라_거부된다() {
    let (bench, id) = live_bench().await;
    let before = bench.store().get_template(id).unwrap();
    let reply = adjust(&bench, id, json!({"risk_per_trade_pct": "3"})).await;
    assert_eq!(reply.status, 400, "상한 넘음은 거부된다: {}", reply.text);
    assert!(
        proposals(&bench).await.is_empty(),
        "상한을 넘는 값으로는 개선안을 만들지 않는다"
    );
    let after = bench.store().get_template(id).unwrap();
    assert_eq!(after.value, before.value);
    bench.close().await;
}

// 검증: R14-1 E28
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 모의중_템플릿은_위험을_키우는_값도_지금처럼_바로_반영한다() {
    let bench = Plan::demo().start(kst(monday(), 10, 0)).await;
    bench.ready().await;
    let id = bench
        .store()
        .templates_in_state(TemplateState::Simulating)
        .unwrap()[0]
        .id;
    let before = bench.store().get_template(id).unwrap();

    let reply = adjust(&bench, id, json!({"risk_per_trade_pct": "1.5"})).await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    assert_eq!(reply.body["state"], json!("simulating"), "{}", reply.text);
    assert_eq!(
        bench.store().get_template(id).unwrap().revision,
        before.revision + 1,
        "새 판이 바로 생긴다"
    );
    assert_eq!(risk_of(&bench, id), d("1.5"));
    assert!(proposals(&bench).await.is_empty(), "개선안은 생기지 않는다");
    bench.close().await;
}
