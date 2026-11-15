//! 절대 상한과 값 맞추기(`dev.md` C14 · E5 · V28, `principles.md` P1 · P3 · P4 · P8).

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::error::OverCap;
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
pub fn adjust(current: &RiskParams, changes: &[Change]) -> Result<RiskParams, OverCap> {
    let _ = (current, changes);
    todo!()
}
