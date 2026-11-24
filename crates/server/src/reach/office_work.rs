//! 사무실 답 만들기(뒤에서 돈다): 담당 대기열에서 차례가 온 응답에 AI 턴으로 답한다.
//!
//! (33차) 멘션으로 받은 턴은 시작하는 순간 그 담당을 곁으로 부르고(`mention_turn_started`, 원본: 턴 시작과 동시에
//! 이동), 팀 방 답은 답 속 멘션으로 이어 말하게 한다(`complete_with_relay`). 1:1 답이 끝나면 곁에서 기다리는
//! 담당의 10초를 다시 센다.

use std::sync::Arc;

use pi_office::{Agent, ChatResponse, Room};

use super::office_http::{OfficeDesk, lock};
use super::team_chat::{name_of, roster};
use super::{CHAT_TOTAL_TIMEOUT, OFFICE_TEXT_MAX};
use crate::team::role_of;

impl OfficeDesk {
    /// 그 담당의 대기열에서 차례가 온 것에 답한다(뒤에서 돈다).
    pub(super) async fn work(self: Arc<Self>, agent: Agent) {
        let Some(turn) = self.turns.get(agent.id()) else {
            return;
        };
        let _turn = turn.lock().await;
        let office = &self.linked.office;
        let next = match office.start_next(agent).await {
            Ok(Some(next)) => next,
            Ok(None) => return,
            Err(error) => {
                tracing::warn!(%error, "사무실 답을 시작하지 못했다");
                return;
            }
        };
        self.come_if_mentioned(&next).await;
        let text = lock(&self.texts)
            .remove(&next.request_id)
            .unwrap_or_default();
        let answer = self.answer(agent, &text).await;
        // 답이 온 시각을 "마지막 대화"로 적는다. 끝남 이벤트를 내기 전에 적어야, 끝남을 본 쪽이
        // 곧바로 시간을 재도 10초가 답에서부터 세어진다.
        if next.room == Room::Direct {
            office.note_conversation(agent);
        }
        let done = match answer {
            Ok(answer) if !answer.trim().is_empty() => {
                match office
                    .complete_with_relay(&next.request_id, &answer, &roster())
                    .await
                {
                    Ok(relayed) => {
                        self.relay(&text, agent, &answer, relayed.follow_ups);
                        Ok(())
                    }
                    Err(error) => Err(error),
                }
            }
            Ok(_) => office
                .fail(&next.request_id, "empty_response")
                .await
                .map(drop),
            Err(why) => office.fail(&next.request_id, why).await.map(drop),
        };
        if let Err(error) = done {
            tracing::warn!(%error, "사무실 답을 끝내지 못했다");
        }
    }

    /// 담당의 AI 턴(시연 모드는 가짜 AI). 답은 가린 것.
    async fn answer(&self, agent: Agent, text: &str) -> Result<String, &'static str> {
        let Some(crew) = &self.crew else {
            return Err("no_team");
        };
        let asked = crew.chat(role_of(agent), text);
        match tokio::time::timeout(CHAT_TOTAL_TIMEOUT, asked).await {
            Ok(Ok(answer)) => Ok(self.linked.secrets.redact(&answer)),
            Ok(Err(_)) => Err("agent_failed"),
            Err(_) => Err("timeout"),
        }
    }

    /// (33차 2번) 멘션으로 받은 턴이면 그 담당을 곁으로 부른다. 논의 중이면 오지 않고 답만 간다.
    async fn come_if_mentioned(&self, next: &ChatResponse) {
        if !lock(&self.mentions).remove(&next.request_id) {
            return;
        }
        match self.linked.office.mention_turn_started(next.agent).await {
            Ok(_) | Err(pi_office::Error::MeetingReserved { .. }) => {}
            Err(error) => tracing::warn!(%error, "멘션된 담당을 부르지 못했다"),
        }
    }

    /// 답 속 멘션으로 이어 말하게 된 담당들에게 차례로 답을 맡긴다. 그 담당이 받는 글은 처음 글과 앞 담당의 답이다.
    fn relay(
        self: &Arc<Self>,
        text: &str,
        from: Agent,
        answer: &str,
        follow_ups: Vec<ChatResponse>,
    ) {
        if follow_ups.is_empty() {
            return;
        }
        let said = format!("{text}\n\n{}의 답: {answer}", name_of(from));
        let said: String = said.chars().take(OFFICE_TEXT_MAX * 2).collect();
        let mut agents = Vec::with_capacity(follow_ups.len());
        for follow in follow_ups {
            lock(&self.texts).insert(follow.request_id.clone(), said.clone());
            lock(&self.mentions).insert(follow.request_id);
            agents.push(follow.agent);
        }
        tokio::spawn(Arc::clone(self).work_in_turn(agents));
    }

    /// 담당들이 차례로 답한다(멘션 · 공지 확인 답의 "차례로").
    pub(super) async fn work_in_turn(self: Arc<Self>, agents: Vec<Agent>) {
        for agent in agents {
            Arc::clone(&self).work(agent).await;
        }
    }
}
