//! 팀 방 멘션(6차, 결정 17 · C18 · E43 · E44 · N11).
//!
//! 원본 DeskRPG와 같은 동작: 글 속 `@[이름]`(이름 속 `\` `[` `]`는 `\`로 가림)을 찾아 담당으로 바꾼다.
//! 표시 이름 **정확 일치**만, 같은 담당은 한 번, 명단에 없는 이름은 무시, 순서는 글에 나온 순서.
//! 팀 방(원본 오피스 방, 정책 mention)에서는 멘션한 담당만 답하고, 멘션이 없으면 아무도 답하지 않는다.

use crate::chat::ChatResponse;
use crate::place::Agent;

/// 한 사용자 글에서 담당끼리 이어 말하기를 할 수 있는 횟수(원본 `ChatQuota` 6, N11).
/// 사람이 팀 방에 글을 쓸 때마다 다시 6으로 찬다.
pub const RELAY_BUDGET: usize = 6;

/// 명단 한 줄: 담당과 화면에 보이는 이름.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Member {
    /// 담당.
    pub agent: Agent,
    /// 표시 이름. 멘션은 이 이름과 정확히 같아야 한다.
    pub name: String,
}

/// 글 속 `@[이름]`의 이름들(가림 `\`을 푼 것). 글에 나온 순서이고 같은 이름은 한 번만.
/// 괄호 없는 `@이름`은 멘션이 아니다.
pub fn mentioned_names(text: &str) -> Vec<String> {
    let _ = text;
    todo!()
}

/// 글 속 멘션을 명단의 담당으로 바꾼다.
///
/// - 표시 이름과 **정확히 같은** 것만(부분 일치 · 대소문자 다름은 아님).
/// - 같은 담당을 여러 번 불러도 한 번. 명단에 없는 이름은 무시한다.
/// - 순서는 글에 처음 나온 순서.
pub fn parse_mentions(text: &str, roster: &[Member]) -> Vec<Agent> {
    let _ = (text, roster);
    todo!()
}

/// 이름을 멘션 글자로 바꾼다: `@[이름]`, 이름 속 `\` `[` `]` 앞에 `\`를 붙인다(원본 직렬화).
pub fn mention_tag(name: &str) -> String {
    let _ = name;
    todo!()
}

/// 팀 방 글에 아무도 답하지 않은 까닭(화면 안내용).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum TeamHint {
    /// 멘션이 없다(E44). 글은 남고 "@로 담당을 부르거나 공지로 보내세요"를 안내한다. 이름 `"no_mention"`.
    NoMention,
    /// 멘션은 있었는데 명단의 이름과 맞는 것이 없다(E43). 보낸 사람에게만 안내한다. 이름 `"no_match"`.
    NoMatch,
}

impl TeamHint {
    /// 응답에 쓰는 이름.
    pub fn as_str(self) -> &'static str {
        match self {
            TeamHint::NoMention => "no_mention",
            TeamHint::NoMatch => "no_match",
        }
    }
}

/// 팀 방 글을 받은 결과(C18).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TeamPost {
    /// 답할 담당들(멘션 순서).
    pub responders: Vec<Agent>,
    /// 답할 담당마다 하나씩 만든 응답 객체(queued, 팀 방). `responders`와 같은 순서.
    pub responses: Vec<ChatResponse>,
    /// 아무도 답하지 않을 때의 까닭. 답할 담당이 있으면 `None`.
    pub hint: Option<TeamHint>,
}

/// 팀 방 답을 끝낸 결과: 끝난 응답과 답 속 멘션으로 이어 말하게 된 응답들.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Relayed {
    /// 끝난 응답(complete 또는 빈 답이면 failed).
    pub response: ChatResponse,
    /// 이어 말하기로 새로 만든 응답 객체(queued, 팀 방, 같은 사용자 글 번호). 멘션 순서.
    pub follow_ups: Vec<ChatResponse>,
}
