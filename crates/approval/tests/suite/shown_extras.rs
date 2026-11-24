//! 5차: 승인 카드의 보이는 내용(summary)에 투표 수 · 한도 적용 전 → 후를 싣는다.
//! 두 칸은 보여 주기용이라 지문 · 승인 · 소비 규칙은 그대로다(V55).

use crate::common::{T0, ask, buy, env, stop_loss, take_profit};
use pi_approval::{ClampNote, Decision, Error, Requested, ShownExtras, fingerprint};
use pi_store::{
    AccountKind, ApprovalStatus, Conviction, Judgment, NewTrade, Rationale, RoleOpinion, RoleScore,
    SourceRef, Stance, Store, TradeId, TradeSide, TradeSummary, UnixMillis,
};
use rust_decimal::Decimal;
use serde_json::{Value, json};

// ───────────── 도우미 ─────────────

fn new_trade(store: &Store) -> TradeId {
    store
        .create_trade(NewTrade {
            symbol: "005930".to_string(),
            account: AccountKind::Paper,
            at: UnixMillis(T0 - 100),
        })
        .unwrap()
        .id
}

fn opinion(role: &str, stance: Option<Stance>) -> RoleOpinion {
    RoleOpinion {
        role: role.to_string(),
        stance,
        conviction: stance.map(|_| Conviction::Medium),
        reason: "시험 의견".to_string(),
        horizon_days: Some(5),
        abstained: stance.is_none(),
    }
}

/// 담당마다 의견 하나씩 달아 판단된 매매. `None`은 기권이다.
fn opined_trade(store: &Store, stances: &[Option<Stance>]) -> TradeId {
    let id = new_trade(store);
    let roles: Vec<String> = (0..stances.len()).map(|i| format!("담당{i}")).collect();
    let judgment = Judgment {
        decided_at: UnixMillis(T0 - 50),
        rationale: Rationale::default(),
        sources: vec![SourceRef {
            name: "시세".to_string(),
            known_at: UnixMillis(T0 - 60),
        }],
        scores: roles
            .iter()
            .map(|role| RoleScore {
                role: role.clone(),
                score: 10,
                reason: "시험 근거".to_string(),
            })
            .collect(),
        consensus_score: 60,
    };
    let opinions: Vec<RoleOpinion> = roles
        .iter()
        .zip(stances)
        .map(|(role, stance)| opinion(role, *stance))
        .collect();
    store
        .judge_trade_with_opinions(
            id,
            judgment,
            TradeSide::Buy,
            &opinions,
            &TradeSummary {
                summary: "시험 요약".to_string(),
                reversal: "시험 뒤집힘".to_string(),
            },
        )
        .unwrap();
    id
}

fn clamp(limit: &str, before: i64, after: i64) -> ClampNote {
    ClampNote {
        limit: limit.to_string(),
        before: Decimal::new(before, 0),
        after: Decimal::new(after, 0),
    }
}

fn extras(trade: Option<TradeId>, clamps: Vec<ClampNote>) -> ShownExtras {
    ShownExtras { trade, clamps }
}

/// 저장소에 남은 요청 summary. 기다리는 요청이 이 하나뿐인 시험에서 쓴다.
fn stored_summary(store: &Store) -> Value {
    let pending = store.approvals_in_status(ApprovalStatus::Pending).unwrap();
    assert_eq!(pending.len(), 1);
    pending[0].summary.clone()
}

fn votes(bull: u64, bear: u64, neutral: u64, abstain: u64) -> Value {
    json!({"bull": bull, "bear": bear, "neutral": neutral, "abstain": abstain})
}

// ───────────── 투표 수 ─────────────

// 검증: R4-4 C24 C12
#[test]
fn votes_count_each_stance_and_abstain() {
    let e = env();
    let trade = opined_trade(
        &e.store,
        &[
            Some(Stance::Bullish),
            Some(Stance::Bullish),
            Some(Stance::Bearish),
            Some(Stance::Neutral),
        ],
    );
    e.approvals
        .request_with_extras(ask(buy("sig-1")), extras(Some(trade), vec![]))
        .unwrap();
    assert_eq!(stored_summary(&e.store)["votes"], votes(2, 1, 1, 0));
}

// 검증: R4-4 C24
#[test]
fn abstained_opinions_are_counted_apart_from_neutral() {
    let e = env();
    let trade = opined_trade(
        &e.store,
        &[
            Some(Stance::Bullish),
            None,
            None,
            Some(Stance::Neutral),
            Some(Stance::Bearish),
        ],
    );
    e.approvals
        .request_with_extras(ask(buy("sig-1")), extras(Some(trade), vec![]))
        .unwrap();
    assert_eq!(stored_summary(&e.store)["votes"], votes(1, 1, 1, 2));
}

// 검증: R4-4 C24
#[test]
fn votes_are_shown_on_sell_orders_too() {
    let e = env();
    let trade = opined_trade(&e.store, &[Some(Stance::Bearish), Some(Stance::Bearish)]);
    e.approvals
        .request_with_extras(ask(stop_loss("sig-1")), extras(Some(trade), vec![]))
        .unwrap();
    assert_eq!(stored_summary(&e.store)["votes"], votes(0, 2, 0, 0));
}

// 검증: R4-4 C24
#[test]
fn trade_without_opinion_records_has_no_votes_field() {
    // 의견이 생기기 전의 옛 매매(논의중 그대로, 의견 기록 없음).
    let e = env();
    let trade = new_trade(&e.store);
    e.approvals
        .request_with_extras(ask(buy("sig-1")), extras(Some(trade), vec![]))
        .unwrap();
    let summary = stored_summary(&e.store);
    assert!(summary.get("votes").is_none(), "{summary}");
}

// 검증: R4-4 C24
#[test]
fn no_trade_given_means_no_votes_field() {
    let e = env();
    e.approvals
        .request_with_extras(ask(buy("sig-1")), extras(None, vec![]))
        .unwrap();
    let summary = stored_summary(&e.store);
    assert!(summary.get("votes").is_none(), "{summary}");
}

// 검증: R4-4 C24 E8
#[test]
fn unknown_trade_is_a_store_error_and_makes_no_request() {
    let e = env();
    let result = e
        .approvals
        .request_with_extras(ask(buy("sig-1")), extras(Some(TradeId(987_654)), vec![]));
    assert!(matches!(result, Err(Error::Store(_))), "{result:?}");
    assert!(
        e.store
            .approvals_in_status(ApprovalStatus::Pending)
            .unwrap()
            .is_empty()
    );
}

// ───────────── 한도 적용 전 → 후 ─────────────

// 검증: R4-4 C24
#[test]
fn one_clamp_is_shown_with_before_and_after() {
    let e = env();
    e.approvals
        .request_with_extras(
            ask(buy("sig-1")),
            extras(None, vec![clamp("max_order_amount", 5_000_000, 3_000_000)]),
        )
        .unwrap();
    assert_eq!(
        stored_summary(&e.store)["clamps"],
        json!([{"limit": "max_order_amount", "before": "5000000", "after": "3000000"}])
    );
}

// 검증: R4-4 C24
#[test]
fn two_clamps_keep_the_order_they_were_applied() {
    let e = env();
    e.approvals
        .request_with_extras(
            ask(buy("sig-1")),
            extras(
                None,
                vec![
                    clamp("max_order_amount", 5_000_000, 3_000_000),
                    clamp("max_position_ratio", 3_000_000, 2_000_000),
                ],
            ),
        )
        .unwrap();
    assert_eq!(
        stored_summary(&e.store)["clamps"],
        json!([
            {"limit": "max_order_amount", "before": "5000000", "after": "3000000"},
            {"limit": "max_position_ratio", "before": "3000000", "after": "2000000"},
        ])
    );
}

// 검증: R4-4 C24
#[test]
fn empty_clamp_list_has_no_clamps_field() {
    let e = env();
    e.approvals
        .request_with_extras(ask(buy("sig-1")), extras(None, vec![]))
        .unwrap();
    let summary = stored_summary(&e.store);
    assert!(summary.get("clamps").is_none(), "{summary}");
}

// 검증: R4-4 C24
#[test]
fn votes_and_clamps_can_be_shown_together() {
    let e = env();
    let trade = opined_trade(&e.store, &[Some(Stance::Bullish), None]);
    e.approvals
        .request_with_extras(
            ask(buy("sig-1")),
            extras(Some(trade), vec![clamp("max_order_amount", 900, 700)]),
        )
        .unwrap();
    let summary = stored_summary(&e.store);
    assert_eq!(summary["votes"], votes(1, 0, 0, 1));
    assert_eq!(
        summary["clamps"],
        json!([{"limit": "max_order_amount", "before": "900", "after": "700"}])
    );
}

// ───────────── 지문은 그대로 (V55) ─────────────

fn plain_and_extra(signal: &str) -> (Requested, Requested, Value) {
    // 같은 주문을 두 길로 만든다. 한 쪽은 기존 길, 한 쪽은 두 칸을 실은 새 길.
    let plain_env = env();
    let plain = plain_env.approvals.request(ask(buy(signal))).unwrap();

    let e = env();
    let trade = opined_trade(&e.store, &[Some(Stance::Bullish), Some(Stance::Bearish)]);
    let shown = e
        .approvals
        .request_with_extras(
            ask(buy(signal)),
            extras(
                Some(trade),
                vec![clamp("max_order_amount", 5_000_000, 3_000_000)],
            ),
        )
        .unwrap();
    let summary = stored_summary(&e.store);
    (plain, shown, summary)
}

// 검증: V55 C12 V44
#[test]
fn fingerprint_is_the_same_with_and_without_the_two_fields() {
    let (plain, shown, summary) = plain_and_extra("sig-1");
    assert!(summary.get("votes").is_some() && summary.get("clamps").is_some());
    assert_eq!(plain.fingerprint, shown.fingerprint);
    assert_eq!(plain.short_code, shown.short_code);
    assert_eq!(shown.fingerprint, fingerprint(&buy("sig-1")));
}

// 검증: V55 C12
#[test]
fn stored_fingerprint_is_the_order_fingerprint() {
    let e = env();
    let trade = opined_trade(&e.store, &[Some(Stance::Bullish)]);
    e.approvals
        .request_with_extras(
            ask(buy("sig-1")),
            extras(
                Some(trade),
                vec![clamp("max_order_amount", 5_000_000, 3_000_000)],
            ),
        )
        .unwrap();
    let pending = e
        .store
        .approvals_in_status(ApprovalStatus::Pending)
        .unwrap();
    assert_eq!(pending[0].fingerprint, fingerprint(&buy("sig-1")).as_str());
}

// 검증: V55 V44 V45 C7
#[test]
fn approving_and_consuming_work_with_the_order_fingerprint() {
    let e = env();
    let subject = take_profit("sig-1");
    let trade = opined_trade(&e.store, &[Some(Stance::Bullish), Some(Stance::Neutral)]);
    let r = e
        .approvals
        .request_with_extras(
            ask(subject.clone()),
            extras(Some(trade), vec![clamp("max_order_amount", 800, 600)]),
        )
        .unwrap();

    // 화면에는 같은 주문의 지문이 보인다. 그 지문으로 승인하고 한 번 소비한다.
    let answer = e
        .approvals
        .answer(&r.number, fingerprint(&subject).as_str(), Decision::Approve)
        .unwrap();
    assert!(
        matches!(answer, pi_approval::Answer::Approved),
        "{answer:?}"
    );
    e.approvals.consume(&r.number, &subject).unwrap();
    assert!(e.approvals.get(&r.number).unwrap().unwrap().consumed);

    // 두 번째 소비는 막힌다(소비 규칙 그대로).
    assert!(e.approvals.consume(&r.number, &subject).is_err());
}

// 검증: V55 V44
#[test]
fn a_different_order_still_does_not_consume_the_approval() {
    let e = env();
    let subject = buy("sig-1");
    let r = e
        .approvals
        .request_with_extras(
            ask(subject.clone()),
            extras(None, vec![clamp("max_order_amount", 800, 600)]),
        )
        .unwrap();
    e.approvals
        .answer(&r.number, fingerprint(&subject).as_str(), Decision::Approve)
        .unwrap();
    assert!(e.approvals.consume(&r.number, &buy("sig-2")).is_err());
}

// ───────────── 기존 길은 그대로 ─────────────

// 검증: V55 C12
#[test]
fn plain_request_summary_has_neither_field() {
    let e = env();
    e.approvals.request(ask(buy("sig-1"))).unwrap();
    let summary = stored_summary(&e.store);
    assert!(summary.get("votes").is_none(), "{summary}");
    assert!(summary.get("clamps").is_none(), "{summary}");
}

// 검증: V55 C12
#[test]
fn request_with_empty_extras_has_the_same_summary_as_the_plain_request() {
    // 번호는 요청마다 무작위라 그것만 맞춰 놓고 나머지 칸이 글자 그대로 같은지 본다.
    let plain_env = env();
    plain_env.approvals.request(ask(buy("sig-1"))).unwrap();
    let mut plain = stored_summary(&plain_env.store);

    let e = env();
    e.approvals
        .request_with_extras(ask(buy("sig-1")), ShownExtras::default())
        .unwrap();
    let mut with_extras = stored_summary(&e.store);

    plain["number"] = json!("n");
    with_extras["number"] = json!("n");
    assert_eq!(plain, with_extras);
}

// 검증: V55 C12 N2
#[test]
fn wait_and_expiry_times_are_the_same_as_the_plain_request() {
    let plain = env().approvals.request(ask(buy("sig-1"))).unwrap();
    let e = env();
    let trade = opined_trade(&e.store, &[Some(Stance::Bullish)]);
    let shown = e
        .approvals
        .request_with_extras(
            ask(buy("sig-1")),
            extras(Some(trade), vec![clamp("max_order_amount", 5, 3)]),
        )
        .unwrap();
    assert_eq!(plain.wait_until, shown.wait_until);
    assert_eq!(plain.expires_at, shown.expires_at);
    assert_eq!(shown.repeat, 1);
}

// 검증: C12 E8
#[test]
fn market_orders_are_still_refused_on_the_new_path() {
    let e = env();
    let mut order = crate::common::order(
        "sig-1",
        pi_approval::Side::Buy,
        pi_approval::SignalKind::Buy,
    );
    order.order_type = pi_approval::OrderType::Market;
    let result = e.approvals.request_with_extras(
        ask(pi_approval::Subject::Order(order)),
        extras(None, vec![clamp("max_order_amount", 5, 3)]),
    );
    assert!(
        matches!(result, Err(Error::LimitOrderRequired)),
        "{result:?}"
    );
}
