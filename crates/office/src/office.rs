//! 사무실 서버(A3): 담당의 자리 · 일 상태와 채팅 응답을 쥐고 번호 있는 이벤트로 낸다.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex, MutexGuard};

use pi_store::UnixMillis;
use serde_json::Value;

use crate::chat::{ChatResponse, ChatStatus, Room};
use crate::error::Error;
use crate::place::{Activity, Agent, AgentView, TeamState, Work, place_of};
use crate::port::{Clock, EventSink};
use crate::{CHAT_QUEUE_MAX, EVENT_AGENT, EVENT_CHAT};

/// 끝난 응답을 [`Office::response`]로 다시 볼 수 있게 남겨 두는 개수.
/// 원본 DeskRPG 응답 추적기의 "끝난 것은 최근 100개"와 같다(`office-deep.md` 5-1 · 9-3).
const FINISHED_KEEP: usize = 100;

/// 빈 답의 실패 까닭(원본 DeskRPG와 같은 이름).
const EMPTY_RESPONSE: &str = "empty_response";

/// 실패 까닭이 비었을 때 쓰는 이름.
const UNKNOWN_FAILURE: &str = "unknown";

/// 사무실 서버.
///
/// - 담당의 모습([`AgentView`])이 바뀔 때마다 `office.agent` 이벤트를 하나 낸다. 바뀌지 않으면 내지 않는다.
/// - 응답 객체가 바뀔 때마다 `office.chat` 이벤트로 객체 전체를 낸다.
/// - 화면 연결과 상관없이 돈다. 이벤트는 포트로 나가 쌓인다(E37).
///
/// 바꾸는 일은 한 번에 하나씩 차례로 한다(이벤트 번호의 순서가 바뀐 순서와 같다).
/// 응답 객체는 그 이벤트가 나간 뒤에야 바뀐다: 포트가 실패하면 응답은 그대로다.
/// 담당 모습은 "마지막으로 낸 모습"을 쥐므로, 내지 못한 모습은 다음 바꿈 때 다시 낸다.
/// 말하기 · 생각 시작 · 끝내기는 응답 이벤트가 나갔으면 성공이다(담당 모습 이벤트가 실패해도 오류가 아니다).
pub struct Office {
    sink: Arc<dyn EventSink>,
    clock: Arc<dyn Clock>,
    /// 바꾸는 일을 차례로 세운다. 이벤트를 내는 동안(await) 쥐고 있다.
    turn: tokio::sync::Mutex<()>,
    /// 상태. 짧게만 쥐고 await 너머로 들고 가지 않는다.
    state: Mutex<State>,
    /// 응답 번호의 앞머리(만든 시각). 다시 켜도 번호가 앞서 낸 것과 겹치지 않게 한다.
    id_prefix: i64,
}

struct State {
    team: TeamState,
    activity: [Activity; 6],
    /// 마지막으로 이벤트를 낸(또는 처음 정한) 모습.
    shown: [AgentView; 6],
    responses: HashMap<String, ChatResponse>,
    /// 담당마다 끝나지 않은 응답 번호(온 순서, FIFO).
    queues: [VecDeque<String>; 6],
    /// 담당의 마지막으로 끝난 응답이 실패였는가(E6).
    last_failed: [bool; 6],
    /// 끝난 응답 번호(끝난 순서). 오래된 것부터 지운다.
    finished: VecDeque<String>,
    next_id: u64,
}

impl State {
    fn work_of(&self, agent: Agent) -> Work {
        let i = agent.index();
        let queue = &self.queues[i];
        let thinking = queue.iter().any(|id| {
            self.responses
                .get(id)
                .is_some_and(|r| r.status == ChatStatus::Thinking)
        });
        if thinking {
            Work::Thinking
        } else if !queue.is_empty() {
            Work::Queued
        } else if self.last_failed[i] {
            Work::Failed
        } else {
            Work::Idle
        }
    }

    fn desired(&self, agent: Agent) -> AgentView {
        AgentView {
            agent,
            place: place_of(self.team, self.activity[agent.index()]),
            work: self.work_of(agent),
        }
    }

    /// 끝난 응답을 반영한다: 대기열에서 빼고, 실패 표시를 정하고, 오래된 끝난 것을 지운다.
    fn finish(&mut self, response: ChatResponse) {
        let i = response.agent.index();
        self.queues[i].retain(|id| *id != response.request_id);
        self.last_failed[i] = response.status == ChatStatus::Failed;
        self.finished.push_back(response.request_id.clone());
        self.responses.insert(response.request_id.clone(), response);
        while self.finished.len() > FINISHED_KEEP {
            if let Some(old) = self.finished.pop_front() {
                self.responses.remove(&old);
            }
        }
    }
}

/// 한 응답 안에서 `updatedAt`이 반드시 커지게 한다: `max(지금, 이전 + 1)`(원본 DeskRPG와 같다).
fn next_stamp(now: UnixMillis, previous: UnixMillis) -> UnixMillis {
    UnixMillis(now.0.max(previous.0.saturating_add(1)))
}

impl Office {
    /// 만든다. 처음 담당 모습은 `team`과 활동 없음(Idle)으로 정해지고, 이때는 이벤트를 내지 않는다.
    pub fn new(team: TeamState, sink: Arc<dyn EventSink>, clock: Arc<dyn Clock>) -> Office {
        let id_prefix = clock.now().0;
        let shown = Agent::ALL.map(|agent| AgentView {
            agent,
            place: place_of(team, Activity::Idle),
            work: Work::Idle,
        });
        let state = State {
            team,
            activity: [Activity::Idle; 6],
            shown,
            responses: HashMap::new(),
            queues: Default::default(),
            last_failed: [false; 6],
            finished: VecDeque::new(),
            next_id: 1,
        };
        Office {
            sink,
            clock,
            turn: tokio::sync::Mutex::new(()),
            state: Mutex::new(state),
            id_prefix,
        }
    }

    fn state(&self) -> MutexGuard<'_, State> {
        // 상태를 쥔 채 패닉하는 곳이 없다. 그래도 독이 들었으면 안의 값을 그대로 쓴다.
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    async fn publish(&self, kind: &str, payload: Value) -> Result<u64, Error> {
        Ok(self.sink.publish(kind, payload).await?)
    }

    /// 담당들의 지금 모습을 마지막으로 낸 모습과 맞춘다. 다른 담당마다 `office.agent`를 낸다.
    /// `turn`을 쥔 채로 부른다.
    async fn sync_views(&self, agents: &[Agent]) -> Result<Vec<u64>, Error> {
        let mut seqs = Vec::new();
        for &agent in agents {
            let (want, shown) = {
                let state = self.state();
                (state.desired(agent), state.shown[agent.index()])
            };
            if want == shown {
                continue;
            }
            let seq = self.publish(EVENT_AGENT, want.to_json()).await?;
            self.state().shown[agent.index()] = want;
            seqs.push(seq);
        }
        Ok(seqs)
    }

    /// 응답 이벤트가 나간 뒤 담당 모습을 맞춘다. 응답은 이미 들어갔으므로 모습 이벤트가 실패해도
    /// 오류로 돌려주지 않는다: "마지막으로 낸 모습"이 그대로라 다음 바꿈 때 다시 낸다.
    /// `turn`을 쥔 채로 부른다.
    async fn sync_views_after(&self, agents: &[Agent]) {
        let _ = self.sync_views(agents).await;
    }

    /// 응답 객체를 이벤트로 낸 뒤에 상태에 반영한다. `turn`을 쥔 채로 부른다.
    async fn publish_response(&self, response: &ChatResponse) -> Result<(), Error> {
        self.publish(EVENT_CHAT, response.to_json()).await?;
        let mut state = self.state();
        if response.status.is_terminal() {
            state.finish(response.clone());
        } else {
            if response.status == ChatStatus::Queued {
                state.queues[response.agent.index()].push_back(response.request_id.clone());
            }
            state
                .responses
                .insert(response.request_id.clone(), response.clone());
        }
        Ok(())
    }

    /// 담당 6명의 지금 모습([`Agent::ALL`] 순서). 접속 스냅샷에 쓴다.
    pub fn agents(&self) -> Vec<AgentView> {
        self.state().shown.to_vec()
    }

    /// 팀 상태가 바뀌었다. 모습이 바뀐 담당마다 `office.agent`를 내고 그 번호들을 돌려준다.
    pub async fn set_team_state(&self, team: TeamState) -> Result<Vec<u64>, Error> {
        let _turn = self.turn.lock().await;
        self.state().team = team;
        self.sync_views(&Agent::ALL).await
    }

    /// 담당 활동이 바뀌었다. 모습이 바뀌면 `office.agent`를 내고 번호를 돌려준다.
    pub async fn set_activity(
        &self,
        agent: Agent,
        activity: Activity,
    ) -> Result<Option<u64>, Error> {
        let _turn = self.turn.lock().await;
        self.state().activity[agent.index()] = activity;
        Ok(self.sync_views(&[agent]).await?.pop())
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
        if text.trim().is_empty() {
            return Err(Error::EmptyMessage);
        }
        let _turn = self.turn.lock().await;
        let response = {
            let mut state = self.state();
            if state.queues[agent.index()].len() >= CHAT_QUEUE_MAX {
                return Err(Error::QueueFull { agent });
            }
            let n = state.next_id;
            state.next_id += 1;
            ChatResponse {
                request_id: format!("chat-{}-{n}", self.id_prefix),
                source_message_id: source_message_id.to_owned(),
                agent,
                room,
                status: ChatStatus::Queued,
                content: String::new(),
                updated_at: self.clock.now(),
                error: None,
            }
        };
        self.publish_response(&response).await?;
        self.sync_views_after(&[agent]).await;
        Ok(response)
    }

    /// 그 담당의 대기열에서 가장 먼저 온 응답을 thinking으로 바꾼다(FIFO).
    /// 대기 중인 것이 없거나 이미 생각 중인 응답이 있으면 `None`.
    pub async fn start_next(&self, agent: Agent) -> Result<Option<ChatResponse>, Error> {
        let _turn = self.turn.lock().await;
        let next = {
            let state = self.state();
            let mut queued = None;
            for id in &state.queues[agent.index()] {
                match state.responses.get(id) {
                    Some(r) if r.status == ChatStatus::Thinking => return Ok(None),
                    Some(r) if r.status == ChatStatus::Queued && queued.is_none() => {
                        queued = Some(r.clone());
                    }
                    _ => {}
                }
            }
            queued
        };
        let Some(mut response) = next else {
            return Ok(None);
        };
        response.status = ChatStatus::Thinking;
        response.updated_at = next_stamp(self.clock.now(), response.updated_at);
        self.publish_response(&response).await?;
        self.sync_views_after(&[agent]).await;
        Ok(Some(response))
    }

    /// 아직 끝나지 않은 응답을 꺼낸다. 없거나 끝났으면 오류.
    fn open_response(&self, request_id: &str) -> Result<ChatResponse, Error> {
        let state = self.state();
        let response = state
            .responses
            .get(request_id)
            .ok_or_else(|| Error::UnknownRequest(request_id.to_owned()))?;
        if response.status.is_terminal() {
            return Err(Error::Finished(request_id.to_owned()));
        }
        Ok(response.clone())
    }

    /// 응답을 끝난 단계로 바꿔 내고, 담당 모습을 맞춘다. `turn`을 쥔 채로 부른다.
    async fn finish(&self, mut response: ChatResponse) -> Result<ChatResponse, Error> {
        response.updated_at = next_stamp(self.clock.now(), response.updated_at);
        self.publish_response(&response).await?;
        self.sync_views_after(&[response.agent]).await;
        Ok(response)
    }

    /// 생각 중인 응답을 끝낸다. 본문이 비면(공백뿐) failed / `empty_response`가 된다.
    ///
    /// - 이미 끝났으면 [`Error::Finished`]이고 아무것도 바뀌지 않는다.
    /// - 아직 queued면 [`Error::InvalidTransition`].
    pub async fn complete(&self, request_id: &str, content: &str) -> Result<ChatResponse, Error> {
        let _turn = self.turn.lock().await;
        let mut response = self.open_response(request_id)?;
        if response.status != ChatStatus::Thinking {
            return Err(Error::InvalidTransition(request_id.to_owned()));
        }
        if content.trim().is_empty() {
            response.status = ChatStatus::Failed;
            response.content = String::new();
            response.error = Some(EMPTY_RESPONSE.to_owned());
        } else {
            response.status = ChatStatus::Complete;
            response.content = content.to_owned();
            response.error = None;
        }
        self.finish(response).await
    }

    /// 응답을 실패로 끝낸다(E6). queued · thinking에서 갈 수 있다.
    /// 그 담당의 일 상태는 failed로 보인다. 이미 끝났으면 [`Error::Finished`].
    pub async fn fail(&self, request_id: &str, reason: &str) -> Result<ChatResponse, Error> {
        let _turn = self.turn.lock().await;
        let mut response = self.open_response(request_id)?;
        let reason = reason.trim();
        response.status = ChatStatus::Failed;
        response.error = Some(if reason.is_empty() {
            UNKNOWN_FAILURE.to_owned()
        } else {
            reason.to_owned()
        });
        self.finish(response).await
    }

    /// 응답 하나.
    pub fn response(&self, request_id: &str) -> Option<ChatResponse> {
        self.state().responses.get(request_id).cloned()
    }

    /// 끝나지 않은 응답들(queued · thinking). 다시 이었을 때 스냅샷에 쓴다.
    pub fn pending_responses(&self) -> Vec<ChatResponse> {
        let state = self.state();
        state
            .queues
            .iter()
            .flatten()
            .filter_map(|id| state.responses.get(id).cloned())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 시각은_지금과_이전_더하기_하나_중_큰_것() {
        assert_eq!(next_stamp(UnixMillis(10), UnixMillis(3)), UnixMillis(10));
        assert_eq!(next_stamp(UnixMillis(3), UnixMillis(3)), UnixMillis(4));
        assert_eq!(next_stamp(UnixMillis(1), UnixMillis(5)), UnixMillis(6));
        assert_eq!(
            next_stamp(UnixMillis(0), UnixMillis(i64::MAX)),
            UnixMillis(i64::MAX)
        );
    }

    struct Fixed;
    impl Clock for Fixed {
        fn now(&self) -> UnixMillis {
            UnixMillis(7)
        }
    }

    struct Counter(std::sync::atomic::AtomicU64);
    #[async_trait::async_trait]
    impl EventSink for Counter {
        async fn publish(
            &self,
            _kind: &str,
            _payload: Value,
        ) -> Result<u64, crate::error::SinkError> {
            Ok(self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst))
        }
    }

    #[tokio::test]
    async fn 끝난_응답은_정해진_개수만_남긴다() {
        let office = Office::new(
            TeamState::Operating,
            Arc::new(Counter(std::sync::atomic::AtomicU64::new(1))),
            Arc::new(Fixed),
        );
        let mut first = None;
        for n in 0..(FINISHED_KEEP + 5) {
            let r = office
                .ask(Room::Direct, Agent::Risk, "q", &format!("m-{n}"))
                .await
                .unwrap();
            first.get_or_insert(r.request_id.clone());
            let failed = office.fail(&r.request_id, " ").await.unwrap();
            assert_eq!(failed.error.as_deref(), Some(UNKNOWN_FAILURE));
        }
        let state = office.state();
        assert_eq!(state.finished.len(), FINISHED_KEEP);
        assert_eq!(state.responses.len(), FINISHED_KEEP);
        drop(state);
        assert!(office.response(&first.unwrap()).is_none());
        let pending = office.pending_responses();
        assert!(pending.is_empty());
    }
}
