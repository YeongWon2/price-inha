//! (41차 3 · 4번) 메신저에서 온 팀 방 멘션 글 · 공지를 사무실과 **같은 길**로 넘긴다. 받을지(길이 · 간격 · 상한 ·
//! 지목) 판정과 답을 메신저로 보내는 일은 `crate::messenger`가 한다. 여기서는 사무실 팀 방 글 · 공지 경로가 하는 일
//! (가려 남기기 · 이벤트 · 담당 차례 답)을 그대로 한다.

use std::sync::Arc;

use pi_office::{Agent, EventSink};
use serde_json::json;

use super::KIND_OFFICE_MESSAGE;
use super::notice_http::{ack_prompt, posted_reply};
use super::office_http::{OfficeDesk, lock};
use super::team_chat::roster;
use crate::MESSENGER_VIA;

/// 메신저에서 넘긴 글에 답할 차례 하나(지목 · 담당 순서).
#[derive(Debug, Clone)]
pub(crate) struct BridgeTurn {
    /// 답할 담당.
    pub(crate) agent: Agent,
    /// 그 응답 번호. 하루 상한으로 턴을 받지 못했으면(공지 확인) 없다.
    pub(crate) request: Option<String>,
}

impl OfficeDesk {
    /// 메신저 팀 방 멘션 글(이미 대괄호 꼴 `@[이름]`)을 사무실 팀 방 글과 같은 길로 받는다: 가려 남기고
    /// [`KIND_OFFICE_MESSAGE`](`"via": "messenger"`)를 내고, 지목한 담당이 차례로 답한다.
    pub(crate) async fn post_from_messenger(
        self: &Arc<Self>,
        text: &str,
        source: &str,
    ) -> Result<Vec<BridgeTurn>, pi_office::Error> {
        let text = self.linked.secrets.redact(text);
        let posted = self
            .linked
            .office
            .post_team(&text, source, &roster())
            .await?;
        let message = json!({
            "room": "team", "agent": null, "text": text, "sourceMessageId": source, "via": MESSENGER_VIA,
        });
        if let Err(error) = self.linked.sink.publish(KIND_OFFICE_MESSAGE, message).await {
            tracing::warn!(%error, "메신저에서 온 팀 방 글을 남기지 못했다");
        }
        let mut turns = Vec::with_capacity(posted.responses.len());
        let mut agents = Vec::new();
        for response in &posted.responses {
            turns.push(BridgeTurn {
                agent: response.agent,
                request: Some(response.request_id.clone()),
            });
            if response.status.is_terminal() {
                continue;
            }
            lock(&self.texts).insert(response.request_id.clone(), text.clone());
            agents.push(response.agent);
        }
        // (결정 29) 메신저에서 온 팀 방 글도 멘션된 담당 모두가 곧바로 곁으로 온다.
        self.gather(&agents).await;
        tokio::spawn(Arc::clone(self).work_in_turn(agents));
        Ok(turns)
    }

    /// 메신저 공지를 사무실 전체 공지와 같은 길로 받는다(가려 저장 · 담당 여섯의 확인 답 턴 · 다음 논의 자료).
    pub(crate) async fn notice_from_messenger(
        self: &Arc<Self>,
        text: &str,
        source: &str,
    ) -> Result<Vec<BridgeTurn>, pi_office::Error> {
        let posted = self.linked.office.post_notice(text, source).await?;
        let (_, agents) = posted_reply(self, &posted, &ack_prompt(&posted.notice.text));
        let turns = posted
            .acks
            .iter()
            .map(|ack| BridgeTurn {
                agent: ack.agent,
                request: ack.response.as_ref().map(|r| r.request_id.clone()),
            })
            .collect();
        tokio::spawn(Arc::clone(self).work_in_turn(agents));
        Ok(turns)
    }
}
