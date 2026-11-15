//! 손실과 멈춤(T13 · M1 · E23 · E24 · P3 · P4, 결정 31번).

use pi_template::{AbsoluteCaps, RiskParams};
use rust_decimal::Decimal;

use crate::{Book, TeamStatus};

/// 그날 손실 퍼센트: `max(0, (시작 평가액 − 지금 평가액) / 시작 평가액 × 100)`.
/// 지금 평가액에 실현 손익과 평가 손익이 함께 들어 있다. 통화마다 따로 센다.
///
/// 시작 평가액이 0 이하이면 잃을 것이 없어 0이다(돈이 없는 통화의 장부가 팀을 멈추지 않게).
pub fn day_loss_pct(book: &Book) -> Decimal {
    loss_from(book.day_start_equity, book.equity)
}

/// 최고 평가액 대비 낙폭 퍼센트: `max(0, (최고 평가액 − 지금 평가액) / 최고 평가액 × 100)`.
///
/// 최고 평가액이 0 이하이면 0이다.
pub fn drawdown_pct(book: &Book) -> Decimal {
    loss_from(book.peak_equity, book.equity)
}

/// `max(0, (기준 − 지금) × 100 / 기준)`. 기준이 0 이하이거나 셈이 넘치면 0이다.
fn loss_from(base: Decimal, now: Decimal) -> Decimal {
    if base <= Decimal::ZERO {
        return Decimal::ZERO;
    }
    (base - now)
        .checked_mul(Decimal::ONE_HUNDRED)
        .and_then(|scaled| scaled.checked_div(base))
        .map_or(Decimal::ZERO, |pct| pct.max(Decimal::ZERO))
}

/// 멈춤 판단의 입력.
#[derive(Debug, Clone)]
pub struct StopContext<'a> {
    /// 지금 팀 상태.
    pub team: TeamStatus,
    /// 템플릿 값. 한도는 [`pi_template::AbsoluteCaps::FIXED`]로 다시 막는다.
    pub params: &'a RiskParams,
    /// 통화별 장부. 통화끼리 더하지 않는다.
    pub books: &'a [Book],
}

/// store가 해야 할 팀 상태 전이.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopTransition {
    /// 하루 한도 멈춤으로(E23 · P3).
    DailyLimitHalt,
    /// 전체 정지로(E24 · P4).
    FullStop,
}

/// 요구할 전이. 어느 한 통화라도
///
/// - 낙폭이 `min(최대 낙폭 한도, 20)`에 **닿으면**(같거나 크면) [`StopTransition::FullStop`]
///   (이미 전체 정지면 `None`). 하루 한도와 함께 닿으면 전체 정지가 이긴다.
/// - 그날 손실이 `min(하루 손실 한도, 5)`에 닿으면 [`StopTransition::DailyLimitHalt`]
///   (이미 하루 한도 멈춤이나 전체 정지면 `None`).
///
/// 아무 데도 닿지 않으면 `None`. 전이는 store가 한다.
pub fn evaluate_stops(ctx: &StopContext<'_>) -> Option<StopTransition> {
    if drawdown_reached(ctx.params, ctx.books) {
        return (ctx.team != TeamStatus::FullStop).then_some(StopTransition::FullStop);
    }
    if day_loss_reached(ctx.params, ctx.books) {
        let already = matches!(ctx.team, TeamStatus::DailyLimitHalt | TeamStatus::FullStop);
        return (!already).then_some(StopTransition::DailyLimitHalt);
    }
    None
}

/// 하루 손실 한도: `min(템플릿 값, 5)`.
pub(crate) fn daily_loss_limit(params: &RiskParams) -> Decimal {
    params
        .daily_loss_limit_pct
        .min(AbsoluteCaps::FIXED.daily_loss_pct)
}

/// 최대 낙폭 한도: `min(템플릿 값, 20)`.
pub(crate) fn drawdown_limit(params: &RiskParams) -> Decimal {
    params
        .max_drawdown_pct
        .min(AbsoluteCaps::FIXED.max_drawdown_pct)
}

/// 어느 한 통화라도 그날 손실이 한도에 닿았는가(같거나 크다).
pub(crate) fn day_loss_reached(params: &RiskParams, books: &[Book]) -> bool {
    let limit = daily_loss_limit(params);
    books.iter().any(|book| day_loss_pct(book) >= limit)
}

/// 어느 한 통화라도 낙폭이 한도에 닿았는가(같거나 크다).
pub(crate) fn drawdown_reached(params: &RiskParams, books: &[Book]) -> bool {
    let limit = drawdown_limit(params);
    books.iter().any(|book| drawdown_pct(book) >= limit)
}
