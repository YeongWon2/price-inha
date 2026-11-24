//! 공지 나눠 주기(6차). `Office`의 안쪽 칸을 그대로 쓰는 `impl Office` 조각이다.

use std::sync::Arc;

use pi_store::{NOTICE_TEXT_MAX_CHARS, NewNotice, Notice, NoticeAckStatus, NoticeId, Store};

use super::Office;
use crate::chat::{ChatResponse, ChatStatus, Room};
use crate::error::Error;
use crate::notice::{NoticeAckTurn, NoticePost, NoticeTurn};
use crate::place::Agent;
use crate::port::{AiBudget, Redactor};
use crate::{EVENT_NOTICE, EVENT_NOTICE_ACK};

/// 공지 저장소와 가리기.
pub(super) struct NoticeDesk {
    store: Store,
    redactor: Arc<dyn Redactor>,
}

/// `office.notice_ack`의 `state` 이름.
fn ack_state_name(status: NoticeAckStatus) -> &'static str {
    match status {
        NoticeAckStatus::Waiting => "waiting",
        NoticeAckStatus::Answered => "answered",
        NoticeAckStatus::Capped => "limit",
        NoticeAckStatus::Failed => "failed",
    }
}

impl Office {
    /// 공지 저장소와 가리기를 끼운다. 끼우지 않으면 [`Office::post_notice`]는 [`Error::NoticeUnavailable`].
    pub fn with_notices(self, store: Store, redactor: Arc<dyn Redactor>) -> Office {
        Office {
            notices: Some(NoticeDesk { store, redactor }),
            ..self
        }
    }

    /// 하루 AI 호출 상한 판정 포트를 끼운다. 끼우지 않으면 늘 허용한다.
    pub fn with_ai_budget(self, budget: Arc<dyn AiBudget>) -> Office {
        Office {
            ai_budget: Some(budget),
            ..self
        }
    }

    /// 공지를 받는다(C19 · 결정 17).
    ///
    /// 1. 글이 0자이거나 [`pi_store::NOTICE_TEXT_MAX_CHARS`]자(글자 수)보다 길면 [`Error::InvalidNotice`]이고
    ///    아무것도 남기지 않는다(저장 · 이벤트 없음). 글자 수는 가리기 전 글로 센다.
    /// 2. 글을 가린 뒤 저장소에 공지를 만든다. 담당 여섯([`crate::Agent::ALL`], 이름은 [`crate::Agent::id`])이 모두 기다림.
    /// 3. `office.notice` `{"notice", "text", "at"}`(가린 글)를 낸다.
    /// 4. 담당마다 [`AiBudget::try_take`]로 물어 허용이면 확인 답 응답 객체(queued, 팀 방, `notice` 붙음)를
    ///    대기열에 넣고 `office.chat`을 낸다. 상한이면 저장소 확인을 "상한으로 못 함"으로 바꾸고
    ///    `office.notice_ack` `{"notice", "agent", "state": "limit"}`를 낸다(E45).
    ///    그 담당의 대기열이 차 있으면 그 몫만 실패 응답(`queue_full`)이고 확인은 "실패"다.
    /// 5. 확인 답이 끝나면 [`Office::complete`] · [`Office::fail`]이 "답함" · "실패"로 한 번만 바꾼다.
    ///
    /// 공지는 규칙 · 템플릿 · 승인 · 팀 상태를 바꾸지 않는다(E46).
    pub async fn post_notice(
        &self,
        text: &str,
        source_message_id: &str,
    ) -> Result<NoticePost, Error> {
        let desk = self.notices.as_ref().ok_or(Error::NoticeUnavailable)?;
        let chars = text.chars().count();
        if chars == 0 || chars > NOTICE_TEXT_MAX_CHARS {
            return Err(Error::InvalidNotice(format!(
                "글은 1~{NOTICE_TEXT_MAX_CHARS}자여야 한다({chars}자)"
            )));
        }
        let _turn = self.turn.lock().await;
        let notice = desk.store.create_notice(NewNotice {
            text: desk.redactor.redact(text),
            roles: Agent::ALL.iter().map(|a| a.id().to_owned()).collect(),
            at: self.clock.now(),
        })?;
        let payload = serde_json::json!({
            "notice": notice.id.0,
            "text": notice.text,
            "at": notice.sent_at.0,
        });
        self.publish(EVENT_NOTICE, payload).await?;
        let mut acks = Vec::with_capacity(Agent::ALL.len());
        for agent in Agent::ALL {
            acks.push(
                self.notice_turn(desk, &notice, agent, source_message_id)
                    .await?,
            );
        }
        Ok(NoticePost { notice, acks })
    }

    /// 담당 한 명에게 확인 답 턴을 준다. 상한이면 확인을 "상한으로 못 함"으로 바꾼다. `turn`을 쥔 채로 부른다.
    async fn notice_turn(
        &self,
        desk: &NoticeDesk,
        notice: &Notice,
        agent: Agent,
        source_message_id: &str,
    ) -> Result<NoticeAckTurn, Error> {
        if !self.ai_allows(agent) {
            self.record_ack(desk, notice.id, agent, NoticeAckStatus::Capped, "")
                .await?;
            return Ok(NoticeAckTurn {
                agent,
                state: NoticeTurn::Limit,
                response: None,
            });
        }
        let response = self
            .enqueue_or_refuse(Room::Team, agent, source_message_id, Some(notice.id))
            .await?;
        Ok(NoticeAckTurn {
            agent,
            state: NoticeTurn::Queued,
            response: Some(response),
        })
    }

    /// 끝난 공지 확인 답을 저장소 확인에 한 번 옮긴다: complete면 "답함", failed면 "실패".
    /// 응답은 이미 끝나 이벤트가 나갔으므로 여기서의 실패는 돌려주지 않는다(이미 바뀐 확인은 그대로 둔다).
    /// `turn`을 쥔 채로 부른다.
    pub(super) async fn settle_notice(&self, response: &ChatResponse) {
        let (Some(id), Some(desk)) = (response.notice, self.notices.as_ref()) else {
            return;
        };
        let (status, answer) = match response.status {
            ChatStatus::Complete => (NoticeAckStatus::Answered, response.content.as_str()),
            ChatStatus::Failed => (
                NoticeAckStatus::Failed,
                response.error.as_deref().unwrap_or_default(),
            ),
            _ => return,
        };
        let _ = self
            .record_ack(desk, id, response.agent, status, answer)
            .await;
    }

    /// 저장소의 그 담당 확인을 바꾸고(답 글은 가린 뒤) `office.notice_ack`를 낸다. `turn`을 쥔 채로 부른다.
    async fn record_ack(
        &self,
        desk: &NoticeDesk,
        id: NoticeId,
        agent: Agent,
        status: NoticeAckStatus,
        answer: &str,
    ) -> Result<(), Error> {
        let answer = desk.redactor.redact(answer);
        desk.store
            .answer_notice(id, agent.id(), status, &answer, self.clock.now())?;
        let payload = serde_json::json!({
            "notice": id.0,
            "agent": agent.id(),
            "state": ack_state_name(status),
        });
        self.publish(EVENT_NOTICE_ACK, payload).await?;
        Ok(())
    }
}
