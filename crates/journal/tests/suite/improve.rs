//! 규칙 개선안: 제안 · 승인 · 거절 · 냉각 · 상한.

use pi_journal::{Error, Improvement, Mistake, increases_risk};
use pi_store::{ApprovalUse, NewProposal, ProposalState, StoreError, TradeId, UnixMillis};
use pi_template::{Change, PctRange, RiskParams};
use rust_decimal::Decimal;

use crate::common::{DAY, Env, MIN, T0, base_params, t};

fn pct(n: i64) -> Decimal {
    Decimal::new(n, 0)
}

/// 같은 실수를 기록한 매매 둘을 만들어 번호를 준다.
fn two_with(env: &Env, mistake: &Mistake) -> (TradeId, TradeId) {
    let a = env.sold("005930", T0);
    let b = env.sold("000660", T0);
    env.journal
        .review_trade(a, vec![mistake.clone()], "", t(T0 + MIN))
        .unwrap();
    env.journal
        .review_trade(b, vec![mistake.clone()], "", t(T0 + 2 * MIN))
        .unwrap();
    (a, b)
}

/// 일일 손실 한도를 1.5%에서 3%로 넓히는 변경(위험을 키운다. 상한 5% 안).
fn widen() -> Vec<Change> {
    vec![Change::DailyLossLimitPct(pct(3))]
}

/// 일일 손실 한도를 1.5%에서 1%로 좁히는 변경(위험을 키우지 않는다).
fn tighten() -> Vec<Change> {
    vec![Change::DailyLossLimitPct(pct(1))]
}

// 검증: V20 M4
#[test]
fn 같은_실수가_한_건이면_개선안이_없고_아무것도_만들지_않는다() {
    let env = Env::new();
    let a = env.sold("005930", T0);
    env.journal
        .review_trade(a, vec![Mistake::LateStop], "", t(T0 + MIN))
        .unwrap();
    let out = env
        .journal
        .propose(
            &Mistake::LateStop,
            &base_params(),
            &tighten(),
            t(T0 + 3 * MIN),
        )
        .unwrap();
    assert!(out.is_none());
    for state in ProposalState::ALL {
        assert!(env.store.proposals_in_state(state).unwrap().is_empty());
    }
}

// 검증: V20 M4
#[test]
fn 다른_실수_둘은_합치지_않는다() {
    let env = Env::new();
    let a = env.sold("005930", T0);
    let b = env.sold("000660", T0);
    env.journal
        .review_trade(a, vec![Mistake::LateStop], "", t(T0 + MIN))
        .unwrap();
    env.journal
        .review_trade(b, vec![Mistake::ChasedEntry], "", t(T0 + 2 * MIN))
        .unwrap();
    for mistake in [Mistake::LateStop, Mistake::ChasedEntry] {
        let out = env
            .journal
            .propose(&mistake, &base_params(), &tighten(), t(T0 + 3 * MIN))
            .unwrap();
        assert!(out.is_none());
    }
}

// 검증: V20 M4
#[test]
fn 같은_실수가_두_건이면_제안됨_개선안이_나오고_근거와_차이가_담긴다() {
    let env = Env::new();
    let (a, b) = two_with(&env, &Mistake::LateStop);
    let current = base_params();
    let proposal = env
        .journal
        .propose(&Mistake::LateStop, &current, &tighten(), t(T0 + 3 * MIN))
        .unwrap()
        .unwrap();
    assert_eq!(proposal.state, ProposalState::Proposed);
    assert_eq!(proposal.evidence, vec![a, b]);
    assert!(!proposal.increases_risk);
    assert_eq!(proposal.created_at, t(T0 + 3 * MIN));
    assert!(proposal.approval.is_none());

    let improvement = Improvement::from_proposal(&proposal).unwrap();
    assert_eq!(improvement.mistake, Mistake::LateStop);
    assert_eq!(improvement.before, current);
    assert_eq!(improvement.after.daily_loss_limit_pct, pct(1));
    let mut expected = current.clone();
    expected.daily_loss_limit_pct = pct(1);
    assert_eq!(improvement.after, expected);
    assert_eq!(env.store.get_proposal(proposal.id).unwrap(), proposal);
}

// 검증: V20 M4
#[test]
fn 개선안_내용이_이상하면_읽기가_거부된다() {
    let env = Env::new();
    let (a, b) = two_with(&env, &Mistake::NoRationale);
    let bad = env
        .store
        .create_proposal(NewProposal {
            content: serde_json::json!({"뜻밖": true}),
            evidence: vec![a, b],
            increases_risk: false,
            at: t(T0 + 3 * MIN),
        })
        .unwrap();
    let err = Improvement::from_proposal(&bad).unwrap_err();
    assert!(matches!(err, Error::BadProposal(_)), "{err:?}");
}

// 검증: V20 E28 M4
#[test]
fn 위험을_키우는_변경은_키운다고_표시되어_제안된다() {
    let env = Env::new();
    two_with(&env, &Mistake::OversizedPosition);
    let proposal = env
        .journal
        .propose(
            &Mistake::OversizedPosition,
            &base_params(),
            &widen(),
            t(T0 + 3 * MIN),
        )
        .unwrap()
        .unwrap();
    assert!(proposal.increases_risk);
    assert_eq!(proposal.state, ProposalState::Proposed);
}

// 검증: M4 E28
#[test]
fn 위험_판정은_한도와_손절을_넓힐_때만_참이다() {
    let base = base_params();
    assert!(!increases_risk(&base, &base));

    let mut wider: RiskParams = base.clone();
    wider.daily_loss_limit_pct = base.daily_loss_limit_pct + pct(1);
    assert!(increases_risk(&base, &wider));
    assert!(!increases_risk(&wider, &base));

    let mut risk = base.clone();
    risk.risk_per_trade_pct = base.risk_per_trade_pct + Decimal::new(5, 1);
    assert!(increases_risk(&base, &risk));

    let mut dd = base.clone();
    dd.max_drawdown_pct = base.max_drawdown_pct + pct(1);
    assert!(increases_risk(&base, &dd));

    let mut weight = base.clone();
    weight.symbol_weight_pct = base.symbol_weight_pct + pct(1);
    assert!(increases_risk(&base, &weight));

    let mut narrow_stop = base.clone();
    narrow_stop.stop.fixed_loss_pct = Some(PctRange {
        low: pct(1),
        high: pct(2),
    });
    let mut wide_stop = narrow_stop.clone();
    wide_stop.stop.fixed_loss_pct = Some(PctRange {
        low: pct(1),
        high: pct(3),
    });
    assert!(increases_risk(&narrow_stop, &wide_stop));
    assert!(!increases_risk(&wide_stop, &narrow_stop));

    let mut other = base.clone();
    other.max_trades_per_day = base.max_trades_per_day.saturating_sub(1);
    assert!(!increases_risk(&base, &other));
}

// 검증: V20 M4
#[test]
fn 절대_상한을_넘는_개선안은_제안되지_않는다() {
    let env = Env::new();
    two_with(&env, &Mistake::PanicExit);
    let err = env
        .journal
        .propose(
            &Mistake::PanicExit,
            &base_params(),
            &[Change::DailyLossLimitPct(pct(6))],
            t(T0 + 3 * MIN),
        )
        .unwrap_err();
    assert!(matches!(err, Error::OverCap(_)), "{err:?}");
    for state in ProposalState::ALL {
        assert!(env.store.proposals_in_state(state).unwrap().is_empty());
    }
}

// 검증: V20 V21 E35
#[test]
fn 승인_번호가_없으면_규칙이_바뀌지_않고_개선안도_그대로다() {
    let env = Env::new();
    two_with(&env, &Mistake::LateStop);
    let current = base_params();
    let proposal = env
        .journal
        .propose(&Mistake::LateStop, &current, &tighten(), t(T0 + 3 * MIN))
        .unwrap()
        .unwrap();
    let err = env
        .journal
        .approve(proposal.id, None, &current, t(T0 + 4 * MIN))
        .unwrap_err();
    assert!(matches!(err, Error::ApprovalRequired), "{err:?}");
    assert_eq!(env.store.get_proposal(proposal.id).unwrap(), proposal);
}

// 검증: E35
#[test]
fn 내가_답하지_않았거나_지문이_다른_승인으로는_반영되지_않는다() {
    let env = Env::new();
    two_with(&env, &Mistake::LateStop);
    let current = base_params();
    let proposal = env
        .journal
        .propose(&Mistake::LateStop, &current, &tighten(), t(T0 + 3 * MIN))
        .unwrap()
        .unwrap();

    let pending = ApprovalUse {
        id: env.pending_approval(T0),
        fingerprint: "fp-pending".to_string(),
    };
    let err = env
        .journal
        .approve(proposal.id, Some(pending), &current, t(T0 + 4 * MIN))
        .unwrap_err();
    assert!(
        matches!(err, Error::Store(StoreError::ApprovalNotUsable { .. })),
        "{err:?}"
    );

    let ok = env.approved("fp-real", T0);
    let wrong = ApprovalUse {
        id: ok.id,
        fingerprint: "fp-other".to_string(),
    };
    let err = env
        .journal
        .approve(proposal.id, Some(wrong), &current, t(T0 + 4 * MIN))
        .unwrap_err();
    assert!(
        matches!(
            err,
            Error::Store(StoreError::ApprovalFingerprintMismatch { .. })
        ),
        "{err:?}"
    );
    assert_eq!(env.store.get_proposal(proposal.id).unwrap(), proposal);
    assert!(env.store.get_approval(ok.id).unwrap().consumed_at.is_none());
}

// 검증: V20 V21 M4
#[test]
fn 위험을_키우지_않는_개선안은_승인하면_바로_반영된다() {
    let env = Env::new();
    two_with(&env, &Mistake::LateStop);
    let current = base_params();
    let proposal = env
        .journal
        .propose(&Mistake::LateStop, &current, &tighten(), t(T0 + 3 * MIN))
        .unwrap()
        .unwrap();
    let approval = env.approved("fp-1", T0);
    let at = t(T0 + 4 * MIN);
    let out = env
        .journal
        .approve(proposal.id, Some(approval.clone()), &current, at)
        .unwrap();
    assert_eq!(out.proposal.state, ProposalState::Applied);
    assert_eq!(out.proposal.applied_at, Some(at));
    assert_eq!(out.proposal.approval, Some(approval.id));
    assert_eq!(out.rules.daily_loss_limit_pct, pct(1));
    assert_eq!(
        out.rules,
        Improvement::from_proposal(&out.proposal).unwrap().after
    );
    assert!(
        env.store
            .get_approval(approval.id)
            .unwrap()
            .consumed_at
            .is_some()
    );
}

// 검증: V21
#[test]
fn 거절하면_규칙이_그대로고_거절됨이며_다시_승인할_수_없다() {
    let env = Env::new();
    two_with(&env, &Mistake::LateStop);
    let current = base_params();
    let proposal = env
        .journal
        .propose(&Mistake::LateStop, &current, &tighten(), t(T0 + 3 * MIN))
        .unwrap()
        .unwrap();
    let out = env
        .journal
        .reject(proposal.id, &current, t(T0 + 4 * MIN))
        .unwrap();
    assert_eq!(out.proposal.state, ProposalState::Rejected);
    assert_eq!(out.rules, current);

    let approval = env.approved("fp-late", T0);
    let err = env
        .journal
        .approve(
            proposal.id,
            Some(approval.clone()),
            &current,
            t(T0 + 5 * MIN),
        )
        .unwrap_err();
    assert!(
        matches!(err, Error::Store(StoreError::TransitionDenied { .. })),
        "{err:?}"
    );
    assert_eq!(
        env.store.get_proposal(proposal.id).unwrap().state,
        ProposalState::Rejected
    );
    assert!(
        env.store
            .get_approval(approval.id)
            .unwrap()
            .consumed_at
            .is_none()
    );
}

// 검증: E28 M4
#[test]
fn 위험을_키우는_개선안은_승인해도_냉각중이고_규칙은_그대로다() {
    let env = Env::new();
    two_with(&env, &Mistake::OversizedPosition);
    let current = base_params();
    let proposal = env
        .journal
        .propose(
            &Mistake::OversizedPosition,
            &current,
            &widen(),
            t(T0 + 3 * MIN),
        )
        .unwrap()
        .unwrap();
    let out = env
        .journal
        .approve(
            proposal.id,
            Some(env.approved("fp-1", T0)),
            &current,
            t(T0 + 4 * MIN),
        )
        .unwrap();
    assert_eq!(out.proposal.state, ProposalState::Cooling);
    assert!(out.proposal.applied_at.is_none());
    assert_eq!(out.rules, current);
}

// 검증: E28
#[test]
fn 냉각중_개선안은_다음_거래일_시작_전에는_반영되지_않고_시작_시각부터_반영된다() {
    let env = Env::new();
    two_with(&env, &Mistake::OversizedPosition);
    let current = base_params();
    let proposal = env
        .journal
        .propose(
            &Mistake::OversizedPosition,
            &current,
            &widen(),
            t(T0 + 3 * MIN),
        )
        .unwrap()
        .unwrap();
    env.journal
        .approve(
            proposal.id,
            Some(env.approved("fp-1", T0)),
            &current,
            t(T0 + 4 * MIN),
        )
        .unwrap();
    let start = t(T0 + DAY);

    let early = UnixMillis(start.0 - 1);
    let err = env
        .journal
        .apply_cooled(proposal.id, &current, start, early)
        .unwrap_err();
    assert!(
        matches!(err, Error::StillCooling { until, at } if until == start && at == early),
        "{err:?}"
    );
    assert_eq!(
        env.store.get_proposal(proposal.id).unwrap().state,
        ProposalState::Cooling
    );

    let out = env
        .journal
        .apply_cooled(proposal.id, &current, start, start)
        .unwrap();
    assert_eq!(out.proposal.state, ProposalState::Applied);
    assert_eq!(out.proposal.applied_at, Some(start));
    assert_eq!(out.rules.daily_loss_limit_pct, pct(3));
}

// 검증: E28 V20
#[test]
fn 냉각중인_개선안은_다시_열어도_이어서_반영된다() {
    let env = Env::new();
    two_with(&env, &Mistake::OversizedPosition);
    let current = base_params();
    let proposal = env
        .journal
        .propose(
            &Mistake::OversizedPosition,
            &current,
            &widen(),
            t(T0 + 3 * MIN),
        )
        .unwrap()
        .unwrap();
    env.journal
        .approve(
            proposal.id,
            Some(env.approved("fp-1", T0)),
            &current,
            t(T0 + 4 * MIN),
        )
        .unwrap();
    let env = env.reopen();
    assert_eq!(
        env.store.get_proposal(proposal.id).unwrap().state,
        ProposalState::Cooling
    );
    let start = t(T0 + DAY);
    let out = env
        .journal
        .apply_cooled(proposal.id, &current, start, t(start.0 + MIN))
        .unwrap();
    assert_eq!(out.rules.daily_loss_limit_pct, pct(3));
}

// 검증: E28
#[test]
fn 냉각중이_아닌_개선안은_냉각_해제로_반영되지_않는다() {
    let env = Env::new();
    two_with(&env, &Mistake::LateStop);
    let current = base_params();
    let proposal = env
        .journal
        .propose(&Mistake::LateStop, &current, &tighten(), t(T0 + 3 * MIN))
        .unwrap()
        .unwrap();
    let err = env
        .journal
        .apply_cooled(proposal.id, &current, t(T0 + DAY), t(T0 + DAY))
        .unwrap_err();
    assert!(
        matches!(err, Error::Store(StoreError::TransitionDenied { .. })),
        "{err:?}"
    );
    assert_eq!(env.store.get_proposal(proposal.id).unwrap(), proposal);
}

// 검증: M4 E28 E35
#[test]
fn 반영하려는_값이_절대_상한을_넘으면_승인도_개선안도_그대로다() {
    let env = Env::new();
    let (a, b) = two_with(&env, &Mistake::PanicExit);
    let current = base_params();
    let mut over = current.clone();
    over.daily_loss_limit_pct = pct(6);
    let content = serde_json::to_value(Improvement {
        mistake: Mistake::PanicExit,
        before: current.clone(),
        after: over,
    })
    .unwrap();
    let proposal = env
        .store
        .create_proposal(NewProposal {
            content,
            evidence: vec![a, b],
            increases_risk: true,
            at: t(T0 + 3 * MIN),
        })
        .unwrap();
    let approval = env.approved("fp-over", T0);
    let err = env
        .journal
        .approve(
            proposal.id,
            Some(approval.clone()),
            &current,
            t(T0 + 4 * MIN),
        )
        .unwrap_err();
    assert!(matches!(err, Error::OverCap(_)), "{err:?}");
    assert_eq!(env.store.get_proposal(proposal.id).unwrap(), proposal);
    assert!(
        env.store
            .get_approval(approval.id)
            .unwrap()
            .consumed_at
            .is_none()
    );
}
