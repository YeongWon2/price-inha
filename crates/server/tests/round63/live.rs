//! 실전중 템플릿의 사는 주문은 실전 계좌의 것이라 실행기가 막는다(E25 — 실전 주문은 만들지 않는다). 그때
//! 주문은 나가지 않고, 그 매매는 판단됨에 남지 않고 막힘이 되며 "실전중 템플릿이라 내지 않았다"는 까닭이 남는다
//! (E25 "보이는 것: 막힌 이유가 보인다").

use pi_approval::{Ask, Subject};
use pi_paper::{OrderGroup, Side};
use pi_rules::Purpose;
use pi_store::{NewTemplate, TemplateId, TemplateState, TradeState};
use pi_template::{Style, Template};
use price_inha::{APPROVAL_ANSWER_PATH, Placed};
use serde_json::{Value, json};

use crate::promoting::{Bench, ENTRY, HYNIX, d, kst, monday, ms, order, plus, post};

/// 종목 비중 10%의 스윙 템플릿을 저장소에 만들고 승격대기 → 지금 승인 길로 승인해 실전중으로 만든다.
async fn live_template(bench: &Bench) -> TemplateId {
    let store = bench.store();
    let mut template = Template::new(Style::Swing);
    template.params.symbol_weight_pct = d("10");
    let value: Value = serde_json::from_str(&template.to_json()).unwrap();
    let at = ms(bench.now());
    let saved = store.create_template(NewTemplate { value, at }).unwrap();
    let pending = store
        .submit_template_for_promotion(saved.id, &json!({ "passed": true }), at)
        .unwrap();
    let requested = bench
        .approvals()
        .request(Ask {
            subject: Subject::LivePromotion {
                template: pending.id,
                revision: pending.revision,
            },
            reason: "시험: 승격 기준을 모두 넘었다".to_string(),
        })
        .expect("실전 승격 승인 요청");
    let body = json!({
        "number": requested.number.as_str(),
        "fingerprint": requested.fingerprint.as_str(),
        "answer": "approve",
    });
    let reply = post(bench.addr(), APPROVAL_ANSWER_PATH, &body).await;
    assert_eq!(reply.body["result"], json!("approved"), "{}", reply.body);
    let live = store.get_template(pending.id).unwrap();
    assert_eq!(live.state, TemplateState::Live, "실전중이다");
    pending.id
}

// 검증: E25 V29
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 실전중_템플릿의_사는_주문이_막히면_매매가_막힘이_되고_까닭이_남는다() {
    let bench = Bench::start(kst(monday(), 10, 0)).await;
    bench.ready().await;
    live_template(&bench).await;

    let at = plus(bench.now(), 1);
    bench.set(at);
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

    let stored = bench.store().get_trade(trade).unwrap();
    assert_eq!(stored.state, TradeState::Blocked, "판단됨에 남지 않는다");
    let reason = stored.blocked.expect("막힌 까닭").detail;
    assert!(
        reason.contains("실전중"),
        "까닭에 실전중 템플릿이 보인다: {reason}"
    );
    bench.close().await;
}
