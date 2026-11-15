//! 사무실 서버(A3): 담당의 자리 · 일 상태와 채팅 응답을 쥐고 번호 있는 이벤트로 낸다.

use std::sync::Arc;

use crate::chat::{ChatResponse, Room};
use crate::error::Error;
use crate::place::{Activity, Agent, AgentView, TeamState};
use crate::port::{Clock, EventSink};

/// 사무실 서버.
///
/// - 담당의 모습([`AgentView`])이 바뀔 때마다 `office.agent` 이벤트를 하나 낸다. 바뀌지 않으면 내지 않는다.
/// - 응답 객체가 바뀔 때마다 `office.chat` 이벤트로 객체 전체를 낸다.
/// - 화면 연결과 상관없이 돈다. 이벤트는 포트로 나가 쌓인다(E37).
pub struct Office {
    _sink: Arc<dyn EventSink>,
    _clock: Arc<dyn Clock>,
}

impl Office {
    /// 만든다. 처음 담당 모습은 `team`과 활동 없음(Idle)으로 정해지고, 이때는 이벤트를 내지 않는다.
    pub fn new(team: TeamState, sink: Arc<dyn EventSink>, clock: Arc<dyn Clock>) -> Office {
        let _ = (team, sink, clock);
        todo!()
    }

    /// 담당 6명의 지금 모습([`Agent::ALL`] 순서). 접속 스냅샷에 쓴다.
    pub fn agents(&self) -> Vec<AgentView> {
        todo!()
    }

    /// 팀 상태가 바뀌었다. 모습이 바뀐 담당마다 `office.agent`를 내고 그 번호들을 돌려준다.
    pub async fn set_team_state(&self, team: TeamState) -> Result<Vec<u64>, Error> {
        let _ = team;
        todo!()
    }

    /// 담당 활동이 바뀌었다. 모습이 바뀌면 `office.agent`를 내고 번호를 돌려준다.
    pub async fn set_activity(
        &self,
        agent: Agent,
        activity: Activity,
    ) -> Result<Option<u64>, Error> {
        let _ = (agent, activity);
        todo!()
    }

    /// 담당에게 말한다(C5). 응답 객체를 queued로 만들어 내고 돌려준다.
    ///
    /// - 글이 비면 [`Error::EmptyMessage`].
    /// - 그 담당의 끝나지 않은 응답이 [`crate::CHAT_QUEUE_MAX`]개면 [`Error::QueueFull`].
    pub async fn ask(
        &self,
        room: Room,
        agent: Agent,
        text: &str,
        source_message_id: &str,
    ) -> Result<ChatResponse, Error> {
        let _ = (room, agent, text, source_message_id);
        todo!()
    }

    /// 그 담당의 대기열에서 가장 먼저 온 응답을 thinking으로 바꾼다(FIFO).
    /// 대기 중인 것이 없거나 이미 생각 중인 응답이 있으면 `None`.
    pub async fn start_next(&self, agent: Agent) -> Result<Option<ChatResponse>, Error> {
        let _ = agent;
        todo!()
    }

    /// 생각 중인 응답을 끝낸다. 본문이 비면(공백뿐) failed / `empty_response`가 된다.
    ///
    /// - 이미 끝났으면 [`Error::Finished`]이고 아무것도 바뀌지 않는다.
    /// - 아직 queued면 [`Error::InvalidTransition`].
    pub async fn complete(&self, request_id: &str, content: &str) -> Result<ChatResponse, Error> {
        let _ = (request_id, content);
        todo!()
    }

    /// 응답을 실패로 끝낸다(E6). queued · thinking에서 갈 수 있다.
    /// 그 담당의 일 상태는 failed로 보인다. 이미 끝났으면 [`Error::Finished`].
    pub async fn fail(&self, request_id: &str, reason: &str) -> Result<ChatResponse, Error> {
        let _ = (request_id, reason);
        todo!()
    }

    /// 응답 하나.
    pub fn response(&self, request_id: &str) -> Option<ChatResponse> {
        let _ = request_id;
        todo!()
    }

    /// 끝나지 않은 응답들(queued · thinking). 다시 이었을 때 스냅샷에 쓴다.
    pub fn pending_responses(&self) -> Vec<ChatResponse> {
        todo!()
    }
}
