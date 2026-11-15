//! 손실과 멈춤(T13 · M1 · E23 · E24 · P3 · P4, 결정 31번).

use pi_template::RiskParams;
use rust_decimal::Decimal;

use crate::{Book, TeamStatus};

/// 그날 손실 퍼센트: `max(0, (시작 평가액 − 지금 평가액) / 시작 평가액 × 100)`.
/// 지금 평가액에 실현 손익과 평가 손익이 함께 들어 있다. 통화마다 따로 센다.
pub fn day_loss_pct(book: &Book) -> Decimal {
    let _ = book;
    todo!()
}

/// 최고 평가액 대비 낙폭 퍼센트: `max(0, (최고 평가액 − 지금 평가액) / 최고 평가액 × 100)`.
pub fn drawdown_pct(book: &Book) -> Decimal {
    let _ = book;
    todo!()
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
    let _ = ctx;
    todo!()
}
