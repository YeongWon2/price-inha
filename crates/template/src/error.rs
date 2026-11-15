//! 오류.

use std::fmt;

use rust_decimal::Decimal;
use thiserror::Error;

use crate::caps::CappedField;

/// 상한을 넘은 칸 하나.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Violation {
    /// 넘은 칸.
    pub field: CappedField,
    /// 넣으려던 값(퍼센트).
    pub requested: Decimal,
    /// 그 칸의 절대 상한(퍼센트).
    pub cap: Decimal,
}

/// [`crate::RiskParams`] 의 칸. 선언 순서는 `RiskParams` 칸 순서와 같다.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ParamField {
    /// `risk_per_trade_pct`.
    RiskPerTrade,
    /// `daily_loss_limit_pct`.
    DailyLossLimit,
    /// `max_drawdown_pct`.
    MaxDrawdown,
    /// `loss_streak`(연속 손절 횟수 · 쉬는 영업일).
    LossStreak,
    /// `max_positions`.
    MaxPositions,
    /// `symbol_weight_pct`.
    SymbolWeight,
    /// `conviction_weight_pct`.
    ConvictionWeight,
    /// `sector_weight_pct`.
    SectorWeight,
    /// `min_cash_pct`.
    MinCash,
    /// `stop`(고정 손실 폭 범위 등).
    Stop,
    /// `take_profit`.
    TakeProfit,
    /// `max_holding`.
    MaxHolding,
    /// `entry_signals`.
    EntrySignals,
    /// `universe`.
    Universe,
    /// `auto_order_limit_pct`.
    AutoOrderLimit,
    /// `trading_window`.
    TradingWindow,
    /// `chase_limit_pct`.
    ChaseLimit,
    /// `max_trades_per_day`.
    MaxTradesPerDay,
    /// `triggers`.
    Triggers,
    /// `rebalance`(주기 · 이탈 폭).
    Rebalance,
    /// `approval_wait`(N2).
    ApprovalWait,
    /// `unfilled_cancel`(N4).
    UnfilledCancel,
    /// `promotion`.
    Promotion,
}

/// 아래쪽 경계 · 모양 위반의 이유(2차 1 · 3번).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum InvalidReason {
    /// 0 이하(음수 · 0인 비율, 0인 개수 · 시간).
    NotPositive,
    /// 100%를 넘는 비율.
    Over100Pct,
    /// 범위의 아래 끝이 위 끝보다 크다(`PctRange` · `CountRange`).
    RangeReversed,
    /// 업종 비중이 종목 비중(확신 종목 비중 포함)보다 작다.
    SectorBelowSymbol,
}

/// 아래쪽 경계 · 모양을 어긴 칸 하나.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Invalid {
    /// 어긴 칸.
    pub field: ParamField,
    /// 이유.
    pub reason: InvalidReason,
}

/// 받을 수 없는 값(E5). 상한을 넘은 칸([`Self::violations`])과 아래쪽 경계 · 모양을 어긴 칸
/// ([`Self::invalid`])을 모두 담는다. 둘 가운데 하나는 비어 있지 않다.
///
/// 화면에 보이는 글에는 칸마다 상한이 `"<상한>%"` 모양으로 들어간다(V28: 3%를 넣으면 "2%"가 보인다).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OverCap {
    /// 상한을 넘은 칸들. [`CappedField`] 순서.
    pub violations: Vec<Violation>,
    /// 아래쪽 경계 · 모양을 어긴 칸들. [`ParamField`] 순서.
    pub invalid: Vec<Invalid>,
}

impl fmt::Display for OverCap {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("원칙 상한을 넘는 값이라 저장하지 않았습니다: ")?;
        for (i, v) in self.violations.iter().enumerate() {
            if i > 0 {
                f.write_str(", ")?;
            }
            write!(
                f,
                "{} {}% (상한 {}%)",
                v.field.label(),
                v.requested.normalize(),
                v.cap.normalize()
            )?;
        }
        Ok(())
    }
}

impl std::error::Error for OverCap {}

/// 이 모듈의 오류.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum Error {
    /// 원칙 상한을 넘는 값.
    #[error(transparent)]
    OverCap(#[from] OverCap),
    /// 모르는 판 번호.
    #[error("모르는 템플릿 판 번호: {found}")]
    UnsupportedVersion {
        /// 읽은 판 번호.
        found: u32,
    },
    /// JSON 모양이 틀림.
    #[error("템플릿 JSON 을 읽지 못함: {0}")]
    Json(String),
}
