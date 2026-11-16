//! 주문 실행(A8, T7). 승인된 주문을 규칙 · 승인 확인 뒤 모의계좌로 낸다.
//!
//! 작업 지시서: `docs/dev/modules/exec.md`.
//!
//! - 증권사는 자체 모의계좌([`pi_paper::PaperBroker`])뿐이다. 실제 증권사 구현은 없다(결정 22번).
//! - 실행하기 전에 먼저 저장하고, 저장에 실패하면 실행하지 않는다(persist-before-execute).
//! - 시각은 주입한 [`pi_market::Clock`]에서만 읽는다. 돈과 수량은 십진수다.

pub mod error;
mod executor;
mod grant;
mod keep;
mod track;

pub use error::Error;
pub use executor::{
    Executor, Guard, Reconciled, SELL_RETRY_FIRST, SELL_RETRY_MAX, SellPurpose, SellRetry,
    SellSubmission, Submission, TradeChange,
};
