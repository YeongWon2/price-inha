//! 절대 상한과 값 맞추기(`dev.md` C14 · E5 · V28, `principles.md` P1 · P3 · P4 · P8).

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::error::{OverCap, Violation};
use crate::params::{
    ApprovalWait, CountRange, EntrySignal, LossStreakStop, MaxHolding, Rebalance, RiskParams,
    StopRule, TakeProfitRule, TradingWindow, TriggerKind, UnfilledCancel, Universe,
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
    finish(violations)?;
    Ok(next)
}

/// 값 전체가 상한 안인지 본다(읽어 들인 템플릿 점검용). 넘은 칸을 [`CappedField`] 순서로 모두 담는다.
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
    finish(violations)
}

/// 넘은 칸이 있으면 [`CappedField`] 순서로 정렬해 오류로 낸다(같은 칸은 넣은 순서를 지킨다).
fn finish(mut violations: Vec<Violation>) -> Result<(), OverCap> {
    if violations.is_empty() {
        return Ok(());
    }
    violations.sort_by_key(|v| v.field.order());
    Err(OverCap {
        violations,
        invalid: Vec::new(),
    })
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
