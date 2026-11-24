//! 담당 부르기 단계 · 오래된 일 풀기 · 일 알림(5차). `Office`의 안쪽 칸을 그대로 쓰는 `impl Office` 조각이다.

use pi_store::UnixMillis;

use super::Office;
use crate::chat::Room;
use crate::error::Error;
use crate::place::{Agent, Place, Work, place_of};
use crate::summon::{STALE_WORK_MS, SUMMON_ARRIVE_MS, SUMMON_IDLE_MS, SummonPhase, SummonReason};

/// 부름으로 곁에서 기다리는 담당이 대화 없이 10초를 넘겼는가. 멘션으로 온 담당은 방을 보는 동안 남는다.
fn phase_waiting_expired(phase: SummonPhase, reason: Option<SummonReason>, quiet_ms: i64) -> bool {
    phase == SummonPhase::Waiting
        && reason == Some(SummonReason::Call)
        && quiet_ms >= SUMMON_IDLE_MS
}

impl Office {
    /// 부르기 단계를 바꿔 이벤트를 낸다. 내지 못하면 되돌리고 오류를 돌려준다. `turn`을 쥔 채로 부른다.
    async fn set_phase(
        &self,
        agent: Agent,
        phase: SummonPhase,
        reason: Option<SummonReason>,
        at: UnixMillis,
    ) -> Result<Option<u64>, Error> {
        let i = agent.index();
        let old = {
            let mut state = self.state();
            let old = (
                state.phase[i],
                state.reason[i],
                state.phase_at[i],
                state.last_talk[i],
            );
            state.phase[i] = phase;
            state.reason[i] = reason;
            state.phase_at[i] = at;
            if phase == SummonPhase::Waiting {
                state.last_talk[i] = at;
            }
            old
        };
        match self.sync_views(&[agent]).await {
            Ok(mut seqs) => Ok(seqs.pop()),
            Err(e) => {
                let mut state = self.state();
                state.phase[i] = old.0;
                state.reason[i] = old.1;
                state.phase_at[i] = old.2;
                state.last_talk[i] = old.3;
                Err(e)
            }
        }
    }

    /// 부르기와 멘션이 함께 쓰는 길. `turn`을 쥔 채로 부르지 않는다(안에서 쥔다).
    async fn call_in(&self, agent: Agent, reason: SummonReason) -> Result<SummonPhase, Error> {
        let _turn = self.turn.lock().await;
        let now = self.clock.now();
        let i = agent.index();
        {
            let state = self.state();
            if matches!(state.phase[i], SummonPhase::Called | SummonPhase::Waiting) {
                return Ok(state.phase[i]);
            }
            if place_of(state.team, state.activity[i]) == Place::Meeting {
                return Err(Error::MeetingReserved { agent });
            }
        }
        self.set_phase(agent, SummonPhase::Called, Some(reason), now)
            .await?;
        Ok(SummonPhase::Called)
    }

    /// 담당을 부른다(C20 · 결정 17). 자리의 담당은 `Called`(까닭 `Call`)로 들어가고 `office.agent`를 하나 낸다.
    ///
    /// - 담당이 미팅룸에서 논의 중이면(`place`가 `Meeting`) [`Error::MeetingReserved`]이고 아무것도 바뀌지 않는다.
    /// - 이미 `Called` · `Waiting`이면 그대로다(이벤트 없음). 일하는 중이어도 부를 수 있다.
    /// - 돌려주는 것은 부른 뒤의 단계다.
    pub async fn summon(&self, agent: Agent) -> Result<SummonPhase, Error> {
        self.call_in(agent, SummonReason::Call).await
    }

    /// 화면이 "도착했다"를 알린다. `Called`면 `Waiting`으로 간다. 그 밖의 단계에서는 아무것도 하지 않는다.
    /// 돌려주는 것은 알린 뒤의 단계다.
    pub async fn summon_arrived(&self, agent: Agent) -> Result<SummonPhase, Error> {
        let _turn = self.turn.lock().await;
        let now = self.clock.now();
        let i = agent.index();
        let (phase, reason) = {
            let state = self.state();
            (state.phase[i], state.reason[i])
        };
        if phase != SummonPhase::Called {
            return Ok(phase);
        }
        self.set_phase(agent, SummonPhase::Waiting, reason, now)
            .await?;
        Ok(SummonPhase::Waiting)
    }

    /// 화면이 "자리로 돌아왔다"를 알린다. `Returning`이면 `Idle`로 간다. 그 밖의 단계에서는 아무것도 하지 않는다.
    /// 돌려주는 것은 알린 뒤의 단계다.
    pub async fn summon_returned(&self, agent: Agent) -> Result<SummonPhase, Error> {
        let _turn = self.turn.lock().await;
        let now = self.clock.now();
        let phase = self.state().phase[agent.index()];
        if phase != SummonPhase::Returning {
            return Ok(phase);
        }
        self.set_phase(agent, SummonPhase::Idle, None, now).await?;
        Ok(SummonPhase::Idle)
    }

    /// 곁에서 기다리는 담당과 대화가 있었다(말을 걸었거나 답이 갔다). `Waiting`이면 10초를 다시 센다.
    /// 이벤트는 내지 않는다.
    pub fn note_conversation(&self, agent: Agent) {
        let now = self.clock.now();
        let mut state = self.state();
        let i = agent.index();
        if state.phase[i] == SummonPhase::Waiting {
            state.last_talk[i] = now;
        }
    }

    /// 팀 방에서 멘션된 담당의 답 턴이 시작됐다(원본: 턴 시작과 동시에 이동). `Called`(까닭 `Mention`)로 들어간다.
    /// 논의 중이면 [`Error::MeetingReserved`](답은 채팅에만 간다). 이미 불려 온 담당이면 그대로.
    pub async fn mention_turn_started(&self, agent: Agent) -> Result<SummonPhase, Error> {
        self.call_in(agent, SummonReason::Mention).await
    }

    /// 사용자가 방을 떠났다(화면이 알린다). 팀 방을 떠나면 멘션으로 온 담당(`Called` · `Waiting`)이
    /// `Returning`으로 간다. 부름으로 온 담당은 그대로다. 바뀐 담당마다 이벤트를 내고 번호들을 돌려준다.
    pub async fn user_left_room(&self, room: Room) -> Result<Vec<u64>, Error> {
        if room != Room::Team {
            return Ok(Vec::new());
        }
        let _turn = self.turn.lock().await;
        let now = self.clock.now();
        let mut seqs = Vec::new();
        for agent in Agent::ALL {
            let i = agent.index();
            let hit = {
                let state = self.state();
                state.reason[i] == Some(SummonReason::Mention)
                    && matches!(state.phase[i], SummonPhase::Called | SummonPhase::Waiting)
            };
            if hit
                && let Some(seq) = self
                    .set_phase(
                        agent,
                        SummonPhase::Returning,
                        Some(SummonReason::Mention),
                        now,
                    )
                    .await?
            {
                seqs.push(seq);
            }
        }
        Ok(seqs)
    }

    /// 시계가 흘렀다. 넣어 준 시계로 지금을 읽어 때가 된 일을 한다. 바뀐 담당마다 이벤트를 내고 번호들을 돌려준다.
    ///
    /// - 부름으로 `Waiting`인 담당이 마지막 대화(없으면 도착) 뒤 10초 이상 조용하면 `Returning`.
    ///   멘션으로 온 담당은 방을 보는 동안 남으므로 이 시간으로는 돌아가지 않는다.
    /// - `Called`인 담당이 부른 뒤 30초 이상 도착 알림이 없으면 `Returning`을 거쳐 자리(`Idle`)로 되돌린다.
    /// - `thinking` · `queued` 일이 마지막으로 바뀐 뒤 300초 이상 그대로면 한가(`idle`)로 풀고 이벤트를 낸다.
    pub async fn tick(&self) -> Result<Vec<u64>, Error> {
        let _turn = self.turn.lock().await;
        let now = self.clock.now();
        let mut seqs = Vec::new();
        for agent in Agent::ALL {
            let i = agent.index();
            let (reason, back, give_up) = {
                let state = self.state();
                let quiet = now
                    .0
                    .saturating_sub(state.last_talk[i].0.max(state.phase_at[i].0));
                let since_call = now.0.saturating_sub(state.phase_at[i].0);
                (
                    state.reason[i],
                    phase_waiting_expired(state.phase[i], state.reason[i], quiet),
                    state.phase[i] == SummonPhase::Called && since_call >= SUMMON_ARRIVE_MS,
                )
            };
            if back || give_up {
                if let Some(seq) = self
                    .set_phase(agent, SummonPhase::Returning, reason, now)
                    .await?
                {
                    seqs.push(seq);
                }
                if give_up
                    && let Some(seq) = self.set_phase(agent, SummonPhase::Idle, None, now).await?
                {
                    seqs.push(seq);
                }
            }
            if let Some(seq) = self.release_stale(agent, now).await? {
                seqs.push(seq);
            }
        }
        Ok(seqs)
    }

    /// 일(`thinking` · `queued`)이 300초 넘게 바뀌지 않았으면 한가로 푼다. `turn`을 쥔 채로 부른다.
    async fn release_stale(&self, agent: Agent, now: UnixMillis) -> Result<Option<u64>, Error> {
        let i = agent.index();
        let old = {
            let mut state = self.state();
            let busy = matches!(state.effective_work(agent), Work::Thinking | Work::Queued);
            if !busy || now.0.saturating_sub(state.work_since[i].0) < STALE_WORK_MS {
                return Ok(None);
            }
            let old = (
                state.work_override[i],
                state.work_since[i],
                state.work_floor[i],
            );
            state.work_override[i] = Some(Work::Idle);
            state.work_since[i] = now;
            state.work_floor[i] = Some(now);
            old
        };
        match self.sync_views(&[agent]).await {
            Ok(mut seqs) => Ok(seqs.pop()),
            Err(e) => {
                let mut state = self.state();
                state.work_override[i] = old.0;
                state.work_since[i] = old.1;
                state.work_floor[i] = old.2;
                Err(e)
            }
        }
    }

    /// 담당의 일 상태 소식이 밖에서 늦게 왔다(N15 · E55). `at`은 그 소식의 시각이다.
    /// 이미 반영한 것(오래된 일 풀기 포함)의 시각보다 크지 않으면 버리고 `Ok(None)`.
    /// 반영하면 모습이 바뀌었을 때 낸 이벤트 번호를 돌려준다.
    pub async fn report_work(
        &self,
        agent: Agent,
        work: Work,
        at: UnixMillis,
    ) -> Result<Option<u64>, Error> {
        let _turn = self.turn.lock().await;
        let i = agent.index();
        let old = {
            let mut state = self.state();
            if state.work_floor[i].is_some_and(|floor| at.0 <= floor.0) {
                return Ok(None);
            }
            let old = (
                state.work_override[i],
                state.work_since[i],
                state.work_floor[i],
            );
            state.work_override[i] = Some(work);
            state.work_since[i] = at;
            state.work_floor[i] = Some(at);
            old
        };
        match self.sync_views(&[agent]).await {
            Ok(mut seqs) => Ok(seqs.pop()),
            Err(e) => {
                let mut state = self.state();
                state.work_override[i] = old.0;
                state.work_since[i] = old.1;
                state.work_floor[i] = old.2;
                Err(e)
            }
        }
    }
}
