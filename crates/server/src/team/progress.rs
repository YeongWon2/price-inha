//! (51차, 차수 표 21번) 논의 진행 단계(C21 · E51 · V63): agent의 진행 신호 포트를 사무실 이벤트로 잇는다.
//!
//! - 논의가 도는 동안 [`KIND_OFFICE_PROGRESS`] `{"room", "topic", "members": [{"agent", "state", "retry"?,
//!   "retry_max": 3}], "calls", "done", "started_at", "at", "closed"}`를 신호마다 낸다. 담당은 여섯(사무실 순서),
//!   논의에 나오지 않는 담당은 대기다. `calls`는 그 논의에서 부른 AI 호출 수(담당 턴 · 다시 시도 · 점수 · 팀장 판단
//!   하나씩), `done`은 완료인 담당 수다. 끝나면 마지막 상태를 `closed: true`로 한 번 내고 비운다.
//! - 담당 턴이 실패하면 3번까지 다시 시도한다(E51): 그 담당은 "오류" 다음 `retry: n`으로 다시 진행이 된다.
//! - 점수 턴 · 팀장 판단은 끝 신호가 없다. 다음 턴이 시작되거나 논의가 끝나면 진행 중이던 담당을 완료로 본다.
//! - 신호를 받는 쪽은 상태만 바꿔 줄에 넣는다. 내기는 논의와 나란히 도는 쪽이 차례대로 한다. 내다 실패해도
//!   논의는 그대로 간다. 신호마다 그 담당의 하루 AI 사용도 센다([`super::budget`]).

use std::sync::{Mutex, PoisonError};

use pi_agent::{
    DiscussionOptions, DiscussionOutcome, DiscussionRequest, ProgressSignal, ProgressSink, Role,
    SignalError, Stage, TURN_RETRY_MAX,
};
use pi_office::Agent;
use serde_json::{Value, json};
use tokio::sync::mpsc;

use super::Crew;
use super::link::office_agent;

/// 논의 진행 단계 이벤트(C21).
pub const KIND_OFFICE_PROGRESS: &str = "office.progress";

/// 담당 하나의 진행.
#[derive(Debug, Clone, Copy)]
struct Seat {
    stage: Stage,
    retry: Option<u32>,
}

const WAITING: Seat = Seat {
    stage: Stage::Waiting,
    retry: None,
};

/// 논의 하나의 진행 상태.
struct Board {
    room: String,
    topic: String,
    started_at: i64,
    seats: [Seat; 6],
    calls: u32,
}

/// 줄에 넣는 것 하나: 낼 내용, 호출을 쓴 담당(있으면), 마지막인가.
type Note = (Value, Option<Role>, bool);

fn slot(agent: Agent) -> usize {
    Agent::ALL
        .iter()
        .position(|one| *one == agent)
        .unwrap_or_default()
}

fn stage_name(stage: Stage) -> &'static str {
    match stage {
        Stage::Waiting => "waiting",
        Stage::Running => "running",
        Stage::Done => "done",
        Stage::Error => "error",
    }
}

impl Board {
    fn seat(&mut self, role: Role) -> &mut Seat {
        &mut self.seats[slot(office_agent(role))]
    }

    /// 진행 중이던 다른 담당을 완료로 본다(끝 신호가 없는 점수 · 팀장 턴).
    fn settle_running(&mut self) {
        for seat in &mut self.seats {
            if seat.stage == Stage::Running {
                *seat = Seat {
                    stage: Stage::Done,
                    retry: None,
                };
            }
        }
    }

    /// 그 담당의 턴(호출 하나)이 시작된다.
    fn start(&mut self, role: Role) -> Option<Role> {
        let retry = self.seat(role).retry;
        self.settle_running();
        *self.seat(role) = Seat {
            stage: Stage::Running,
            retry,
        };
        self.calls += 1;
        Some(role)
    }

    /// 신호 하나를 반영한다. 호출 하나를 쓴 담당이면 그 담당을 준다.
    fn apply(&mut self, signal: &ProgressSignal) -> Option<Role> {
        match signal {
            ProgressSignal::Opened { .. } => {
                self.seats = [WAITING; 6];
                None
            }
            ProgressSignal::Member {
                role,
                stage: Stage::Running,
            } => self.start(*role),
            ProgressSignal::Member { role, stage } => {
                let seat = self.seat(*role);
                seat.stage = *stage;
                if *stage == Stage::Done {
                    seat.retry = None;
                }
                None
            }
            ProgressSignal::Retry { role, attempt, .. } => {
                self.seat(*role).retry = Some(*attempt);
                None
            }
            ProgressSignal::ScoreTurn { role } => self.start(*role),
            ProgressSignal::LeaderTurn => self.start(Role::Leader),
            ProgressSignal::Closed => {
                self.settle_running();
                None
            }
        }
    }

    fn to_json(&self, at: i64, closed: bool) -> Value {
        let members: Vec<Value> = Agent::ALL
            .iter()
            .map(|agent| {
                let seat = self.seats[slot(*agent)];
                let mut one = json!({
                    "agent": agent.id(),
                    "state": stage_name(seat.stage),
                    "retry_max": TURN_RETRY_MAX,
                });
                if let Some(retry) = seat.retry {
                    one["retry"] = json!(retry);
                }
                one
            })
            .collect();
        let done = self
            .seats
            .iter()
            .filter(|seat| seat.stage == Stage::Done)
            .count();
        json!({
            "room": self.room, "topic": self.topic, "members": members, "calls": self.calls,
            "done": done, "started_at": self.started_at, "at": at, "closed": closed,
        })
    }
}

/// agent의 진행 신호 포트: 상태를 바꿔 줄에 넣는다.
struct Signals {
    board: Mutex<Board>,
    clock: std::sync::Arc<dyn pi_market::Clock>,
    line: mpsc::UnboundedSender<Note>,
}

impl ProgressSink for Signals {
    fn signal(&self, signal: ProgressSignal) -> Result<(), SignalError> {
        let last = matches!(signal, ProgressSignal::Closed);
        let mut board = self.board.lock().unwrap_or_else(PoisonError::into_inner);
        let called = board.apply(&signal);
        let at = self.clock.now().timestamp_millis();
        let note = (board.to_json(at, last), called, last);
        self.line
            .send(note)
            .map_err(|_| SignalError("진행 단계 줄이 닫혔습니다".to_string()))
    }
}

/// 논의가 취소 · panic으로 끝나도 사무실 표시를 비운다.
struct ClearOnDrop<'a>(&'a Crew);

impl Drop for ClearOnDrop<'_> {
    fn drop(&mut self) {
        if let Some(board) = self.0.meter.board.get() {
            board.clear();
        }
    }
}

impl Crew {
    /// (51차) 논의를 진행 신호 · 다시 시도(3번)와 함께 돈다. 신호는 논의와 나란히 차례대로 내고, 논의가 끝나면
    /// 줄을 다 낸 뒤에 돌아온다(마지막 상태가 이벤트로 남은 뒤).
    pub(super) async fn discuss_watched(
        &self,
        request: DiscussionRequest,
    ) -> Result<DiscussionOutcome, pi_agent::Error> {
        let (line, mut notes) = mpsc::unbounded_channel::<Note>();
        let signals = std::sync::Arc::new(Signals {
            board: Mutex::new(Board {
                room: request.room.clone(),
                topic: request.topic.clone(),
                started_at: self.clock.now().timestamp_millis(),
                seats: [WAITING; 6],
                calls: 0,
            }),
            clock: std::sync::Arc::clone(&self.clock),
            line,
        });
        let options: DiscussionOptions = self
            .discussion_options()
            .with_signals(signals)
            .with_turn_retries(TURN_RETRY_MAX);
        let _clear = ClearOnDrop(self);
        // (57차) 시작할 때 고른 담당 모델로 짠 팀으로 끝까지 돈다.
        let team = self.team_now();
        let discussing = team.discuss_with(request, options);
        let showing = async {
            while let Some((payload, called, last)) = notes.recv().await {
                if let Some(role) = called {
                    self.note_member_call(role);
                }
                if let Some(board) = self.meter.board.get() {
                    board.show(payload, last).await;
                }
            }
        };
        let (result, ()) = tokio::join!(discussing, showing);
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn board() -> Board {
        Board {
            room: "trade-1".into(),
            topic: "주제".into(),
            started_at: 5,
            seats: [WAITING; 6],
            calls: 0,
        }
    }

    fn state(json: &Value, agent: &str) -> Value {
        json["members"]
            .as_array()
            .unwrap()
            .iter()
            .find(|m| m["agent"] == agent)
            .unwrap()
            .clone()
    }

    #[test]
    fn 실패하면_오류_다음_다시_시도_n으로_진행이_되고_끝나면_지운다() {
        let mut b = board();
        let running = ProgressSignal::Member {
            role: Role::Strategy,
            stage: Stage::Running,
        };
        assert_eq!(b.apply(&running), Some(Role::Strategy));
        b.apply(&ProgressSignal::Member {
            role: Role::Strategy,
            stage: Stage::Error,
        });
        assert_eq!(state(&b.to_json(0, false), "strategy")["state"], "error");
        b.apply(&ProgressSignal::Retry {
            role: Role::Strategy,
            attempt: 1,
            max: 3,
        });
        b.apply(&running);
        let now = state(&b.to_json(0, false), "strategy");
        assert_eq!(now["state"], "running");
        assert_eq!(now["retry"], 1);
        b.apply(&ProgressSignal::Member {
            role: Role::Strategy,
            stage: Stage::Done,
        });
        let done = state(&b.to_json(0, false), "strategy");
        assert_eq!(done["state"], "done");
        assert!(done.get("retry").is_none());
        assert_eq!(b.calls, 2, "다시 시도도 호출 하나");
    }

    #[test]
    fn 점수_팀장_턴은_다음_턴이나_끝에서_완료가_된다() {
        let mut b = board();
        assert_eq!(
            b.apply(&ProgressSignal::ScoreTurn { role: Role::Scout }),
            Some(Role::Scout)
        );
        assert_eq!(b.apply(&ProgressSignal::LeaderTurn), Some(Role::Leader));
        let mid = b.to_json(9, false);
        assert_eq!(state(&mid, "news")["state"], "done");
        assert_eq!(state(&mid, "leader")["state"], "running");
        b.apply(&ProgressSignal::Closed);
        let end = b.to_json(10, true);
        assert_eq!(state(&end, "leader")["state"], "done");
        assert_eq!(state(&end, "review")["state"], "waiting");
        assert_eq!(end["done"], 2);
        assert_eq!(end["closed"], true);
        assert_eq!(end["started_at"], 5);
    }
}
