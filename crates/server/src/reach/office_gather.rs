//! (59차 · 결정 29) 멘션하면 담당이 내 앞으로 모인다: 멘션이 정해진 그때(팀 방 글을 받아 응답자를 정한 때 · 답 속
//! 멘션으로 이어 말할 담당이 정해진 때 · 메신저 팀 방 글) 멘션된 담당 모두를 곧바로 곁으로 부른다. 답 턴을 기다리지
//! 않는다(답은 지금처럼 차례로 쓴다). 이미 와 있거나 오는 중이면 그대로, 논의 중이면 오지 않고 답만 간다.

use pi_office::Agent;

use super::office_http::OfficeDesk;

impl OfficeDesk {
    /// 멘션된 담당들을 곧바로 곁으로 부른다(`Called`, 까닭 `Mention`). 실패는 적기만 한다(글 받기는 막지 않는다).
    pub(super) async fn gather(&self, agents: &[Agent]) {
        for (agent, called) in self.linked.office.mention_posted(agents).await {
            match called {
                Ok(_) | Err(pi_office::Error::MeetingReserved { .. }) => {}
                Err(error) => {
                    tracing::warn!(%error, agent = agent.id(), "멘션된 담당을 부르지 못했다")
                }
            }
        }
    }
}
