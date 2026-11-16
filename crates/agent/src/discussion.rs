//! 팀 논의 방의 입력 · 결과 값(만드는 것 3 · 4 · 5, 결정 14 · 15번). 로직은 `team.rs`에 있다.

use pi_store::{RoleScore, TradeId, UnixMillis};

use crate::ports::OrderDraft;
use crate::role::Role;

/// 논의 상한(결정 14번, 원본 Hermes의 숫자).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiscussionLimits {
    /// 라운드 상한.
    pub max_rounds: u32,
    /// 논의 하나의 담당 글 수 상한.
    pub max_messages: u32,
    /// 참가 담당 수 상한.
    pub max_members: usize,
}

impl Default for DiscussionLimits {
    /// 3라운드 · 10글 · 6명.
    fn default() -> Self {
        DiscussionLimits {
            max_rounds: 3,
            max_messages: 10,
            max_members: 6,
        }
    }
}

/// 판단에 필요한 자료 하나.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataItem {
    /// 자료 이름.
    pub name: String,
    /// 자료 내용. 가져오지 못했으면 `None`(E32).
    pub content: Option<String>,
    /// 알게 된 시각. 판단의 자료(`SourceRef`)로 그대로 남는다.
    pub known_at: UnixMillis,
}

/// 논의 요청.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscussionRequest {
    /// 방 이름(방 로그의 `room`).
    pub room: String,
    /// 논의중인 매매.
    pub trade: TradeId,
    /// 계기(첫 글로 지명된 담당에게 간다).
    pub topic: String,
    /// 참가 담당. 2명 이상 [`DiscussionLimits::max_members`]명 이하, 겹치지 않는다. 이 순서가 명단 순서다.
    pub members: Vec<Role>,
    /// 0라운드에 지명된 담당. 하나 이상, 모두 참가 담당이다.
    pub nominated: Vec<Role>,
    /// 판단에 필요한 자료. 하나라도 비면 지켜보기(E32).
    pub data: Vec<DataItem>,
    /// 템플릿의 합의 점수 기준.
    pub consensus_required: i32,
    /// 논의 상한.
    pub limits: DiscussionLimits,
}

/// 지켜보기로 남긴 까닭. 방 로그 `room.activity`의 `reason_code`는 [`WatchReason::code`]다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WatchReason {
    /// 필요한 자료가 비었다(E32). 빈 자료 이름들, 요청 순서대로. 코드 `missing_data`.
    MissingData(Vec<String>),
    /// 담당 하나가 답하지 못했다(E6) — 논의 · 점수 · 판단 어느 턴이든. 코드 `role_unavailable`.
    RoleUnavailable(Role),
    /// 논의가 라운드 · 글 상한에 닿았다(결정 14번). 코드 `discussion_bounded`.
    DiscussionBounded,
    /// 합의 점수가 기준을 넘지 못했다(E27). 코드 `consensus_below`.
    ConsensusBelow {
        /// 합의 점수.
        consensus: i32,
        /// 기준.
        required: i32,
    },
    /// 팀장이 사기로 정했지만 근거 칸이 비었다(P9). 코드 `rationale_incomplete`.
    RationaleIncomplete,
    /// 팀장이 지켜보기로 정했다. 코드 `leader_watch`.
    LeaderWatch {
        /// 지켜보는 이유.
        reason: String,
        /// 다시 볼 조건.
        revisit: String,
    },
}

impl WatchReason {
    /// 방 로그에 남기는 까닭 코드.
    pub fn code(&self) -> &'static str {
        match self {
            WatchReason::MissingData(_) => "missing_data",
            WatchReason::RoleUnavailable(_) => "role_unavailable",
            WatchReason::DiscussionBounded => "discussion_bounded",
            WatchReason::ConsensusBelow { .. } => "consensus_below",
            WatchReason::RationaleIncomplete => "rationale_incomplete",
            WatchReason::LeaderWatch { .. } => "leader_watch",
        }
    }
}

/// 종목 판단.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    /// 사기. 매매는 판단됨이고 주문 안이 있다(V4).
    Buy,
    /// 팔기. 매매는 판단됨이다. 팔기 주문 안은 이 단계에서 만들지 않는다.
    Sell,
    /// 지켜보기. 매매는 논의중 그대로이고 주문 안이 없다.
    Watch(WatchReason),
}

/// 논의 하나의 결과.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscussionOutcome {
    /// 종목 판단.
    pub decision: Decision,
    /// 점수를 낸 담당들의 점수(참가 명단 순서). 점수 턴까지 가지 않았으면 빈 목록.
    pub scores: Vec<RoleScore>,
    /// 합의 점수. 점수 턴을 모두 마치지 못했으면 `None`.
    pub consensus: Option<i32>,
    /// 매매 담당의 주문 안. 사기로 판단되고 주문 안이 만들어졌을 때만 있다.
    pub order: Option<OrderDraft>,
}
