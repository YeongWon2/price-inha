//! 팀 방 멘션 길 · 이어 말하기(6차). `Office`의 안쪽 칸을 그대로 쓰는 `impl Office` 조각이다.

use super::Office;
use crate::error::Error;
use crate::mention::{Member, Relayed, TeamPost};

impl Office {
    /// 팀 방에 사용자 글이 왔다(C18). 멘션한 담당만 답한다(원본 오피스 방 정책 mention).
    ///
    /// - 멘션한 담당마다 응답 객체를 하나씩 queued로 만들어 `office.chat`으로 낸다(지금의 대기열 · 상태 규칙 그대로).
    ///   `responders`는 멘션 순서.
    /// - 멘션이 없으면 아무도 답하지 않는다: 응답 0, `hint`는 [`crate::TeamHint::NoMention`](E44). 오류가 아니다(글은 남는다).
    /// - 멘션이 명단과 하나도 맞지 않으면 응답 0, `hint`는 [`crate::TeamHint::NoMatch`](E43).
    /// - 글이 비면 [`Error::EmptyMessage`].
    /// - 이 글(`source_message_id`)의 이어 말하기 예산을 [`crate::RELAY_BUDGET`]으로 채운다.
    pub async fn post_team(
        &self,
        text: &str,
        source_message_id: &str,
        roster: &[Member],
    ) -> Result<TeamPost, Error> {
        let _ = (text, source_message_id, roster);
        todo!()
    }

    /// 생각 중인 응답을 끝내고(규칙은 [`Office::complete`]와 같다), 팀 방 답이면 답 속 멘션으로 이어 말하게 한다.
    ///
    /// - 답 속에 다른 담당 멘션이 있으면 그 담당의 응답 객체를 queued로 만든다(팀 방, 같은 사용자 글 번호).
    /// - 한 사용자 글에서 이어 말하기는 [`crate::RELAY_BUDGET`]번까지. 넘으면 더 만들지 않는다.
    /// - 자기 멘션, 그리고 같은 사용자 글에 아직 끝나지 않은 응답(queued · thinking)이 있는 담당은
    ///   무시하고 예산을 쓰지 않는다(원본: 이미 말하는 중인 NPC는 다시 부르지 않음).
    /// - 1:1 방의 답, 빈 답(failed)은 이어 말하지 않는다.
    pub async fn complete_with_relay(
        &self,
        request_id: &str,
        content: &str,
        roster: &[Member],
    ) -> Result<Relayed, Error> {
        let _ = (request_id, content, roster);
        todo!()
    }
}
