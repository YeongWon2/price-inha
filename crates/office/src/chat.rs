//! 채팅 응답 객체(C5 · D20).
//!
//! 원본 DeskRPG `ChatResponse`와 같은 수명주기: queued → thinking → complete | failed.
//! 1차는 스트리밍 없음(결정 2026-11-16 13번). 상태가 바뀔 때마다 객체 전체를 이벤트로 낸다.

use pi_store::UnixMillis;

use crate::place::Agent;

/// 대화 자리: 팀 방 하나와 담당별 1:1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Room {
    /// 팀 방. 이벤트 이름 `"team"`.
    Team,
    /// 그 담당과의 1:1. 이벤트 이름 `"direct"`.
    Direct,
}

/// 응답의 단계.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ChatStatus {
    /// 대기열에 들어감. 이벤트 이름 `"queued"`.
    Queued,
    /// 차례가 와서 생각 중. 이벤트 이름 `"thinking"`.
    Thinking,
    /// 답이 나옴(끝). 이벤트 이름 `"complete"`.
    Complete,
    /// 답하지 못함(끝, E6). 이벤트 이름 `"failed"`.
    Failed,
}

impl ChatStatus {
    /// 이벤트에 쓰는 이름.
    pub fn as_str(self) -> &'static str {
        todo!()
    }

    /// 끝난 단계인가(complete · failed). 끝난 응답은 다시 바뀌지 않는다.
    pub fn is_terminal(self) -> bool {
        todo!()
    }
}

/// 응답 객체. 이벤트 `office.chat`의 내용은 이 객체 전체다
/// (`requestId`, `sourceMessageId`, `agent`, `room`, `status`, `content`, `updatedAt`, `error`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatResponse {
    /// 서버가 붙인 응답 번호. 모든 응답에서 다르다.
    pub request_id: String,
    /// 화면이 만든 사용자 글 번호. 답을 그 글 뒤에 붙이는 데 쓴다.
    pub source_message_id: String,
    /// 답하는 담당.
    pub agent: Agent,
    /// 대화 자리.
    pub room: Room,
    /// 단계.
    pub status: ChatStatus,
    /// 답 본문. complete 전에는 빈 글.
    pub content: String,
    /// 마지막으로 바뀐 시각. 한 응답 안에서 바뀔 때마다 반드시 커진다.
    pub updated_at: UnixMillis,
    /// 실패 까닭(failed일 때만). 빈 답이면 `"empty_response"`.
    pub error: Option<String>,
}
