//! 오류.

use thiserror::Error;

use crate::place::Agent;

/// 이 모듈의 오류.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum Error {
    /// 그 담당의 대기열이 찼다([`crate::CHAT_QUEUE_MAX`]). 원본 DeskRPG `SessionQueue(8)`의 "넘치면 거절".
    #[error("{agent:?} 담당의 대기열이 찼다")]
    QueueFull {
        /// 대기열이 찬 담당.
        agent: Agent,
    },
    /// 보낸 글이 비었다(앞뒤 공백을 떼면 아무것도 없다).
    #[error("보낸 글이 비었다")]
    EmptyMessage,
    /// 그런 응답 번호가 없다.
    #[error("응답 {0}이 없다")]
    UnknownRequest(String),
    /// 응답이 이미 끝났다(complete · failed). 끝난 응답은 바뀌지 않는다(D20).
    #[error("응답 {0}은 이미 끝났다")]
    Finished(String),
    /// 지금 상태에서 갈 수 없는 단계다(예: 아직 생각을 시작하지 않은 응답을 끝냄).
    #[error("응답 {0}은 지금 그 단계로 갈 수 없다")]
    InvalidTransition(String),
    /// 그 담당이 지금 미팅룸에서 논의 중이라 부르거나 멘션으로 불러올 수 없다(E47).
    /// 회의는 끊지 않는다.
    #[error("{agent:?} 담당은 지금 회의 중이다")]
    MeetingReserved {
        /// 회의 중인 담당.
        agent: Agent,
    },
    /// 이벤트를 내보내는 포트가 실패했다.
    #[error("이벤트를 내보내지 못했다: {0}")]
    Publish(#[from] SinkError),
}

/// 이벤트 포트([`crate::EventSink`])의 실패.
#[derive(Debug, Error)]
#[error("{message}")]
pub struct SinkError {
    /// 까닭.
    pub message: String,
}
