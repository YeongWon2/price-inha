//! 위험 증가 판정 표(칸마다 빈 칸 전이 · 넓힘 · 좁힘)와 승인 때 다시 판정하기.

use pi_journal::{Mistake, increases_risk};
use pi_store::ProposalState;
use pi_template::{
    AtrBar, AtrStop, Change, Cooldown, CountRange, LossStreakStop, PctRange, RiskParams,
};
use rust_decimal::Decimal;

use crate::common::{Env, MIN, T0, base_params, t};

fn pct(n: i64) -> Decimal {
    Decimal::new(n, 0)
}

/// 손절 · 연속 손실 · 확신 비중 칸을 알려진 값으로 고정한 기준.
fn fixed_base() -> RiskParams {
    let mut p = base_params();
    p.stop.fixed_loss_pct = Some(PctRange {
        low: pct(2),
        high: pct(4),
    });
    p.stop.atr = Some(AtrStop {
        multiple: pct(2),
        bar: AtrBar::Day,
    });
    p.loss_streak = Some(LossStreakStop {
        losses: 3,
        cooldown: Cooldown::RestOfDay,
    });
    p.conviction_weight_pct = None;
    p
}

/// 기준을 고쳐 만든 값.
fn with(base: &RiskParams, edit: impl FnOnce(&mut RiskParams)) -> RiskParams {
    let mut p = base.clone();
    edit(&mut p);
    p
}

/// (이름, 전, 후, 위험 증가 기대). 표를 한 번에 돌려 어긋난 칸 이름을 알린다.
fn check(rows: Vec<(&str, RiskParams, RiskParams, bool)>) {
    for (name, before, after, expected) in rows {
        assert_eq!(increases_risk(&before, &after), expected, "칸: {name}");
    }
}

// 검증: M4 E28
#[test]
fn 확신_비중은_빈_칸에서_생겨도_종목_비중보다_넓어지면_위험이_커진다() {
    let base = fixed_base();
    let sym = base.symbol_weight_pct;
    check(vec![
        (
            "없음에서 종목 비중보다 넓은 값",
            base.clone(),
            with(&base, |p| p.conviction_weight_pct = Some(sym + pct(1))),
            true,
        ),
        (
            "없음에서 종목 비중보다 좁은 값",
            base.clone(),
            with(&base, |p| p.conviction_weight_pct = Some(sym - pct(1))),
            false,
        ),
        ("없음에서 없음", base.clone(), base.clone(), false),
        (
            "종목 비중보다 좁은 값에서 없음",
            with(&base, |p| p.conviction_weight_pct = Some(sym - pct(1))),
            base.clone(),
            true,
        ),
        (
            "종목 비중보다 넓은 값에서 없음",
            with(&base, |p| p.conviction_weight_pct = Some(sym + pct(1))),
            base.clone(),
            false,
        ),
        (
            "값에서 더 넓은 값",
            with(&base, |p| p.conviction_weight_pct = Some(sym - pct(2))),
            with(&base, |p| p.conviction_weight_pct = Some(sym - pct(1))),
            true,
        ),
        (
            "값에서 더 좁은 값",
            with(&base, |p| p.conviction_weight_pct = Some(sym - pct(1))),
            with(&base, |p| p.conviction_weight_pct = Some(sym - pct(2))),
            false,
        ),
    ]);
}

// 검증: M4 E28
#[test]
fn 고정_손절은_사라지거나_끝이_넓어지면_위험이_커진다() {
    let base = fixed_base();
    let range = |low: i64, high: i64| {
        Some(PctRange {
            low: pct(low),
            high: pct(high),
        })
    };
    let stop = |r: Option<PctRange>| with(&base, |p| p.stop.fixed_loss_pct = r);
    check(vec![
        (
            "있음에서 없음(손절이 사라짐)",
            stop(range(2, 4)),
            stop(None),
            true,
        ),
        (
            "없음에서 있음(손절이 생김)",
            stop(None),
            stop(range(2, 4)),
            false,
        ),
        ("없음에서 없음", stop(None), stop(None), false),
        ("위 끝이 넓어짐", stop(range(2, 4)), stop(range(2, 5)), true),
        (
            "위 끝이 좁아짐",
            stop(range(2, 4)),
            stop(range(2, 3)),
            false,
        ),
        (
            "아래 끝이 넓어짐",
            stop(range(2, 4)),
            stop(range(1, 4)),
            true,
        ),
        (
            "아래 끝이 좁아짐",
            stop(range(2, 4)),
            stop(range(3, 4)),
            false,
        ),
        ("같음", stop(range(2, 4)), stop(range(2, 4)), false),
    ]);
}

// 검증: M4 E28
#[test]
fn 연속_손실_정지는_해제되면_위험이_커진다() {
    let base = fixed_base();
    let streak = |s: Option<LossStreakStop>| with(&base, |p| p.loss_streak = s);
    let on = Some(LossStreakStop {
        losses: 3,
        cooldown: Cooldown::RestOfDay,
    });
    check(vec![
        ("있음에서 없음(해제)", streak(on), streak(None), true),
        ("없음에서 있음(걸림)", streak(None), streak(on), false),
        ("없음에서 없음", streak(None), streak(None), false),
        ("같음", streak(on), streak(on), false),
    ]);
}

// 검증: M4 E28
#[test]
fn atr_배수가_커지면_위험이_커지고_작아지면_아니다() {
    let base = fixed_base();
    let atr = |multiple: Option<i64>| {
        with(&base, |p| {
            p.stop.atr = multiple.map(|m| AtrStop {
                multiple: pct(m),
                bar: AtrBar::Day,
            });
        })
    };
    check(vec![
        ("배수 증가", atr(Some(2)), atr(Some(3)), true),
        ("배수 감소", atr(Some(3)), atr(Some(2)), false),
        ("배수 같음", atr(Some(2)), atr(Some(2)), false),
        ("없음에서 생김(손절이 생김)", atr(None), atr(Some(2)), false),
    ]);
}

// 검증: M4 E28
#[test]
fn 최소_현금이_줄면_위험이_커지고_늘면_아니다() {
    let base = fixed_base();
    check(vec![
        (
            "최소 현금 감소",
            base.clone(),
            with(&base, |p| p.min_cash_pct = base.min_cash_pct - pct(1)),
            true,
        ),
        (
            "최소 현금 증가",
            base.clone(),
            with(&base, |p| p.min_cash_pct = base.min_cash_pct + pct(1)),
            false,
        ),
        ("같음", base.clone(), base.clone(), false),
    ]);
}

// 검증: M4 E28
#[test]
fn 이미_있던_칸의_넓힘과_좁힘도_표로_맞는다() {
    let base = fixed_base();
    let up = |edit: fn(&mut RiskParams)| with(&base, edit);
    check(vec![
        (
            "1회 위험 상향",
            base.clone(),
            up(|p| p.risk_per_trade_pct += pct(1)),
            true,
        ),
        (
            "1회 위험 하향",
            base.clone(),
            up(|p| p.risk_per_trade_pct -= Decimal::new(1, 1)),
            false,
        ),
        (
            "일일 손실 상향",
            base.clone(),
            up(|p| p.daily_loss_limit_pct += pct(1)),
            true,
        ),
        (
            "일일 손실 하향",
            base.clone(),
            up(|p| p.daily_loss_limit_pct -= Decimal::new(1, 1)),
            false,
        ),
        (
            "최대 낙폭 상향",
            base.clone(),
            up(|p| p.max_drawdown_pct += pct(1)),
            true,
        ),
        (
            "최대 낙폭 하향",
            base.clone(),
            up(|p| p.max_drawdown_pct -= pct(1)),
            false,
        ),
        (
            "종목 비중 상향",
            base.clone(),
            up(|p| p.symbol_weight_pct += pct(1)),
            true,
        ),
        (
            "종목 비중 하향",
            base.clone(),
            up(|p| p.symbol_weight_pct -= pct(1)),
            false,
        ),
        (
            "업종 비중 상향",
            base.clone(),
            up(|p| p.sector_weight_pct += pct(1)),
            true,
        ),
        (
            "업종 비중 하향",
            base.clone(),
            up(|p| p.sector_weight_pct -= pct(1)),
            false,
        ),
        (
            "자동 주문 한도 상향",
            base.clone(),
            up(|p| p.auto_order_limit_pct += pct(1)),
            true,
        ),
        (
            "자동 주문 한도 하향",
            base.clone(),
            up(|p| p.auto_order_limit_pct -= pct(1)),
            false,
        ),
        (
            "일일 매매 횟수 상향",
            base.clone(),
            up(|p| p.max_trades_per_day += 1),
            true,
        ),
        (
            "일일 매매 횟수 하향",
            base.clone(),
            up(|p| p.max_trades_per_day = 0),
            false,
        ),
        (
            "동시 보유 상향",
            base.clone(),
            up(|p| {
                p.max_positions = CountRange {
                    low: p.max_positions.low,
                    high: p.max_positions.high + 1,
                }
            }),
            true,
        ),
        (
            "동시 보유 하향",
            base.clone(),
            up(|p| {
                p.max_positions = CountRange {
                    low: p.max_positions.low,
                    high: p.max_positions.high - 1,
                }
            }),
            false,
        ),
    ]);
}

/// 같은 실수 두 건을 남기고, 같은 `current`에서 위험을 키우지 않는 개선안 둘을 낸다.
/// 앞것은 일일 손실 한도를 좁히고, 뒤것은 종목 비중만 좁힌다(일일 손실 한도는 current 그대로).
fn two_proposals(env: &Env, current: &RiskParams) -> (pi_store::Proposal, pi_store::Proposal) {
    let a = env.sold("005930", T0);
    let b = env.sold("000660", T0);
    for (id, at) in [(a, T0 + MIN), (b, T0 + 2 * MIN)] {
        env.journal
            .review_trade(id, vec![Mistake::LateStop], "", t(at))
            .unwrap();
    }
    let first = env
        .journal
        .propose(
            &Mistake::LateStop,
            current,
            &[Change::DailyLossLimitPct(pct(1))],
            t(T0 + 3 * MIN),
        )
        .unwrap()
        .unwrap();
    let second = env
        .journal
        .propose(
            &Mistake::LateStop,
            current,
            &[Change::SymbolWeightPct(current.symbol_weight_pct - pct(1))],
            t(T0 + 4 * MIN),
        )
        .unwrap()
        .unwrap();
    (first, second)
}

// 검증: E28 E35 M4
#[test]
fn 다른_개선안이_먼저_반영돼_지금보다_위험해지면_승인해도_냉각중이다() {
    let env = Env::new();
    let current = base_params();
    let (first, second) = two_proposals(&env, &current);
    assert!(!first.increases_risk && !second.increases_risk);

    let applied = env
        .journal
        .approve(
            first.id,
            Some(env.approved("fp-1", T0)),
            &current,
            t(T0 + 5 * MIN),
        )
        .unwrap();
    assert_eq!(applied.proposal.state, ProposalState::Applied);
    let now = applied.rules;
    assert_eq!(now.daily_loss_limit_pct, pct(1));

    // 뒤 개선안의 after는 일일 손실 한도가 1.5%라 지금(1%)보다 위험하다.
    let out = env
        .journal
        .approve(
            second.id,
            Some(env.approved("fp-2", T0)),
            &now,
            t(T0 + 6 * MIN),
        )
        .unwrap();
    assert_eq!(out.proposal.state, ProposalState::Cooling);
    assert!(out.proposal.applied_at.is_none());
    assert_eq!(out.rules, now);
    assert_eq!(
        env.store
            .proposals_in_state(ProposalState::Applied)
            .unwrap()
            .len(),
        1
    );
}

// 검증: E28 E35 M4
#[test]
fn 지금과_같은_규칙에서_낸_개선안은_승인하면_그대로_반영된다() {
    let env = Env::new();
    let current = base_params();
    let (first, _second) = two_proposals(&env, &current);
    let out = env
        .journal
        .approve(
            first.id,
            Some(env.approved("fp-1", T0)),
            &current,
            t(T0 + 5 * MIN),
        )
        .unwrap();
    assert_eq!(out.proposal.state, ProposalState::Applied);
    assert_eq!(out.rules.daily_loss_limit_pct, pct(1));
}
