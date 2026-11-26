//! 논의 진행 신호 · 호출 예산 · 논의 옵션(10차, C21 · M13 · E51 · E52 · N14).
//!
//! 기존 [`crate::DiscussionRequest`]에 칸을 더하지 않는다(다른 크레이트가 글자로 만든다).
//! 신호 포트와 예산은 [`DiscussionOptions`]로 [`crate::Team::discuss_with`]에 넘긴다.
//! 아무것도 넘기지 않으면(기본) [`crate::Team::discuss`]와 똑같이 돈다.

use std::sync::Arc;

use pi_store::UnixMillis;

use crate::error::SignalError;
use crate::role::Role;

/// 정리 모드에서 팀장 턴에 넣는 한 줄(E52).
pub const WRAP_UP_LINE: &str = "호출이 얼마 남지 않았으니 지금까지의 글로 정리하라";

/// 다시 시도 상한(E51): 3번까지.
pub const TURN_RETRY_MAX: u32 = 3;

/// 담당 하나의 진행 단계(C21).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    /// 대기.
    Waiting,
    /// 진행.
    Running,
    /// 완료.
    Done,
    /// 오류.
    Error,
}

/// 논의가 도는 동안 내는 신호 하나.
///
/// 순서(담당 턴 하나): `Member{Running}` → 끝나면 `Member{Done}`, 실패면 `Member{Error}`.
/// 다시 시도하면 `Member{Error}` 다음에 `Retry{attempt, max}`, 그리고 다시 `Member{Running}`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProgressSignal {
    /// 논의 시작. 참가 담당 모두 대기다(명단 순서).
    Opened {
        /// 참가 담당.
        members: Vec<Role>,
    },
    /// 담당 턴의 단계가 바뀜(논의 턴): 시작 `Running`, 끝 `Done`, 실패 `Error`.
    Member {
        /// 담당.
        role: Role,
        /// 새 단계.
        stage: Stage,
    },
    /// 실패한 담당 턴을 다시 시도한다(`attempt`/`max`, 예: 1/3).
    Retry {
        /// 담당.
        role: Role,
        /// 몇 번째 다시 시도인가(1부터).
        attempt: u32,
        /// 상한.
        max: u32,
    },
    /// 점수 턴 시작.
    ScoreTurn {
        /// 담당.
        role: Role,
    },
    /// 팀장 판단 턴 시작.
    LeaderTurn,
    /// 논의 끝(어떤 결과든 마지막에 한 번).
    Closed,
}

/// 진행 신호를 받는 포트. 신호를 내다 실패해도 논의는 그대로 간다.
pub trait ProgressSink: Send + Sync {
    /// 신호 하나를 받는다.
    fn signal(&self, signal: ProgressSignal) -> Result<(), SignalError>;
}

/// 부르는 쪽이 넘기는 남은 AI 호출 수와 문턱(N14). 값은 부르는 쪽이 정한다.
///
/// 호출 수는 담당 턴(논의 · 점수 · 팀장 · 다시 시도)마다 1로 센다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CallBudget {
    /// 지금 남은 호출 수.
    pub remaining: u32,
    /// 팀장 예비분. 팀장 판단 턴만 쓴다. 담당 턴은 `remaining - reserve`까지만 쓴다.
    pub reserve: u32,
    /// 정리 모드 문턱(예비분 + 상한의 20%). 남은 호출이 이 값 이하면 새 라운드를 열지 않는다.
    pub wrap_up_at: u32,
}

/// 판단의 자료 목록(sources)에 공지를 한 줄로 남길 때 쓰는 이름(R17-2).
pub const NOTICE_SOURCE_NAME: &str = "사용자 공지";

/// 논의 자료로 넘기는 사용자 공지 하나(11차, R17-2). server가 저장소의 공지에서 글자로 만든다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoticeNote {
    /// 공지 글(가리기 전).
    pub text: String,
    /// 보낸 시각.
    pub sent_at: UnixMillis,
}

/// 논의 옵션. 기본은 신호 없음 · 예산 없음 · 다시 시도 없음 · 공지 없음이다.
#[derive(Clone, Default)]
pub struct DiscussionOptions {
    pub(crate) budget: Option<CallBudget>,
    pub(crate) signals: Option<Arc<dyn ProgressSink>>,
    pub(crate) turn_retries: u32,
    /// 사용자 공지(보낸 시각 순서). 비어 있으면 공지 없음이다.
    pub(crate) notices: Vec<NoticeNote>,
    /// (기능 L) 팀 지침 줄(고른 순서). 비어 있으면 블록이 없다.
    pub(crate) guidelines: Vec<String>,
}

impl DiscussionOptions {
    /// 남은 호출 예산을 넘긴다.
    pub fn with_budget(mut self, budget: CallBudget) -> DiscussionOptions {
        self.budget = Some(budget);
        self
    }

    /// 진행 신호 포트를 넘긴다.
    pub fn with_signals(mut self, sink: Arc<dyn ProgressSink>) -> DiscussionOptions {
        self.signals = Some(sink);
        self
    }

    /// 담당 턴이 실패하면 최대 `retries`번(상한 [`TURN_RETRY_MAX`]) 다시 시도한다. 기본 0.
    pub fn with_turn_retries(mut self, retries: u32) -> DiscussionOptions {
        self.turn_retries = retries.min(TURN_RETRY_MAX);
        self
    }

    /// 사용자 공지 글들을 논의 자료로 넘긴다(11차, R17-2).
    ///
    /// 담당 브리핑의 자료 뒤에 "사용자 공지" 묶음으로 들어간다(가린 글, 보낸 시각 순서).
    /// 자료 완전성(E32)에는 들지 않는다. 판단의 자료 목록에는 이름 [`NOTICE_SOURCE_NAME`]과
    /// 가장 늦은 보낸 시각으로 한 줄 들어간다. 빈 목록이면 넘기지 않은 것과 같다.
    pub fn with_notices(mut self, mut notices: Vec<NoticeNote>) -> DiscussionOptions {
        // 보낸 시각이 같으면 받은 순서를 지킨다(안정 정렬).
        notices.sort_by_key(|note| note.sent_at.0);
        self.notices = notices;
        self
    }

    /// (기능 L · 결정 30) 팀 지침 줄을 넘긴다. 담당 브리핑의 자료 · 공지 뒤 정해진 자리에 "팀 지침" 블록으로 든다
    /// (가린 글, 받은 순서, 개수 [`crate::GUIDELINE_LINES_MAX`] · 글자 [`crate::GUIDELINE_CHARS_MAX`] 상한 안).
    /// 빈 목록이면 블록이 없다. 자료 완전성(E32)이나 판단의 자료 목록에는 들지 않는다.
    pub fn with_guidelines(mut self, lines: Vec<String>) -> DiscussionOptions {
        self.guidelines = lines;
        self
    }
}
