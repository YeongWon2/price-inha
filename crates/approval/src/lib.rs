//! 승인 관리(A7, T14).
//!
//! 위험을 키우는 행동은 건마다 사람이 승인하고(P18), 승인 확인 실패 · 모르는 답은 거절이며(P19),
//! 승인 경로가 막히면 더 보수적으로 처리한다(P20). 저장은 `pi-store`의 승인 표를 쓴다.

mod approvals;
mod error;
mod subject;

pub use approvals::{
    Answer, ApprovalNumber, ApprovalView, Approvals, Ask, Clock, Decision, DueAction, DueOutcome,
    Notice, Requested, WaitTimes,
};
pub use error::{Error, RejectReason};
pub use subject::{
    Fingerprint, OrderSubject, OrderType, Side, SignalKind, Subject, canonical_json, fingerprint,
};
