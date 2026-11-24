//! 템플릿 화면이 쓰는 읽기 · 판정 함수(12차, `additions.md` C28 · E58 · T37, `spec.md` R10-1 · R10-2 · R11-2 · F6).
//!
//! 순수 계산이다. 저장소를 읽지 않는다(저장은 store, 화면 · 경로는 server가 한다).
//! 원본에 없는 우리 고유 기능이다(스펙 R10-1 · R10-2 · R11-2 · F6-5a1 · F6-9a, 결정 17).

use rust_decimal::Decimal;
use serde_json::Value;

use crate::params::RiskParams;
use crate::promotion::{Criterion, PromotionCriteria, PromotionStats};

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
/// 순서(25칸): `risk_per_trade_pct` · `daily_loss_limit_pct` · `max_drawdown_pct` · `symbol_weight_pct` ·
/// `conviction_weight_pct` · `sector_weight_pct` · `auto_order_limit_pct` · `min_cash_pct` · `max_positions` ·
/// `max_trades_per_day` · `loss_streak` · `stop.fixed_loss_pct` · `stop.atr` · `stop.thesis_break` ·
/// `stop.flatten_before_close` · `max_holding` · `chase_limit_pct` · `promotion` · `universe` ·
/// `trading_window` · `entry_signals` · `triggers` · `rebalance` · `take_profit` · `approval_wait` ·
/// `unfilled_cancel`.
///
/// 절대 상한은 여섯 칸에만 있다: R 2% · 일일 손실 5% · 최대 낙폭 20% · 종목 25% · 확신 종목 25% · 업종 50%.
pub fn template_view(_params: &RiskParams) -> Vec<ViewField> {
    Vec::new()
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
pub fn promotion_lines(
    _criteria: &PromotionCriteria,
    _stats: &PromotionStats,
) -> Vec<CriterionLine> {
    Vec::new()
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

/// 두 [`RiskParams`] 의 바뀐 칸만 낸다(판 이력 화면용). 순서는 [`template_view`] 와 같다.
/// 같은 값이면 빈 목록이다.
pub fn diff_params(_before: &RiskParams, _after: &RiskParams) -> Vec<FieldChange> {
    Vec::new()
}
