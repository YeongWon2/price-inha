//! 위험을 키우는 변경의 판정(9차, `principles.md` P15 · `dev.md` E5 · E28 · `spec.md` F5 7a · F6 3).
//!
//! P15: 위험을 키우는 변경은 승인 번호와 냉각 기간 뒤(다음 거래일부터), 줄이는 변경은 바로 적용한다.
//! 이 모듈은 **판정과 표만** 둔다(순수 계산). 승인 · 저장 · 냉각 · 다음 거래일 반영은
//! store · approval · journal · server 가 한다. 절대 상한은 승인이 있어도 넘지 않는다([`crate::adjust`]).
//!
//! 원본에 없는 우리 고유 기능이다(사용자 결정 2026-11-21 9번 · 13번, PRD R14-1).
//! "줄이는 쪽은 바로, 키우는 쪽은 더 확인"의 비대칭은 `docs/research/remote-access-security.md` 5.3.

use serde::{Deserialize, Serialize};

use rust_decimal::Decimal;

use crate::caps::{Change, adjust};
use crate::error::OverCap;
use crate::params::{AtrBar, AtrStop, Cooldown, LossStreakStop, MaxHolding, PctRange, RiskParams};
use crate::promotion::PromotionCriteria;

/// 위험을 키울 수 있는 칸. 순서는 지시서 9차 (b) 4 표의 순서이며 [`widened_fields`] 가 이 순서로 준다.
///
/// 표에서 "키우지 않는 것으로 본다"인 칸(유니버스 · 거래 시간대 · 진입 신호 · 트리거 · 리밸런싱 ·
/// 익절 · 승인 대기 · 미체결 취소)은 여기 없다.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskField {
    /// [`RiskParams::risk_per_trade_pct`] 오름(P1).
    RiskPerTrade,
    /// [`RiskParams::daily_loss_limit_pct`] 오름(P3).
    DailyLossLimit,
    /// [`RiskParams::max_drawdown_pct`] 오름(P4).
    MaxDrawdown,
    /// [`RiskParams::symbol_weight_pct`] 오름(P8).
    SymbolWeight,
    /// [`RiskParams::conviction_weight_pct`] 의 `unwrap_or(종목 비중)` 이 오름.
    ConvictionWeight,
    /// [`RiskParams::sector_weight_pct`] 오름(P8).
    SectorWeight,
    /// [`RiskParams::auto_order_limit_pct`] 오름(승인 없이 나가는 금액이 커짐).
    AutoOrderLimit,
    /// [`RiskParams::min_cash_pct`] 내림.
    MinCash,
    /// [`RiskParams::max_positions`] 의 위 끝 오름.
    MaxPositions,
    /// [`RiskParams::max_trades_per_day`] 오름.
    MaxTradesPerDay,
    /// [`RiskParams::loss_streak`] 없어짐 · 정지까지의 손실 횟수 오름 · 쉬는 기간 줄어듦
    /// (그날만 < 거래일 n일, n 이 작을수록 짧다).
    LossStreak,
    /// `stop.fixed_loss_pct` 없어짐 · 아래 끝 내려감 · 위 끝 오름.
    FixedLossStop,
    /// `stop.atr` 없어짐 · 배수 오름.
    AtrStop,
    /// `stop.thesis_break`(논리 손절)가 `true` 에서 `false` 로. 추정(손절을 넓힘) · 사용자 확인 대상.
    ThesisBreakStop,
    /// `stop.flatten_before_close`(장 마감 청산)가 `true` 에서 `false` 로. 추정 · 사용자 확인 대상.
    FlattenBeforeClose,
    /// [`RiskParams::max_holding`] 길어짐(`SameDay` < `TradingDays(n)` < `Unlimited`, n 이 커짐).
    /// 추정(시간 손절) · 사용자 확인 대상.
    MaxHolding,
    /// [`RiskParams::chase_limit_pct`] 오름 · 정해져 있던 것이 없어짐. 추정 · 사용자 확인 대상.
    ChaseLimit,
    /// [`RiskParams::promotion`] 이 느슨해짐: `min_trades` · `min_trading_days` · `net_pnl_above` 내림,
    /// `out_of_sample_pnl_above` 내림 · 있던 것이 `None` 으로. 추정(실전에 더 쉽게 오름, P14) · 사용자 확인 대상.
    Promotion,
}

/// [`plan_adjust`] 의 결과.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdjustPlan {
    /// 위험을 키운 칸이 없다(줄이거나 같거나 위험과 무관한 칸만 바뀜) → 바로 적용한다.
    ApplyNow(RiskParams),
    /// 위험을 키운 칸이 하나라도 있다 → **묶음 전체가** 승인 대기(P15, 결정 2026-11-21 9번 · 13번).
    NeedsApproval {
        /// 승인되면 반영할 값(= [`crate::adjust`] 의 결과).
        after: RiskParams,
        /// 위험을 키운 칸들([`RiskField`] 순서, 겹치지 않게).
        widened: Vec<RiskField>,
    },
}

/// `before` → `after` 에서 위험을 키운 칸들. [`RiskField`] 순서, 겹치지 않게.
/// 키운 칸이 없으면 빈 목록이다.
pub fn widened_fields(before: &RiskParams, after: &RiskParams) -> Vec<RiskField> {
    let conviction = |p: &RiskParams| p.conviction_weight_pct.unwrap_or(p.symbol_weight_pct);
    let checks = [
        (
            RiskField::RiskPerTrade,
            after.risk_per_trade_pct > before.risk_per_trade_pct,
        ),
        (
            RiskField::DailyLossLimit,
            after.daily_loss_limit_pct > before.daily_loss_limit_pct,
        ),
        (
            RiskField::MaxDrawdown,
            after.max_drawdown_pct > before.max_drawdown_pct,
        ),
        (
            RiskField::SymbolWeight,
            after.symbol_weight_pct > before.symbol_weight_pct,
        ),
        (
            RiskField::ConvictionWeight,
            conviction(after) > conviction(before),
        ),
        (
            RiskField::SectorWeight,
            after.sector_weight_pct > before.sector_weight_pct,
        ),
        (
            RiskField::AutoOrderLimit,
            after.auto_order_limit_pct > before.auto_order_limit_pct,
        ),
        (RiskField::MinCash, after.min_cash_pct < before.min_cash_pct),
        (
            RiskField::MaxPositions,
            after.max_positions.high > before.max_positions.high,
        ),
        (
            RiskField::MaxTradesPerDay,
            after.max_trades_per_day > before.max_trades_per_day,
        ),
        (
            RiskField::LossStreak,
            streak_loosened(before.loss_streak, after.loss_streak),
        ),
        (
            RiskField::FixedLossStop,
            fixed_stop_widened(before.stop.fixed_loss_pct, after.stop.fixed_loss_pct),
        ),
        (
            RiskField::AtrStop,
            atr_stop_widened(before.stop.atr, after.stop.atr),
        ),
        (
            RiskField::ThesisBreakStop,
            before.stop.thesis_break && !after.stop.thesis_break,
        ),
        (
            RiskField::FlattenBeforeClose,
            before.stop.flatten_before_close && !after.stop.flatten_before_close,
        ),
        (
            RiskField::MaxHolding,
            holding_rank(after.max_holding) > holding_rank(before.max_holding),
        ),
        (
            RiskField::ChaseLimit,
            chase_widened(before.chase_limit_pct, after.chase_limit_pct),
        ),
        (
            RiskField::Promotion,
            promotion_loosened(&before.promotion, &after.promotion),
        ),
    ];
    checks
        .into_iter()
        .filter_map(|(field, widened)| widened.then_some(field))
        .collect()
}

/// 연속 손실 정지가 느슨해졌는가: 없어졌거나, 정지까지의 손실 횟수가 늘었거나, 쉬는 기간이 줄었다.
fn streak_loosened(before: Option<LossStreakStop>, after: Option<LossStreakStop>) -> bool {
    match (before, after) {
        (Some(_), None) => true,
        (Some(b), Some(a)) => a.losses > b.losses || rest_days(a.cooldown) < rest_days(b.cooldown),
        _ => false,
    }
}

/// 쉬는 기간의 길이 순서: 그날만(0) < 거래일 n일(n).
fn rest_days(cooldown: Cooldown) -> u32 {
    match cooldown {
        Cooldown::RestOfDay => 0,
        Cooldown::TradingDays(days) => days,
    }
}

/// 고정 손절이 넓어졌는가: 없어졌거나, 아래 끝이 내려갔거나, 위 끝이 올라갔다.
fn fixed_stop_widened(before: Option<PctRange>, after: Option<PctRange>) -> bool {
    match (before, after) {
        (Some(_), None) => true,
        (Some(b), Some(a)) => a.low < b.low || a.high > b.high,
        _ => false,
    }
}

/// ATR 손절이 넓어졌는가: 없어졌거나, 배수가 커졌거나, 봉 단위가 길어졌다(분봉 → 일봉).
///
/// 같은 배수라도 일봉 ATR은 분봉 ATR보다 훨씬 크므로 봉 단위가 길어지면 배수가 줄어도 넓힘으로 본다(11차).
fn atr_stop_widened(before: Option<AtrStop>, after: Option<AtrStop>) -> bool {
    match (before, after) {
        (Some(_), None) => true,
        (Some(b), Some(a)) => a.multiple > b.multiple || bar_rank(a.bar) > bar_rank(b.bar),
        _ => false,
    }
}

/// 봉 단위의 길이 순서: 분봉 < 일봉.
fn bar_rank(bar: AtrBar) -> u8 {
    match bar {
        AtrBar::Minute => 0,
        AtrBar::Day => 1,
    }
}

/// 최대 보유 기간의 길이 순서: 그날만 < 거래일 n일(n 이 클수록 김) < 정하지 않음.
fn holding_rank(holding: MaxHolding) -> (u8, u32) {
    match holding {
        MaxHolding::SameDay => (0, 0),
        MaxHolding::TradingDays(days) => (1, days),
        MaxHolding::Unlimited => (2, 0),
    }
}

/// 추격 폭이 넓어졌는가: 정해져 있던 것이 없어졌거나 값이 올랐다.
fn chase_widened(before: Option<Decimal>, after: Option<Decimal>) -> bool {
    match (before, after) {
        (Some(_), None) => true,
        (Some(b), Some(a)) => a > b,
        _ => false,
    }
}

/// 승격 기준이 느슨해졌는가: 매매 건수 · 거래일 · 비용 뺀 손익 기준이 내려갔거나,
/// 표본 밖 기준이 내려갔거나 없어졌다.
fn promotion_loosened(before: &PromotionCriteria, after: &PromotionCriteria) -> bool {
    after.min_trades < before.min_trades
        || after.min_trading_days < before.min_trading_days
        || after.net_pnl_above < before.net_pnl_above
        || match (
            before.out_of_sample_pnl_above,
            after.out_of_sample_pnl_above,
        ) {
            (Some(_), None) => true,
            (Some(b), Some(a)) => a < b,
            _ => false,
        }
}

/// 위험을 키우는 변경인가 = `!widened_fields(before, after).is_empty()`.
///
/// 넓힘으로 보는 것([`RiskField`] 칸마다): 1회 위험 · 일일 손실 · 최대 낙폭 · 종목 · 확신 종목 · 업종 비중 ·
/// 자동 주문 한도 · 최대 동시 보유 수 · 일일 최대 매매 횟수의 상향, 최소 현금의 감소, 연속 손실 정지의 해제 ·
/// 정지까지 손실 횟수 증가 · 쉬는 기간 단축, 고정 손절의 사라짐 · 끝 넓힘, ATR 손절의 사라짐 · 배수 증가 ·
/// 봉 단위 길어짐(분봉 → 일봉), 논리 손절 · 마감 전 청산 끄기, 최대 보유 기간 늘림, 추격 폭 넓힘 · 없앰,
/// 승격 기준 느슨하게. journal의 개선안 판정도 이 함수를 그대로 쓴다.
pub fn increases_risk(before: &RiskParams, after: &RiskParams) -> bool {
    !widened_fields(before, after).is_empty()
}

/// 조정을 바로 적용할지 승인 대기로 둘지 가른다(P15 · E5 · E28).
///
/// 1. 먼저 [`crate::adjust`] 를 부른다. 상한 · 아래쪽 경계를 어기면 그 [`OverCap`] 을 그대로 준다
///    (승인이 있어도 바뀌지 않는다).
/// 2. 통과하면 `widened_fields(current, &after)` 가 비었으면 [`AdjustPlan::ApplyNow`],
///    아니면 [`AdjustPlan::NeedsApproval`]. 키우는 칸과 줄이는 칸이 섞이면 묶음 전체가 승인 대기다.
///
/// 결과 값은 언제나 `adjust(current, changes)` 와 같다(판정만 더해진다). `current` 는 바뀌지 않는다.
pub fn plan_adjust(current: &RiskParams, changes: &[Change]) -> Result<AdjustPlan, OverCap> {
    let after = adjust(current, changes)?;
    let widened = widened_fields(current, &after);
    if widened.is_empty() {
        Ok(AdjustPlan::ApplyNow(after))
    } else {
        Ok(AdjustPlan::NeedsApproval { after, widened })
    }
}
