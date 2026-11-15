//! pi-rules: 규칙 엔진. 구성 요소 A6. 순수 계산이다(파일 · 네트워크 · 시계 없음).
//!
//! 작업 지시서: `docs/dev/modules/rules.md`.
//!
//! - 주문 전 점검 [`check`](C9 · FN1)은 판단 상태 → 팀 상태 → 근거(P9) → 원칙 P1~P13 →
//!   주문 직전 점검(P12) → 금액 한도 순서로 본다.
//! - 수량은 [`size`]가 센다(P1 · P7 · P8). 템플릿 값은 믿지 않고 언제나
//!   [`pi_template::AbsoluteCaps::FIXED`]로 다시 막는다.
//! - 점검을 지난 주문만 [`issue`]로 점검 통과 표시를 받는다(E33). 승인을 기다리던 주문은
//!   승인 소비 뒤 [`approved`]로 다시 점검을 거쳐야 점검 통과가 된다.
//! - 손실과 멈춤(T13)은 [`day_loss_pct`] · [`drawdown_pct`] · [`evaluate_stops`]가 센다.
//!   전이를 실제로 하는 것은 store다.
//! - 시각은 언제나 인자로 받는다. 돈은 [`rust_decimal::Decimal`]로만 센다.

mod approval;
mod check;
pub mod error;
mod loss;
mod model;
mod outcome;
mod principles;
mod screen;
mod sizing;

pub use approval::{ApprovalGrant, approved};
pub use check::{check, issue};
pub use error::Error;
pub use loss::{StopContext, StopTransition, day_loss_pct, drawdown_pct, evaluate_stops};
pub use model::{
    ApiBudget, Book, CheckContext, Funding, Health, OrderDraft, Position, ProductKind, Purpose,
    Rationale, RationaleField, RulesConfig, SourceStamp, SymbolStatus, TeamStatus, TradeStage,
};
pub use outcome::{
    BlockKind, Blocked, Outcome, Passed, PendingApproval, Principle, Reason, Violation,
};
pub use principles::check_stop_change;
pub use sizing::{SizeRequest, align_down_to_tick, net_expected_value, size};
