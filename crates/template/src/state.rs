//! 템플릿 상태와 허용 전이 표(`dev.md` M3, `spec.md` F6).
//!
//! 전이를 실제로 강제하는 것(E36: 승인 번호 필요, 전체 정지 → 실전중이 멈춤으로)은 store 가 한다.
//! 여기서는 이름과 표를 값으로 공개한다.

use serde::{Deserialize, Serialize};

/// 템플릿 상태.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TemplateState {
    /// 모의중.
    Paper,
    /// 승격대기.
    PendingPromotion,
    /// 실전중.
    Live,
    /// 멈춤.
    Stopped,
}

/// 전이가 일어나는 까닭.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransitionReason {
    /// 승격 기준 통과(F6 6).
    CriteriaPassed,
    /// 내가 승인(F6 8, 승인 번호 필요).
    Approved,
    /// 내가 거절(F6 7a).
    Rejected,
    /// 내가 실전 템플릿을 멈춤(F6 9a).
    UserStopped,
    /// 투자 팀 전체 정지(F6 9b).
    GlobalHalt,
}

/// 허용 전이 하나.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Transition {
    /// 전 상태.
    pub from: TemplateState,
    /// 뒤 상태.
    pub to: TemplateState,
    /// 까닭.
    pub reason: TransitionReason,
}

/// 허용 전이 표(F6). 여기에 없는 전이는 허용하지 않는다. 멈춤에서 나가는 전이는 스펙에 없어 두지 않는다.
pub const ALLOWED_TRANSITIONS: &[Transition] = &[
    Transition {
        from: TemplateState::Paper,
        to: TemplateState::PendingPromotion,
        reason: TransitionReason::CriteriaPassed,
    },
    Transition {
        from: TemplateState::PendingPromotion,
        to: TemplateState::Live,
        reason: TransitionReason::Approved,
    },
    Transition {
        from: TemplateState::PendingPromotion,
        to: TemplateState::Paper,
        reason: TransitionReason::Rejected,
    },
    Transition {
        from: TemplateState::Live,
        to: TemplateState::Stopped,
        reason: TransitionReason::UserStopped,
    },
    Transition {
        from: TemplateState::Live,
        to: TemplateState::Stopped,
        reason: TransitionReason::GlobalHalt,
    },
];

impl TemplateState {
    /// 새 템플릿이 시작하는 상태(V29: 모의중).
    pub const INITIAL: TemplateState = TemplateState::Paper;

    /// 화면에 보이는 이름: "모의중" · "승격대기" · "실전중" · "멈춤".
    pub fn label(self) -> &'static str {
        match self {
            TemplateState::Paper => "모의중",
            TemplateState::PendingPromotion => "승격대기",
            TemplateState::Live => "실전중",
            TemplateState::Stopped => "멈춤",
        }
    }

    /// `to` 로 가는 전이가 [`ALLOWED_TRANSITIONS`] 에 있으면 그 까닭을 준다(여럿이면 표 순서대로).
    pub fn reasons_to(self, to: TemplateState) -> Vec<TransitionReason> {
        ALLOWED_TRANSITIONS
            .iter()
            .filter(|t| t.from == self && t.to == to)
            .map(|t| t.reason)
            .collect()
    }
}
