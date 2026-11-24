//! 담당과 "있어야 할 곳".
//!
//! 서버는 지도를 모르고 자리 이름만 정한다(`docs/research/office-deep.md` 1-4 · 11-3).

pub use pi_store::TeamState;
use pi_store::UnixMillis;

use crate::summon::{SummonPhase, SummonReason};

/// 담당 6명(V1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Agent {
    /// 뉴스 공시 시세 담당. id `"news"`.
    News,
    /// 전략 담당. id `"strategy"`.
    Strategy,
    /// 리스크 담당. id `"risk"`.
    Risk,
    /// 복기 담당. id `"review"`.
    Review,
    /// 매매 담당. id `"trader"`.
    Trader,
    /// 팀장. id `"leader"`.
    Leader,
}

impl Agent {
    /// 모든 담당. 이 순서로 스냅샷을 준다.
    pub const ALL: [Agent; 6] = [
        Agent::News,
        Agent::Strategy,
        Agent::Risk,
        Agent::Review,
        Agent::Trader,
        Agent::Leader,
    ];

    /// 이벤트와 지도 정의 파일에 쓰는 이름: `news`, `strategy`, `risk`, `review`, `trader`, `leader`.
    pub fn id(self) -> &'static str {
        match self {
            Agent::News => "news",
            Agent::Strategy => "strategy",
            Agent::Risk => "risk",
            Agent::Review => "review",
            Agent::Trader => "trader",
            Agent::Leader => "leader",
        }
    }

    /// [`Agent::id`]의 거꾸로. 모르는 이름이면 `None`.
    pub fn from_id(id: &str) -> Option<Agent> {
        Agent::ALL.into_iter().find(|a| a.id() == id)
    }

    /// [`Agent::ALL`] 안의 자리 번호(0~5).
    pub(crate) fn index(self) -> usize {
        match self {
            Agent::News => 0,
            Agent::Strategy => 1,
            Agent::Risk => 2,
            Agent::Review => 3,
            Agent::Trader => 4,
            Agent::Leader => 5,
        }
    }
}

/// 담당이 있어야 할 곳. 좌표가 아니라 이름이다(지도는 화면이 안다).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Place {
    /// 자기 방 침대. 이벤트 이름 `"bed"`.
    Bed,
    /// 사무실 자기 책상. 이벤트 이름 `"desk"`.
    Desk,
    /// 논의 탁자. 이벤트 이름 `"meeting"`.
    Meeting,
}

impl Place {
    /// 이벤트에 쓰는 이름: `bed`, `desk`, `meeting`.
    pub fn as_str(self) -> &'static str {
        match self {
            Place::Bed => "bed",
            Place::Desk => "desk",
            Place::Meeting => "meeting",
        }
    }
}

/// 담당 활동(있어야 할 곳을 정하는 쪽).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Activity {
    /// 할 일이 없다.
    Idle,
    /// 일하는 중(답 쓰기 · 판단 등).
    Working,
    /// 팀 논의에 참가 중.
    Discussing,
}

/// 담당의 일 상태(화면의 표시 쪽). 채팅 응답에서 나온다(`office-deep.md` 2-2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Work {
    /// 하는 일 없음. 이벤트 이름 `"idle"`.
    Idle,
    /// 답 대기열에 있음. 이벤트 이름 `"queued"`.
    Queued,
    /// 생각 중. 이벤트 이름 `"thinking"`.
    Thinking,
    /// 마지막 답이 실패함(E6). 이벤트 이름 `"failed"`.
    Failed,
}

impl Work {
    /// 이벤트에 쓰는 이름.
    pub fn as_str(self) -> &'static str {
        match self {
            Work::Idle => "idle",
            Work::Queued => "queued",
            Work::Thinking => "thinking",
            Work::Failed => "failed",
        }
    }
}

/// 담당 한 명의 지금 모습. 이벤트 `office.agent`의 내용과 같다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AgentView {
    /// 담당.
    pub agent: Agent,
    /// 있어야 할 곳.
    pub place: Place,
    /// 일 상태.
    pub work: Work,
    /// 부르기 단계(결정 17 · D12). 걷는 길은 화면이 정하고 서버는 단계만 쥔다.
    pub phase: SummonPhase,
    /// 단계가 `Idle`이 아닐 때 왜 왔는가. `Idle`이면 `None`.
    pub reason: Option<SummonReason>,
    /// 단계가 마지막으로 바뀐 시각.
    pub phase_at: UnixMillis,
}

impl AgentView {
    /// 이벤트 `office.agent`의 내용: `{"agent", "place", "work", "phase", "reason", "phaseAt"}`.
    /// 앞의 세 칸은 옛 화면이 그대로 읽고, 뒤의 세 칸은 새 칸이다.
    pub(crate) fn to_json(self) -> serde_json::Value {
        serde_json::json!({
            "agent": self.agent.id(),
            "place": self.place.as_str(),
            "work": self.work.as_str(),
            "phase": self.phase.as_str(),
            "reason": self.reason.map(SummonReason::as_str),
            "phaseAt": self.phase_at.0,
        })
    }
}

/// 팀 상태와 담당 활동으로 있어야 할 곳을 정한다(V13 · V15, 결정 2026-11-16 25번).
///
/// - 출근 · 퇴근은 팀 상태(M1)에 묶는다: 운용 중이 아니면(쉬는 중 · 멈춤 · 정지) 활동과 상관없이 침대.
/// - 운용 중이면 논의 참가는 논의 탁자, 그 밖은 자기 책상.
pub fn place_of(team: TeamState, activity: Activity) -> Place {
    if team != TeamState::Operating {
        return Place::Bed;
    }
    match activity {
        Activity::Discussing => Place::Meeting,
        Activity::Idle | Activity::Working => Place::Desk,
    }
}
