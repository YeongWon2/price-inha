//! store 12차 API 쓰기: 위험 표시는 같은 개선안에 올리고, 하루 정리는 그날의 중간 상태 바뀜도 본다.

use pi_journal::{Day, DayFacts, Error, Mistake};
use pi_store::{ApprovalUse, Proposal, ProposalState, StoreError, TradeId, TradeState};
use pi_template::{Change, RiskParams};
use rust_decimal::Decimal;

use crate::common::{DAY, Env, MIN, T0, base_params, t};

/// 같은 실수 복기 두 번 뒤, 일일 손실 한도를 낮추는 개선안(안전)과
/// 다시 원래 한도로 되돌리는 개선안 둘을 낸다. 둘 다 제안 때는 위험을 키우지 않는다.
fn two_safe(env: &Env, current: &RiskParams) -> (Proposal, Proposal) {
    let a = env.sold("005930", T0);
    let b = env.sold("000660", T0);
    for (id, at) in [(a, T0 + MIN), (b, T0 + 2 * MIN)] {
        env.journal
            .review_trade(id, vec![Mistake::LateStop], "", t(at))
            .unwrap();
    }
    let propose = |change: Change, at: i64| {
        env.journal
            .propose(&Mistake::LateStop, current, &[change], t(at))
            .unwrap()
            .unwrap()
    };
    let lower = current.daily_loss_limit_pct - Decimal::new(5, 1);
    let first = propose(Change::DailyLossLimitPct(lower), T0 + 3 * MIN);
    let second = propose(
        Change::SymbolWeightPct(current.symbol_weight_pct - Decimal::ONE),
        T0 + 4 * MIN,
    );
    (first, second)
}

/// 개선안 수(모든 상태).
fn proposal_count(env: &Env) -> usize {
    ProposalState::ALL
        .into_iter()
        .map(|state| env.store.proposals_in_state(state).unwrap().len())
        .sum()
}

// 검증: E28 E35 M4 (지금 규칙보다 위험해진 개선안은 같은 개선안에 위험 표시를 올려 냉각한다)
#[test]
fn 위험해진_개선안은_같은_번호로_냉각한다() {
    let env = Env::new();
    let current = base_params();
    let (first, second) = two_safe(&env, &current);
    let now = env
        .journal
        .approve(
            first.id,
            Some(env.approved("fp-1", T0)),
            &current,
            t(T0 + 5 * MIN),
        )
        .unwrap()
        .rules;
    let out = env
        .journal
        .approve(
            second.id,
            Some(env.approved("fp-2", T0)),
            &now,
            t(T0 + 6 * MIN),
        )
        .unwrap();
    assert_eq!(out.proposal.id, second.id);
    assert_eq!(out.proposal.state, ProposalState::Cooling);
    assert!(out.proposal.increases_risk);
    assert_eq!(proposal_count(&env), 2);
    assert!(
        env.store
            .proposals_in_state(ProposalState::Rejected)
            .unwrap()
            .is_empty()
    );
}

// 검증: E28 E35 (승인이 안 되면 개선안은 제안됨으로 남고 새 개선안을 만들지 않는다)
#[test]
fn 위험해진_개선안의_승인이_안_되면_제안됨으로_남는다() {
    let env = Env::new();
    let current = base_params();
    let (first, second) = two_safe(&env, &current);
    let now = env
        .journal
        .approve(
            first.id,
            Some(env.approved("fp-1", T0)),
            &current,
            t(T0 + 5 * MIN),
        )
        .unwrap()
        .rules;
    let pending = ApprovalUse {
        id: env.pending_approval(T0),
        fingerprint: "fp-pending".to_string(),
    };
    let err = env
        .journal
        .approve(second.id, Some(pending), &now, t(T0 + 6 * MIN))
        .unwrap_err();
    assert!(
        matches!(err, Error::Store(StoreError::ApprovalNotUsable { .. })),
        "{err:?}"
    );
    let stored = env.store.get_proposal(second.id).unwrap();
    assert_eq!(stored.state, ProposalState::Proposed);
    assert_eq!(proposal_count(&env), 2);
    assert!(
        env.store
            .proposals_in_state(ProposalState::Rejected)
            .unwrap()
            .is_empty()
    );
}

// 검증: V12 (그날 중간에 상태만 바뀐 매매도 그날 한 일에 남는다)
#[test]
fn 그날_중간_상태_바뀜도_하루_정리에_남는다() {
    let env = Env::new();
    let start = T0 + DAY;
    let moved = env.trade_in("005930", TradeState::Judged, start - DAY + MIN);
    let idle = env.trade_in("000660", TradeState::Judged, start - DAY + MIN);
    env.store
        .transition_trade(moved, TradeState::Ordered, t(start + MIN))
        .unwrap();
    env.store
        .transition_trade(moved, TradeState::Holding, t(start + DAY + MIN))
        .unwrap();
    let day = Day {
        start: t(start),
        end: t(start + DAY),
    };
    let facts = DayFacts {
        realized: vec![],
        last_backup_at: None,
    };
    let summary = env.journal.daily_summary(day, &facts).unwrap();
    let ids: Vec<TradeId> = summary.done.iter().map(|line| line.id).collect();
    assert_eq!(ids, vec![moved]);
    assert_eq!(summary.done[0].state, TradeState::Holding);
    assert!(!ids.contains(&idle));
}
