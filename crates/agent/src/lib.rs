//! pi-agent: docs/dev/modules/agent.md 를 따른다.
//!
//! 에이전트 실행(A4, T9). 담당 6명의 턴, 팀 논의 방, 점수 · 합의 점수, 팀장 판단, 주문 안.
//! AI는 `pi_ai::AiProvider`로, 기록은 `pi_store::Store`로, 도구는 [`Tools`] 포트로 받는다.
//! 비밀(`pi-secrets`)에는 기대지 않는다(P21). 나가는 글은 `pi_ai::Redactor`를 거친다(E38).

mod action;
mod discussion;
pub mod error;
mod floor;
mod opinion;
mod ports;
mod progress;
mod prompt;
mod role;
mod roomlog;
mod score;
mod team;
mod turn;
mod verdict;

pub use action::{ActionCall, Reply, find_object_with, parse_reply};
pub use discussion::{
    DataItem, Decision, DiscussionLimits, DiscussionOutcome, DiscussionRequest, WatchReason,
};
pub use error::{Error, SignalError, ToolError, TurnFailure};
pub use ports::{Clock, OrderDraft, SellDraft, Side, Tools};
pub use progress::{
    CallBudget, DiscussionOptions, NOTICE_SOURCE_NAME, NoticeNote, ProgressSignal, ProgressSink,
    Stage, TURN_RETRY_MAX, WRAP_UP_LINE,
};
pub use role::{
    DEFAULT_MAX_ITERATIONS, DEFAULT_MAX_TOKENS, Role, RoleConfig, TimeLimits, TurnMode,
};
pub use score::{SCORE_MAX, SCORE_MIN, consensus, exceeds_threshold, parse_score};
pub use team::Team;
pub use turn::{
    EMPTY_MARK, MAX_EMPTY_REPLIES, MAX_MALFORMED_REPLIES, MAX_UNKNOWN_TOOL_REPLIES, Ports,
    TurnOutcome, TurnRunner,
};
