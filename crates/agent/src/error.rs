//! 오류.

use pi_ai::AiError;
use pi_store::{StoreError, TradeId, TradeState};
use thiserror::Error;

use crate::role::Role;

/// 한 턴이 끝내 답을 내지 못한 까닭. 어느 것이든 그 담당은 응답 실패(E6)다.
///
/// 오류 상한은 원본(Hermes)의 숫자를 그대로 쓴다(`docs/research/agent-runtime-deep.md` 1장).
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[non_exhaustive]
pub enum TurnFailure {
    /// 공급자가 오류를 냈다(다시 시도는 `pi-ai`가 이미 했다).
    #[error("AI 공급자 오류: {0}")]
    Provider(AiError),
    /// 반복 상한([`crate::RoleConfig::max_iterations`])에 닿았다.
    #[error("반복 상한에 닿았습니다")]
    IterationLimit,
    /// 공급자 호출 하나가 무응답 시간([`crate::TimeLimits::idle`])을 넘었다.
    #[error("응답이 시간 안에 오지 않았습니다")]
    IdleTimeout,
    /// 턴 전체가 시간([`crate::TimeLimits::total`])을 넘었다.
    #[error("턴이 전체 시간을 넘었습니다")]
    TotalTimeout,
    /// 모르는 도구 이름만 부른 답이 연달아 [`crate::MAX_UNKNOWN_TOOL_REPLIES`]번 왔다.
    #[error("모르는 도구만 부르는 답이 되풀이됐습니다")]
    UnknownTools,
    /// 깨진 JSON 행동이 든 답이 연달아 [`crate::MAX_MALFORMED_REPLIES`]번 왔다.
    #[error("깨진 JSON 행동이 되풀이됐습니다")]
    MalformedAction,
    /// 빈 답이 연달아 [`crate::MAX_EMPTY_REPLIES`]번 왔다.
    #[error("빈 답이 되풀이됐습니다")]
    EmptyReply,
    /// 도구를 부르기 전에 메시지를 저장하지 못했다. 도구는 부르지 않았다.
    #[error("메시지를 저장하지 못해 도구를 부르지 않았습니다")]
    Persistence,
    /// 정해진 JSON(점수 · 판단 · 주문 안)을 답하지 않았다.
    #[error("정해진 모양의 답이 아닙니다")]
    InvalidStructuredReply,
}

/// 도구 포트가 낸 오류. 글은 그대로 담당에게 도구 결과로 돌려준다.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("도구 실패: {0}")]
pub struct ToolError(pub String);

/// 이 모듈의 오류.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum Error {
    /// 담당 응답 실패(E6).
    #[error("{role:?} 담당이 답하지 못했습니다: {cause}")]
    RoleUnavailable {
        /// 답하지 못한 담당.
        role: Role,
        /// 까닭.
        cause: TurnFailure,
    },
    /// 담당 설정이 6명을 한 번씩 담지 않았다.
    #[error("담당 설정은 6명을 한 번씩 담아야 합니다")]
    InvalidRoles,
    /// 논의 참가자가 2명보다 적거나 6명보다 많거나 겹치거나,
    /// 지명된 담당이 참가자가 아니다.
    #[error("논의 참가자가 올바르지 않습니다")]
    InvalidMembers,
    /// 판단됨이 아닌 매매로 주문 안을 내려 했다(E20).
    #[error("판단됨이 아닌 매매({trade:?}, {state:?})로는 주문 안을 내지 않습니다")]
    TradeNotJudged {
        /// 매매 번호.
        trade: TradeId,
        /// 그 매매의 지금 상태.
        state: TradeState,
    },
    /// 팔기로 판단된 매매로 사기 주문 안을 내려 했다(V4).
    #[error("팔기로 판단된 매매({trade:?})로는 사기 주문 안을 내지 않습니다")]
    TradeNotBuy {
        /// 매매 번호.
        trade: TradeId,
    },
    /// 팔기로 판단되지 않은(사기 · 방향 없음) 매매로 팔기 주문 안을 내려 했다(V4, 9차).
    #[error("팔기로 판단되지 않은 매매({trade:?})로는 팔기 주문 안을 내지 않습니다")]
    TradeNotSell {
        /// 매매 번호.
        trade: TradeId,
    },
    /// 주문 안을 도구 포트로 보내지 못했다.
    #[error("{0}")]
    Tool(#[from] ToolError),
    /// 저장소 오류.
    #[error("저장소 오류: {0}")]
    Store(#[from] StoreError),
}
