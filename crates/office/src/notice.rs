//! 공지 나눠 주기(6차, 결정 17 · C19 · E45 · E46 · N18). 원본에 없는 우리 기능이다.
//!
//! 공지를 받으면 저장소에 공지를 만들고(담당 여섯 모두 기다림) 담당마다 확인 답 턴을 하나씩 대기열에 넣는다.
//! 공지는 규칙 · 템플릿 · 승인 · 팀 상태를 바꾸지 않는다(E46).

use pi_store::Notice;

use crate::chat::ChatResponse;
use crate::place::Agent;

/// 공지를 받은 순간 담당 한 명의 확인 턴 상태(C19 `acks[].state`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum NoticeTurn {
    /// 확인 답 턴이 대기열에 들어갔다. 이름 `"queued"`.
    Queued,
    /// 하루 AI 호출 상한에 걸려 턴을 주지 않았다(E45). 저장소 확인은 "상한으로 못 함". 이름 `"limit"`.
    Limit,
}

impl NoticeTurn {
    /// 응답에 쓰는 이름.
    pub fn as_str(self) -> &'static str {
        match self {
            NoticeTurn::Queued => "queued",
            NoticeTurn::Limit => "limit",
        }
    }
}

/// 담당 한 명의 확인 턴.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoticeAckTurn {
    /// 담당.
    pub agent: Agent,
    /// 턴 상태.
    pub state: NoticeTurn,
    /// 확인 답 응답 객체(queued, 팀 방, `notice`가 붙음). 상한이면 `None`.
    pub response: Option<ChatResponse>,
}

/// 공지를 받은 결과(C19).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoticePost {
    /// 저장소에 만든 공지(글은 가린 것).
    pub notice: Notice,
    /// 담당마다 확인 턴([`Agent::ALL`] 순서, 여섯).
    pub acks: Vec<NoticeAckTurn>,
}
