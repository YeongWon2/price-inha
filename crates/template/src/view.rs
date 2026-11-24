//! 템플릿 화면이 쓰는 읽기 · 판정 함수(12차, `additions.md` C28 · E58 · T37, `spec.md` R10-1 · R10-2 · R11-2 · F6).
//!
//! 순수 계산이다. 저장소를 읽지 않는다(저장은 store, 화면 · 경로는 server가 한다).
//! 원본에 없는 우리 고유 기능이다(스펙 R10-1 · R10-2 · R11-2 · F6-5a1 · F6-9a, 결정 17).

use rust_decimal::Decimal;
use serde_json::Value;

use crate::caps::AbsoluteCaps;
use crate::params::RiskParams;
use crate::promotion::{Criterion, PromotionCriteria, PromotionStats};
use crate::risk::{RiskField, widened_fields};

/// 칸 이름과 지금 값(json). 보기 칸 순서 그대로다([`template_view`] · [`diff_params`] 가 함께 쓴다).
fn field_values(p: &RiskParams) -> Vec<(&'static str, Value)> {
    fn v<T: serde::Serialize>(x: &T) -> Value {
        serde_json::to_value(x).unwrap_or(Value::Null)
    }
    vec![
        ("risk_per_trade_pct", v(&p.risk_per_trade_pct)),
        ("daily_loss_limit_pct", v(&p.daily_loss_limit_pct)),
        ("max_drawdown_pct", v(&p.max_drawdown_pct)),
        ("symbol_weight_pct", v(&p.symbol_weight_pct)),
        ("conviction_weight_pct", v(&p.conviction_weight_pct)),
        ("sector_weight_pct", v(&p.sector_weight_pct)),
        ("auto_order_limit_pct", v(&p.auto_order_limit_pct)),
        ("min_cash_pct", v(&p.min_cash_pct)),
        ("max_positions", v(&p.max_positions)),
        ("max_trades_per_day", v(&p.max_trades_per_day)),
        ("loss_streak", v(&p.loss_streak)),
        ("stop.fixed_loss_pct", v(&p.stop.fixed_loss_pct)),
        ("stop.atr", v(&p.stop.atr)),
        ("stop.thesis_break", v(&p.stop.thesis_break)),
        ("stop.flatten_before_close", v(&p.stop.flatten_before_close)),
        ("max_holding", v(&p.max_holding)),
        ("chase_limit_pct", v(&p.chase_limit_pct)),
        ("promotion", v(&p.promotion)),
        ("universe", v(&p.universe)),
        ("trading_window", v(&p.trading_window)),
        ("entry_signals", v(&p.entry_signals)),
        ("triggers", v(&p.triggers)),
        ("rebalance", v(&p.rebalance)),
        ("take_profit", v(&p.take_profit)),
        ("approval_wait", v(&p.approval_wait)),
        ("unfilled_cancel", v(&p.unfilled_cancel)),
    ]
}

/// 칸의 절대 상한(상한이 걸린 여섯 칸만).
fn cap_of(key: &str) -> Option<Decimal> {
    let caps = AbsoluteCaps::FIXED;
    match key {
        "risk_per_trade_pct" => Some(caps.risk_per_trade_pct),
        "daily_loss_limit_pct" => Some(caps.daily_loss_pct),
        "max_drawdown_pct" => Some(caps.max_drawdown_pct),
        "symbol_weight_pct" | "conviction_weight_pct" => Some(caps.symbol_weight_pct),
        "sector_weight_pct" => Some(caps.sector_weight_pct),
        _ => None,
    }
}

/// 넓힘 표의 칸에 대응하는 보기 칸 이름.
fn key_of(field: RiskField) -> &'static str {
    match field {
        RiskField::RiskPerTrade => "risk_per_trade_pct",
        RiskField::DailyLossLimit => "daily_loss_limit_pct",
        RiskField::MaxDrawdown => "max_drawdown_pct",
        RiskField::SymbolWeight => "symbol_weight_pct",
        RiskField::ConvictionWeight => "conviction_weight_pct",
        RiskField::SectorWeight => "sector_weight_pct",
        RiskField::AutoOrderLimit => "auto_order_limit_pct",
        RiskField::MinCash => "min_cash_pct",
        RiskField::MaxPositions => "max_positions",
        RiskField::MaxTradesPerDay => "max_trades_per_day",
        RiskField::LossStreak => "loss_streak",
        RiskField::FixedLossStop => "stop.fixed_loss_pct",
        RiskField::AtrStop => "stop.atr",
        RiskField::ThesisBreakStop => "stop.thesis_break",
        RiskField::FlattenBeforeClose => "stop.flatten_before_close",
        RiskField::MaxHolding => "max_holding",
        RiskField::ChaseLimit => "chase_limit_pct",
        RiskField::Promotion => "promotion",
    }
}

/// 템플릿 보기의 칸 하나.
///
/// 칸 이름([`Self::key`])은 안정된 영문 키다. [`RiskParams`] 의 칸 이름을 그대로 쓰되,
/// 손절은 네 칸으로 쪼개 점으로 잇는다: `stop.fixed_loss_pct` · `stop.atr` · `stop.thesis_break` ·
/// `stop.flatten_before_close`. 순서는 지시서 9차 (b) 4 표의 순서([`template_view`] 에 적었다).
#[derive(Debug, Clone, PartialEq)]
pub struct ViewField {
    /// 칸 이름(안정된 영문 키).
    pub key: &'static str,
    /// 지금 값. 그 칸의 값을 `serde_json::to_value` 로 낸 것과 같다(없는 값은 `null`).
    pub value: Value,
    /// 절대 상한(퍼센트). 상한이 걸린 칸만 `Some` 이다([`crate::AbsoluteCaps::FIXED`]).
    pub cap: Option<Decimal>,
    /// 바꿀 수 있는가. 상한 안에서 [`crate::adjust`] 로 바꿀 수 있는 칸이면 `true` 다
    /// (상한 자체는 칸이 아니다).
    pub editable: bool,
}

/// 템플릿 값을 화면이 그릴 칸 목록으로 낸다(R10-1 · C28).
///
/// 순서(26칸): `risk_per_trade_pct` · `daily_loss_limit_pct` · `max_drawdown_pct` · `symbol_weight_pct` ·
/// `conviction_weight_pct` · `sector_weight_pct` · `auto_order_limit_pct` · `min_cash_pct` · `max_positions` ·
/// `max_trades_per_day` · `loss_streak` · `stop.fixed_loss_pct` · `stop.atr` · `stop.thesis_break` ·
/// `stop.flatten_before_close` · `max_holding` · `chase_limit_pct` · `promotion` · `universe` ·
/// `trading_window` · `entry_signals` · `triggers` · `rebalance` · `take_profit` · `approval_wait` ·
/// `unfilled_cancel`.
///
/// 절대 상한은 여섯 칸에만 있다: R 2% · 일일 손실 5% · 최대 낙폭 20% · 종목 25% · 확신 종목 25% · 업종 50%.
pub fn template_view(params: &RiskParams) -> Vec<ViewField> {
    field_values(params)
        .into_iter()
        .map(|(key, value)| ViewField {
            key,
            value,
            cap: cap_of(key),
            editable: true,
        })
        .collect()
}

/// 승격 기준 하나의 진행([`promotion_lines`] 의 한 줄, E26 · E58).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CriterionLine {
    /// 기준.
    pub criterion: Criterion,
    /// 필요한 값: 매매 건수 · 거래일 수 · 비용 뺀 손익 기준 · 최대 낙폭 기준(퍼센트) · 표본 밖 손익 기준.
    pub need: Decimal,
    /// 지금 값. 표본 밖 손익이 아직 없으면 `None` 이다(나머지는 언제나 `Some`).
    pub have: Option<Decimal>,
    /// 채웠는가. [`PromotionCriteria::evaluate`] 의 판정과 같은 답이다
    /// (손익 기준은 **커야**, 낙폭 기준은 **이하**여야 채운 것).
    pub met: bool,
}

/// 기준별 진행 목록을 낸다(R10-2 · E58). [`Criterion`] 순서이고, 표본 밖 기준이 없으면
/// (`out_of_sample_pnl_above` 가 `None`) 그 줄은 내지 않는다. 채운 기준도 줄에 든다(`met`).
pub fn promotion_lines(criteria: &PromotionCriteria, stats: &PromotionStats) -> Vec<CriterionLine> {
    let mut lines = vec![
        CriterionLine {
            criterion: Criterion::Trades,
            need: Decimal::from(criteria.min_trades),
            have: Some(Decimal::from(stats.trades)),
            met: stats.trades >= criteria.min_trades,
        },
        CriterionLine {
            criterion: Criterion::TradingDays,
            need: Decimal::from(criteria.min_trading_days),
            have: Some(Decimal::from(stats.trading_days)),
            met: stats.trading_days >= criteria.min_trading_days,
        },
        CriterionLine {
            criterion: Criterion::NetPnl,
            need: criteria.net_pnl_above,
            have: Some(stats.net_pnl),
            met: stats.net_pnl > criteria.net_pnl_above,
        },
        CriterionLine {
            criterion: Criterion::MaxDrawdown,
            need: criteria.max_drawdown_pct,
            have: Some(stats.max_drawdown_pct),
            met: stats.max_drawdown_pct <= criteria.max_drawdown_pct,
        },
    ];
    if let Some(above) = criteria.out_of_sample_pnl_above {
        lines.push(CriterionLine {
            criterion: Criterion::OutOfSample,
            need: above,
            have: stats.out_of_sample_pnl,
            met: stats.out_of_sample_pnl.is_some_and(|pnl| pnl > above),
        });
    }
    lines
}

/// 두 판 사이에서 바뀐 칸 하나(R11-2 · F6-5a1).
#[derive(Debug, Clone, PartialEq)]
pub struct FieldChange {
    /// 칸 이름([`ViewField::key`] 와 같은 키).
    pub key: &'static str,
    /// 앞 값([`ViewField::value`] 와 같은 모양).
    pub before: Value,
    /// 뒤 값.
    pub after: Value,
    /// 넓힘인가. [`crate::widened_fields`] 가 그 칸을 넓힘으로 센 것과 같은 답이다.
    pub widened: bool,
}

/// 두 [`RiskParams`] 의 바뀐 칸(값이 다르거나 넓힘으로 센 칸)만 낸다(판 이력 화면용). 순서는 [`template_view`] 와 같다.
/// 같은 값이면 빈 목록이다.
pub fn diff_params(before: &RiskParams, after: &RiskParams) -> Vec<FieldChange> {
    let widened: Vec<&'static str> = widened_fields(before, after)
        .into_iter()
        .map(key_of)
        .collect();
    field_values(before)
        .into_iter()
        .zip(field_values(after))
        .map(|((key, b), (_, a))| FieldChange {
            key,
            before: b,
            after: a,
            widened: widened.contains(&key),
        })
        // 값이 달라졌거나 넓힘으로 센 칸만 남긴다. 확신 종목 비중이 `null` 인 채로 종목 비중이 올라 실효 값이
        // 커진 경우는 칸의 값은 같아도 넓힘이다(`widened_fields` 와 같은 답).
        .filter(|c| c.before != c.after || c.widened)
        .collect()
}
