//! 절대 상한과 값 맞추기(`dev.md` C14 · E5 · V28, `principles.md` P1 · P3 · P4 · P8).

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::error::{Invalid, InvalidReason, OverCap, ParamField, Violation};
use crate::params::{
    ApprovalWait, CountRange, EntrySignal, LossStreakStop, MaxHolding, PctRange, Rebalance,
    RiskParams, StopRule, TakeProfitRule, TradingWindow, TriggerKind, UnfilledCancel, Universe,
};
use crate::promotion::PromotionCriteria;

/// 바꿀 수 없는 절대 상한(퍼센트). 템플릿 · 에이전트 · 복기 개선안이 바꿀 수 없다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AbsoluteCaps {
    /// 1회 위험 비율 R%(P1).
    pub risk_per_trade_pct: Decimal,
    /// 일일 손실(P3).
    pub daily_loss_pct: Decimal,
    /// 최대 낙폭(P4).
    pub max_drawdown_pct: Decimal,
    /// 종목당 비중(P8).
    pub symbol_weight_pct: Decimal,
    /// 업종당 비중(P8).
    pub sector_weight_pct: Decimal,
}

impl AbsoluteCaps {
    /// 원칙의 상한: R 2% · 하루 손실 5% · 최대 낙폭 20% · 종목 25% · 업종 50%.
    pub const FIXED: AbsoluteCaps = AbsoluteCaps {
        risk_per_trade_pct: Decimal::from_parts(2, 0, 0, false, 0),
        daily_loss_pct: Decimal::from_parts(5, 0, 0, false, 0),
        max_drawdown_pct: Decimal::from_parts(20, 0, 0, false, 0),
        symbol_weight_pct: Decimal::from_parts(25, 0, 0, false, 0),
        sector_weight_pct: Decimal::from_parts(50, 0, 0, false, 0),
    };
}

/// 상한이 걸린 칸.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CappedField {
    /// [`RiskParams::risk_per_trade_pct`].
    RiskPerTrade,
    /// [`RiskParams::daily_loss_limit_pct`].
    DailyLossLimit,
    /// [`RiskParams::max_drawdown_pct`].
    MaxDrawdown,
    /// [`RiskParams::symbol_weight_pct`].
    SymbolWeight,
    /// [`RiskParams::conviction_weight_pct`].
    ConvictionWeight,
    /// [`RiskParams::sector_weight_pct`].
    SectorWeight,
}

/// 칸 하나를 바꾸는 요청. [`RiskParams`] 의 칸마다 하나씩 있다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    /// 1회 위험 비율 R%.
    RiskPerTradePct(Decimal),
    /// 일일 손실 한도.
    DailyLossLimitPct(Decimal),
    /// 최대 낙폭 한도.
    MaxDrawdownPct(Decimal),
    /// 연속 손실 정지.
    LossStreak(Option<LossStreakStop>),
    /// 최대 동시 보유 종목 수.
    MaxPositions(CountRange),
    /// 종목당 최대 비중.
    SymbolWeightPct(Decimal),
    /// 확신 종목 최대 비중.
    ConvictionWeightPct(Option<Decimal>),
    /// 업종당 최대 비중.
    SectorWeightPct(Decimal),
    /// 최소 현금 비중.
    MinCashPct(Decimal),
    /// 손절 방식.
    Stop(StopRule),
    /// 익절 방식.
    TakeProfit(TakeProfitRule),
    /// 최대 보유 기간.
    MaxHolding(MaxHolding),
    /// 진입 신호.
    EntrySignals(Vec<EntrySignal>),
    /// 유니버스.
    Universe(Universe),
    /// 자동 주문 금액 한도.
    AutoOrderLimitPct(Decimal),
    /// 거래 시간대.
    TradingWindow(TradingWindow),
    /// 추격 허용 폭.
    ChaseLimitPct(Option<Decimal>),
    /// 일일 최대 매매 횟수.
    MaxTradesPerDay(u32),
    /// 구독 트리거.
    Triggers(Vec<TriggerKind>),
    /// 리밸런싱.
    Rebalance(Option<Rebalance>),
    /// 승인 대기 시간.
    ApprovalWait(ApprovalWait),
    /// 미체결 취소 시간.
    UnfilledCancel(UnfilledCancel),
    /// 승격 기준.
    Promotion(PromotionCriteria),
}

/// 지금 값에 바꿀 칸들을 적용한 새 값을 준다(C14).
///
/// 바꿀 칸들은 순서대로 적용한다(같은 칸이 여럿이면 뒤의 것이 남는다). 다만 **넣은 값 하나라도**
/// [`AbsoluteCaps::FIXED`] 를 넘으면(뒤에서 고쳐 넣었더라도) **전체를 거부**한다(E5).
/// 확신 종목 비중은 종목 상한(25%)을 따른다. 상한과 같은 값은 받는다.
/// 오류에는 넘은 칸 · 넣은 값 · 상한이 [`CappedField`] 순서로 모두 담긴다. `current` 는 바뀌지 않는다.
///
/// 아래쪽 경계도 거부한다(2차): 음수 · 0인 비율(R% · 일일 손실 · 최대 낙폭 · 종목 · 확신 종목 · 업종 비중),
/// 100%를 넘는 비율, 아래 끝이 위 끝보다 큰 범위(`PctRange` · `CountRange`), 0 이하의 개수 · 시간,
/// 적용한 결과에서 업종 비중이 종목 비중(확신 종목 비중 포함)보다 작은 것. 이 위반은
/// [`OverCap::invalid`] 에 칸과 이유([`crate::InvalidReason`])로 담긴다.
pub fn adjust(current: &RiskParams, changes: &[Change]) -> Result<RiskParams, OverCap> {
    let mut next = current.clone();
    let mut violations = Vec::new();
    for change in changes {
        if let Some((field, requested)) = change.capped_value()
            && let Some(v) = violation(field, requested)
        {
            violations.push(v);
        }
        change.apply_to(&mut next);
    }
    let invalid = invalid_fields(&next);
    finish(violations, invalid)?;
    Ok(next)
}

/// 값 전체가 상한 안이고 아래쪽 경계 · 모양이 맞는지 본다(읽어 들인 템플릿 점검용).
/// 넘은 칸은 [`CappedField`] 순서로, 어긴 칸은 [`ParamField`] 순서로 모두 담는다.
pub(crate) fn check(params: &RiskParams) -> Result<(), OverCap> {
    let fields = [
        (CappedField::RiskPerTrade, Some(params.risk_per_trade_pct)),
        (
            CappedField::DailyLossLimit,
            Some(params.daily_loss_limit_pct),
        ),
        (CappedField::MaxDrawdown, Some(params.max_drawdown_pct)),
        (CappedField::SymbolWeight, Some(params.symbol_weight_pct)),
        (CappedField::ConvictionWeight, params.conviction_weight_pct),
        (CappedField::SectorWeight, Some(params.sector_weight_pct)),
    ];
    let violations = fields
        .into_iter()
        .filter_map(|(field, value)| value.and_then(|v| violation(field, v)))
        .collect();
    finish(violations, invalid_fields(params))
}

/// 넘은 칸 · 어긴 칸이 있으면 각자의 순서로 정렬해 오류로 낸다(같은 칸은 넣은 순서를 지킨다).
fn finish(mut violations: Vec<Violation>, mut invalid: Vec<Invalid>) -> Result<(), OverCap> {
    if violations.is_empty() && invalid.is_empty() {
        return Ok(());
    }
    violations.sort_by_key(|v| v.field.order());
    invalid.sort_by_key(|i| param_order(i.field));
    invalid.dedup();
    Err(OverCap {
        violations,
        invalid,
    })
}

/// [`ParamField`] 의 선언 순서.
fn param_order(field: ParamField) -> usize {
    ALL_PARAM_FIELDS
        .iter()
        .position(|f| *f == field)
        .unwrap_or(usize::MAX)
}

const ALL_PARAM_FIELDS: [ParamField; 23] = [
    ParamField::RiskPerTrade,
    ParamField::DailyLossLimit,
    ParamField::MaxDrawdown,
    ParamField::LossStreak,
    ParamField::MaxPositions,
    ParamField::SymbolWeight,
    ParamField::ConvictionWeight,
    ParamField::SectorWeight,
    ParamField::MinCash,
    ParamField::Stop,
    ParamField::TakeProfit,
    ParamField::MaxHolding,
    ParamField::EntrySignals,
    ParamField::Universe,
    ParamField::AutoOrderLimit,
    ParamField::TradingWindow,
    ParamField::ChaseLimit,
    ParamField::MaxTradesPerDay,
    ParamField::Triggers,
    ParamField::Rebalance,
    ParamField::ApprovalWait,
    ParamField::UnfilledCancel,
    ParamField::Promotion,
];

/// 아래쪽 경계 · 모양을 어긴 칸들(2차 1 · 3번). 칸 값 전체를 보고 판단한다.
fn invalid_fields(p: &RiskParams) -> Vec<Invalid> {
    let mut out = Vec::new();
    let mut add = |field, reason| out.push(Invalid { field, reason });
    let zero = Decimal::ZERO;
    let hundred = Decimal::ONE_HUNDRED;

    // 상한이 걸린 비율: 0 이하 거부(상한 초과는 violations 가 말한다).
    let positive_only = [
        (ParamField::RiskPerTrade, Some(p.risk_per_trade_pct)),
        (ParamField::DailyLossLimit, Some(p.daily_loss_limit_pct)),
        (ParamField::MaxDrawdown, Some(p.max_drawdown_pct)),
        (ParamField::SymbolWeight, Some(p.symbol_weight_pct)),
        (ParamField::ConvictionWeight, p.conviction_weight_pct),
        (ParamField::SectorWeight, Some(p.sector_weight_pct)),
    ];
    for (field, value) in positive_only {
        if value.is_some_and(|v| v <= zero) {
            add(field, InvalidReason::NotPositive);
        }
    }

    // 업종 비중은 종목 비중(확신 종목 비중 포함)보다 작을 수 없다.
    let widest = p
        .conviction_weight_pct
        .map_or(p.symbol_weight_pct, |c| c.max(p.symbol_weight_pct));
    if p.sector_weight_pct < widest {
        add(ParamField::SectorWeight, InvalidReason::SectorBelowSymbol);
    }

    // 0 에서 100 사이여야 하는 비율(0 은 받는다).
    let share = |field, value: Decimal, add: &mut dyn FnMut(ParamField, InvalidReason)| {
        if value < zero {
            add(field, InvalidReason::NotPositive);
        } else if value > hundred {
            add(field, InvalidReason::Over100Pct);
        }
    };
    share(ParamField::MinCash, p.min_cash_pct, &mut add);
    share(ParamField::AutoOrderLimit, p.auto_order_limit_pct, &mut add);
    if let Some(v) = p.chase_limit_pct {
        share(ParamField::ChaseLimit, v, &mut add);
    }

    if let Some(ls) = p.loss_streak
        && (ls.losses == 0 || matches!(ls.cooldown, crate::params::Cooldown::TradingDays(0)))
    {
        add(ParamField::LossStreak, InvalidReason::NotPositive);
    }

    count_range(ParamField::MaxPositions, p.max_positions, &mut add);

    invalid_rules(p, &mut add);

    let w = p.approval_wait;
    if w.stop_loss.is_zero() || w.take_profit.is_zero() || w.buy.is_zero() {
        add(ParamField::ApprovalWait, InvalidReason::NotPositive);
    }

    if matches!(p.unfilled_cancel, UnfilledCancel::After(d) if d.is_zero()) {
        add(ParamField::UnfilledCancel, InvalidReason::NotPositive);
    }

    let promo: &PromotionCriteria = &p.promotion;
    if promo.min_trades == 0 || promo.min_trading_days == 0 {
        add(ParamField::Promotion, InvalidReason::NotPositive);
    }
    share(ParamField::Promotion, promo.max_drawdown_pct, &mut add);
    // 임계값 하한: 비용 뺀 손익이 음수여도 통과하는 기준은 승격 기준이 아니다(P14).
    if promo.net_pnl_above < zero || promo.out_of_sample_pnl_above.is_some_and(|v| v < zero) {
        add(ParamField::Promotion, InvalidReason::NotPositive);
    }
    out
}

/// 손절 · 익절 · 보유 · 유니버스 · 횟수 · 리밸런싱 칸의 경계.
fn invalid_rules(p: &RiskParams, add: &mut dyn FnMut(ParamField, InvalidReason)) {
    let zero = Decimal::ZERO;
    let stop: &StopRule = &p.stop;
    if let Some(range) = stop.fixed_loss_pct {
        pct_range(ParamField::Stop, range, add);
    }
    if stop.atr.is_some_and(|a| a.multiple <= zero) {
        add(ParamField::Stop, InvalidReason::NotPositive);
    }

    // 손절 방식이 하나도 없으면(보유 기간 제한도 없음) 손실을 끊을 길이 없다.
    if stop.fixed_loss_pct.is_none()
        && stop.atr.is_none()
        && !stop.thesis_break
        && !stop.flatten_before_close
        && matches!(p.max_holding, MaxHolding::Unlimited)
    {
        add(ParamField::Stop, InvalidReason::NoStopMethod);
    }

    let tp: &TakeProfitRule = &p.take_profit;
    if tp.r_target.is_some_and(|r| r <= zero) {
        add(ParamField::TakeProfit, InvalidReason::NotPositive);
    }

    if matches!(p.max_holding, MaxHolding::TradingDays(0)) {
        add(ParamField::MaxHolding, InvalidReason::NotPositive);
    }

    let u: &Universe = &p.universe;
    if u.min_market_cap.is_some_and(|v| v < zero) || u.min_turnover.is_some_and(|v| v < zero) {
        add(ParamField::Universe, InvalidReason::NotPositive);
    }

    if p.max_trades_per_day == 0 {
        add(ParamField::MaxTradesPerDay, InvalidReason::NotPositive);
    }

    if let Some(r) = p.rebalance {
        if r.every_trading_days == 0 {
            add(ParamField::Rebalance, InvalidReason::NotPositive);
        }
        if r.drift_pct < zero {
            add(ParamField::Rebalance, InvalidReason::NotPositive);
        } else if r.drift_pct > Decimal::ONE_HUNDRED {
            add(ParamField::Rebalance, InvalidReason::Over100Pct);
        }
    }
}

fn count_range(field: ParamField, r: CountRange, add: &mut dyn FnMut(ParamField, InvalidReason)) {
    if r.low == 0 {
        add(field, InvalidReason::NotPositive);
    }
    if r.low > r.high {
        add(field, InvalidReason::RangeReversed);
    }
}

fn pct_range(field: ParamField, r: PctRange, add: &mut dyn FnMut(ParamField, InvalidReason)) {
    if r.low <= Decimal::ZERO {
        add(field, InvalidReason::NotPositive);
    }
    if r.high > Decimal::ONE_HUNDRED {
        add(field, InvalidReason::Over100Pct);
    }
    if r.low > r.high {
        add(field, InvalidReason::RangeReversed);
    }
}

/// 상한을 넘으면 그 위반. 상한과 같은 값은 받는다.
fn violation(field: CappedField, requested: Decimal) -> Option<Violation> {
    let cap = field.cap();
    (requested > cap).then_some(Violation {
        field,
        requested,
        cap,
    })
}

impl CappedField {
    /// 이 칸의 절대 상한. 확신 종목 비중은 종목 상한을 따른다.
    pub(crate) fn cap(self) -> Decimal {
        let caps = AbsoluteCaps::FIXED;
        match self {
            CappedField::RiskPerTrade => caps.risk_per_trade_pct,
            CappedField::DailyLossLimit => caps.daily_loss_pct,
            CappedField::MaxDrawdown => caps.max_drawdown_pct,
            CappedField::SymbolWeight | CappedField::ConvictionWeight => caps.symbol_weight_pct,
            CappedField::SectorWeight => caps.sector_weight_pct,
        }
    }

    /// 화면에 보이는 칸 이름.
    pub(crate) fn label(self) -> &'static str {
        match self {
            CappedField::RiskPerTrade => "1회 위험 비율",
            CappedField::DailyLossLimit => "일일 손실 한도",
            CappedField::MaxDrawdown => "최대 낙폭 한도",
            CappedField::SymbolWeight => "종목당 최대 비중",
            CappedField::ConvictionWeight => "확신 종목 최대 비중",
            CappedField::SectorWeight => "업종당 최대 비중",
        }
    }

    /// 선언 순서.
    fn order(self) -> u8 {
        match self {
            CappedField::RiskPerTrade => 0,
            CappedField::DailyLossLimit => 1,
            CappedField::MaxDrawdown => 2,
            CappedField::SymbolWeight => 3,
            CappedField::ConvictionWeight => 4,
            CappedField::SectorWeight => 5,
        }
    }
}

impl Change {
    /// 상한이 걸린 칸이면 그 칸과 넣으려는 값.
    fn capped_value(&self) -> Option<(CappedField, Decimal)> {
        match self {
            Change::RiskPerTradePct(v) => Some((CappedField::RiskPerTrade, *v)),
            Change::DailyLossLimitPct(v) => Some((CappedField::DailyLossLimit, *v)),
            Change::MaxDrawdownPct(v) => Some((CappedField::MaxDrawdown, *v)),
            Change::SymbolWeightPct(v) => Some((CappedField::SymbolWeight, *v)),
            Change::ConvictionWeightPct(v) => v.map(|v| (CappedField::ConvictionWeight, v)),
            Change::SectorWeightPct(v) => Some((CappedField::SectorWeight, *v)),
            _ => None,
        }
    }

    /// 이 칸을 `p` 에 적용한다.
    fn apply_to(&self, p: &mut RiskParams) {
        match self.clone() {
            Change::RiskPerTradePct(v) => p.risk_per_trade_pct = v,
            Change::DailyLossLimitPct(v) => p.daily_loss_limit_pct = v,
            Change::MaxDrawdownPct(v) => p.max_drawdown_pct = v,
            Change::LossStreak(v) => p.loss_streak = v,
            Change::MaxPositions(v) => p.max_positions = v,
            Change::SymbolWeightPct(v) => p.symbol_weight_pct = v,
            Change::ConvictionWeightPct(v) => p.conviction_weight_pct = v,
            Change::SectorWeightPct(v) => p.sector_weight_pct = v,
            Change::MinCashPct(v) => p.min_cash_pct = v,
            Change::Stop(v) => p.stop = v,
            Change::TakeProfit(v) => p.take_profit = v,
            Change::MaxHolding(v) => p.max_holding = v,
            Change::EntrySignals(v) => p.entry_signals = v,
            Change::Universe(v) => p.universe = v,
            Change::AutoOrderLimitPct(v) => p.auto_order_limit_pct = v,
            Change::TradingWindow(v) => p.trading_window = v,
            Change::ChaseLimitPct(v) => p.chase_limit_pct = v,
            Change::MaxTradesPerDay(v) => p.max_trades_per_day = v,
            Change::Triggers(v) => p.triggers = v,
            Change::Rebalance(v) => p.rebalance = v,
            Change::ApprovalWait(v) => p.approval_wait = v,
            Change::UnfilledCancel(v) => p.unfilled_cancel = v,
            Change::Promotion(v) => p.promotion = v,
        }
    }
}
