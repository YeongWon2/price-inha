//! 5차: 위험 증가 판정이 템플릿 판정과 같은 답이다(여섯 가지 변경 · 승인 때 위험 표시 · 줄이기만 한 개선안).

use pi_journal::{Mistake, increases_risk};
use pi_store::ProposalState;
use pi_template::{AtrBar, AtrStop, Change, MaxHolding, PromotionCriteria, RiskParams};
use rust_decimal::Decimal;

use crate::common::{Env, MIN, T0, base_params, t};

fn pct(n: i64) -> Decimal {
    Decimal::new(n, 0)
}

/// 기준을 고쳐 만든 값.
fn with(base: &RiskParams, edit: impl FnOnce(&mut RiskParams)) -> RiskParams {
    let mut p = base.clone();
    edit(&mut p);
    p
}

/// 한 칸의 (이름, 엄격한 값, 느슨한 값). 엄격 → 느슨이 위험 증가다.
type Case = (&'static str, RiskParams, RiskParams);

fn cases() -> Vec<Case> {
    let base = base_params();
    let atr = |bar| {
        with(&base, move |p| {
            p.stop.atr = Some(AtrStop {
                multiple: pct(2),
                bar,
            })
        })
    };
    let chase = |v: Option<Decimal>| with(&base, move |p| p.chase_limit_pct = v);
    let hold = |h| with(&base, move |p| p.max_holding = h);
    let promo = |trades: u32| {
        with(&base, move |p| {
            p.promotion = PromotionCriteria {
                min_trades: trades,
                ..p.promotion.clone()
            }
        })
    };
    vec![
        (
            "논리 손절 끄기",
            with(&base, |p| p.stop.thesis_break = true),
            with(&base, |p| p.stop.thesis_break = false),
        ),
        (
            "마감 전 청산 끄기",
            with(&base, |p| p.stop.flatten_before_close = true),
            with(&base, |p| p.stop.flatten_before_close = false),
        ),
        (
            "최대 보유 기간 늘림",
            hold(MaxHolding::TradingDays(2)),
            hold(MaxHolding::TradingDays(5)),
        ),
        ("추격 폭 넓힘", chase(Some(pct(2))), chase(Some(pct(3)))),
        ("추격 폭 없앰", chase(Some(pct(2))), chase(None)),
        ("승격 기준 느슨하게", promo(30), promo(10)),
        ("ATR 분봉에서 일봉", atr(AtrBar::Minute), atr(AtrBar::Day)),
    ]
}

// 검증: M4 E28
#[test]
fn 여섯_가지_변경은_저마다_위험_증가이고_템플릿과_같은_답이다() {
    for (name, strict, loose) in cases() {
        assert!(increases_risk(&strict, &loose), "위험 증가여야 함: {name}");
        assert_eq!(
            increases_risk(&strict, &loose),
            pi_template::increases_risk(&strict, &loose),
            "템플릿과 같은 답: {name}"
        );
    }
}

// 검증: M4 E28
#[test]
fn 같은_변경을_거꾸로_하면_줄이기라_위험_증가가_아니고_템플릿과_같은_답이다() {
    for (name, strict, loose) in cases() {
        assert!(
            !increases_risk(&loose, &strict),
            "줄이기는 거짓이어야 함: {name}"
        );
        assert_eq!(
            increases_risk(&loose, &strict),
            pi_template::increases_risk(&loose, &strict),
            "템플릿과 같은 답: {name}"
        );
        assert!(!increases_risk(&strict, &strict), "같은 값: {name}");
    }
}

/// 같은 실수 두 건을 남긴다(개선안 하나를 낼 수 있는 근거).
fn leave_evidence(env: &Env) {
    let a = env.sold("005930", T0);
    let b = env.sold("000660", T0);
    for (id, at) in [(a, T0 + MIN), (b, T0 + 2 * MIN)] {
        env.journal
            .review_trade(id, vec![Mistake::LateStop], "", t(at))
            .unwrap();
    }
}

// 검증: E28 E35 M4
#[test]
fn 승인_때_여섯_가지_변경이면_위험_표시가_오르고_냉각중이다() {
    for (name, strict, loose) in cases() {
        let env = Env::new();
        leave_evidence(&env);
        // 제안 때: 느슨한 규칙에서 일일 손실 한도만 좁힌다(위험을 키우지 않는다).
        let current = with(&loose, |p| p.daily_loss_limit_pct = pct(2));
        let proposal = env
            .journal
            .propose(
                &Mistake::LateStop,
                &current,
                &[Change::DailyLossLimitPct(pct(1))],
                t(T0 + 3 * MIN),
            )
            .unwrap()
            .unwrap();
        assert!(!proposal.increases_risk, "제안 때는 안전: {name}");

        // 승인 때: 지금 규칙이 더 엄격해 개선안의 느슨한 칸이 위험 증가다.
        let now = with(&strict, |p| p.daily_loss_limit_pct = pct(1));
        let out = env
            .journal
            .approve(
                proposal.id,
                Some(env.approved("fp-unified", T0)),
                &now,
                t(T0 + 5 * MIN),
            )
            .unwrap();
        assert_eq!(out.proposal.state, ProposalState::Cooling, "냉각중: {name}");
        assert!(out.proposal.increases_risk, "위험 표시: {name}");
        assert_eq!(out.rules, now, "규칙은 그대로: {name}");
    }
}

// 검증: E28 E35 M4
#[test]
fn 줄이기만_한_개선안은_위험_표시가_거짓이고_승인하면_반영된다() {
    for (name, _strict, loose) in cases() {
        let env = Env::new();
        leave_evidence(&env);
        // 느슨한 규칙에서 일일 손실 한도를 좁히고, 해당 칸도 엄격하게 되돌린다.
        let current = with(&loose, |p| p.daily_loss_limit_pct = pct(2));
        let proposal = env
            .journal
            .propose(
                &Mistake::LateStop,
                &current,
                &[Change::DailyLossLimitPct(pct(1))],
                t(T0 + 3 * MIN),
            )
            .unwrap()
            .unwrap();
        assert!(!proposal.increases_risk, "줄이기만: {name}");

        // 지금 규칙이 개선안과 같은 쪽(느슨한 쪽)이어도 줄이기만 한 개선안이다.
        let out = env
            .journal
            .approve(
                proposal.id,
                Some(env.approved("fp-unified-2", T0)),
                &current,
                t(T0 + 5 * MIN),
            )
            .unwrap();
        assert_eq!(out.proposal.state, ProposalState::Applied, "반영됨: {name}");
        assert!(!out.proposal.increases_risk, "표시 거짓: {name}");
        assert_eq!(out.rules.daily_loss_limit_pct, pct(1), "{name}");
    }
}
