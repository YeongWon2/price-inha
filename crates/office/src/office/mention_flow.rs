//! 팀 방 멘션 길 · 이어 말하기(6차). `Office`의 안쪽 칸을 그대로 쓰는 `impl Office` 조각이다.

use std::collections::{HashMap, VecDeque};

use pi_store::NoticeId;

use super::Office;
use crate::chat::{ChatResponse, ChatStatus, Room};
use crate::error::Error;
use crate::mention::{Member, RELAY_BUDGET, Relayed, TeamHint, TeamPost};
use crate::mention::{mentioned_names, parse_mentions};
use crate::place::Agent;

/// 이어 말하기 예산을 기억해 두는 사용자 글 수. 오래된 글부터 잊는다(끝난 응답을 남기는 수와 같다).
const RELAY_KEEP: usize = 100;

/// 대기열이 차서 받지 못한 몫의 실패 까닭(서버의 `queue_full`과 같은 이름).
const QUEUE_FULL: &str = "queue_full";

/// 사용자 글마다 남은 이어 말하기 횟수(원본 `ChatQuota`).
#[derive(Debug, Default)]
pub(super) struct RelayBudgets {
    left: HashMap<String, usize>,
    /// 채운 순서. [`RELAY_KEEP`]를 넘으면 오래된 것부터 지운다.
    order: VecDeque<String>,
}

impl RelayBudgets {
    /// 그 글의 예산을 [`RELAY_BUDGET`]으로 채운다.
    fn fill(&mut self, source_message_id: &str) {
        if self
            .left
            .insert(source_message_id.to_owned(), RELAY_BUDGET)
            .is_none()
        {
            self.order.push_back(source_message_id.to_owned());
        }
        while self.order.len() > RELAY_KEEP {
            if let Some(old) = self.order.pop_front() {
                self.left.remove(&old);
            }
        }
    }

    /// 남은 횟수. 팀 방 글로 채운 적이 없으면 0이다.
    fn left(&self, source_message_id: &str) -> usize {
        self.left.get(source_message_id).copied().unwrap_or(0)
    }

    /// 한 번 쓴다.
    fn spend(&mut self, source_message_id: &str) {
        if let Some(n) = self.left.get_mut(source_message_id) {
            *n = n.saturating_sub(1);
        }
    }
}

impl Office {
    /// 팀 방에 사용자 글이 왔다(C18). 멘션한 담당만 답한다(원본 오피스 방 정책 mention).
    ///
    /// - 멘션한 담당마다 응답 객체를 하나씩 queued로 만들어 `office.chat`으로 낸다(지금의 대기열 · 상태 규칙 그대로).
    ///   `responders`는 멘션 순서.
    /// - 멘션이 없으면 아무도 답하지 않는다: 응답 0, `hint`는 [`crate::TeamHint::NoMention`](E44). 오류가 아니다(글은 남는다).
    /// - 멘션이 명단과 하나도 맞지 않으면 응답 0, `hint`는 [`crate::TeamHint::NoMatch`](E43).
    /// - 글이 비면 [`Error::EmptyMessage`].
    /// - 이 글(`source_message_id`)의 이어 말하기 예산을 [`crate::RELAY_BUDGET`]으로 채운다.
    /// - 멘션한 담당의 대기열이 차 있으면 그 담당 몫만 실패 응답(`queue_full`)으로 남기고 나머지는 그대로 진행한다.
    pub async fn post_team(
        &self,
        text: &str,
        source_message_id: &str,
        roster: &[Member],
    ) -> Result<TeamPost, Error> {
        if text.trim().is_empty() {
            return Err(Error::EmptyMessage);
        }
        let responders = parse_mentions(text, roster);
        let hint = match (responders.is_empty(), mentioned_names(text).is_empty()) {
            (false, _) => None,
            (true, true) => Some(TeamHint::NoMention),
            (true, false) => Some(TeamHint::NoMatch),
        };
        let _turn = self.turn.lock().await;
        self.state().relay.fill(source_message_id);
        let mut responses = Vec::with_capacity(responders.len());
        for &agent in &responders {
            responses.push(
                self.enqueue_or_refuse(Room::Team, agent, source_message_id, None)
                    .await?,
            );
        }
        Ok(TeamPost {
            responders,
            responses,
            hint,
        })
    }

    /// 생각 중인 응답을 끝내고(규칙은 [`Office::complete`]와 같다), 팀 방 답이면 답 속 멘션으로 이어 말하게 한다.
    ///
    /// - 답 속에 다른 담당 멘션이 있으면 그 담당의 응답 객체를 queued로 만든다(팀 방, 같은 사용자 글 번호).
    /// - 한 사용자 글에서 이어 말하기는 [`crate::RELAY_BUDGET`]번까지. 넘으면 더 만들지 않는다.
    /// - 자기 멘션, 그리고 같은 사용자 글에 아직 끝나지 않은 응답(queued · thinking)이 있는 담당은
    ///   무시하고 예산을 쓰지 않는다(원본: 이미 말하는 중인 NPC는 다시 부르지 않음).
    /// - 하루 AI 호출 상한([`crate::AiBudget`])에 걸린 담당은 이어 말하게 하지 않고 예산도 쓰지 않는다.
    /// - 1:1 방의 답, 빈 답(failed), 공지 확인 답은 이어 말하지 않는다.
    pub async fn complete_with_relay(
        &self,
        request_id: &str,
        content: &str,
        roster: &[Member],
    ) -> Result<Relayed, Error> {
        let _turn = self.turn.lock().await;
        let response = self.complete_locked(request_id, content).await?;
        let relays = response.room == Room::Team
            && response.status == ChatStatus::Complete
            && response.notice.is_none();
        let mut follow_ups = Vec::new();
        if relays {
            let source = response.source_message_id.as_str();
            for agent in parse_mentions(content, roster) {
                if agent == response.agent || self.has_open(agent, source) {
                    continue;
                }
                if self.state().relay.left(source) == 0 {
                    break;
                }
                if !self.ai_allows(agent) {
                    continue;
                }
                self.state().relay.spend(source);
                follow_ups.push(
                    self.enqueue_or_refuse(Room::Team, agent, source, None)
                        .await?,
                );
            }
        }
        Ok(Relayed {
            response,
            follow_ups,
        })
    }

    /// 그 담당에게 같은 사용자 글의 끝나지 않은 응답(queued · thinking)이 있는가.
    fn has_open(&self, agent: Agent, source_message_id: &str) -> bool {
        let state = self.state();
        state.queues[agent.index()].iter().any(|id| {
            state
                .responses
                .get(id)
                .is_some_and(|r| r.source_message_id == source_message_id)
        })
    }

    /// 하루 AI 호출 상한 판정. 끼운 판정이 없으면 늘 허용한다.
    pub(super) fn ai_allows(&self, agent: Agent) -> bool {
        self.ai_budget.as_ref().is_none_or(|b| b.try_take(agent))
    }

    /// 응답 객체를 queued로 만들어 낸다. 그 담당의 대기열이 차 있으면 그 몫만 실패 응답
    /// (`queue_full`)으로 남긴다(공지 확인이면 그 담당 확인은 "실패"). `turn`을 쥔 채로 부른다.
    pub(super) async fn enqueue_or_refuse(
        &self,
        room: Room,
        agent: Agent,
        source_message_id: &str,
        notice: Option<NoticeId>,
    ) -> Result<ChatResponse, Error> {
        match self.enqueue(room, agent, source_message_id, notice).await {
            Err(Error::QueueFull { .. }) => {
                let mut refused = self.new_response(room, agent, source_message_id, notice);
                refused.status = ChatStatus::Failed;
                refused.error = Some(QUEUE_FULL.to_owned());
                let done = self.finish(refused).await?;
                self.settle_notice(&done).await;
                Ok(done)
            }
            other => other,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 예산은_채운_글마다_따로이고_오래된_글은_잊는다() {
        let mut b = RelayBudgets::default();
        assert_eq!(b.left("m-0"), 0);
        b.fill("m-0");
        b.spend("m-0");
        assert_eq!(b.left("m-0"), RELAY_BUDGET - 1);
        b.fill("m-0");
        assert_eq!(b.left("m-0"), RELAY_BUDGET);
        for n in 1..=RELAY_KEEP {
            b.fill(&format!("m-{n}"));
        }
        assert_eq!(b.left("m-0"), 0);
        assert_eq!(b.left(&format!("m-{RELAY_KEEP}")), RELAY_BUDGET);
        assert_eq!(b.order.len(), RELAY_KEEP);
    }
}
