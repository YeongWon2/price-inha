//! 규칙 개선안 표 (9차 3번, M4).
//!
//! 무엇을 고치자는지와 그 변경이 위험을 키우는지는 복기(`pi-journal`)가 정한다. 저장소는 상태표에 없는
//! 전이를 막고, 반영은 승인을 소비한 뒤에만 되게 한다.

use crate::approval::{ApprovalId, ApprovalUse};
use crate::model::{TradeId, UnixMillis};
use crate::{Store, StoreError};

/// 개선안을 내는 데 필요한 근거 매매의 가장 적은 수(M4 "2건 이상이어야 제안된다").
pub const PROPOSAL_MIN_EVIDENCE: usize = 2;

/// 규칙 개선안 번호. 저장소가 1부터 붙인다.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ProposalId(pub i64);

/// 규칙 개선안 상태(`spec.md` 상태표 · M4, F5).
///
/// 허용 전이(이 밖은 모두 거부):
///
/// | 에서 | 으로 | 함수 |
/// |---|---|---|
/// | 제안됨 | 반영됨 | [`Store::approve_proposal`] (위험을 키우지 않는 개선안) |
/// | 제안됨 | 냉각중 | [`Store::approve_proposal`] (위험을 키우는 개선안) |
/// | 제안됨 | 거절됨 | [`Store::reject_proposal`] |
/// | 냉각중 | 반영됨 | [`Store::apply_cooled_proposal`] |
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProposalState {
    /// 제안됨
    Proposed,
    /// 냉각중
    Cooling,
    /// 반영됨
    Applied,
    /// 거절됨
    Rejected,
}

impl ProposalState {
    /// 모든 상태.
    pub const ALL: [ProposalState; 4] = [
        ProposalState::Proposed,
        ProposalState::Cooling,
        ProposalState::Applied,
        ProposalState::Rejected,
    ];
}

/// 새 규칙 개선안.
#[derive(Debug, Clone, PartialEq)]
pub struct NewProposal {
    /// 고치자는 내용(JSON). 그대로 저장한다.
    pub content: serde_json::Value,
    /// 같은 실수가 나온 매매들. 서로 다른 매매가 [`PROPOSAL_MIN_EVIDENCE`]건 이상이어야 한다.
    pub evidence: Vec<TradeId>,
    /// 한도나 손절을 넓히는 변경인가. 참이면 승인 뒤 냉각중을 거친다.
    pub increases_risk: bool,
    /// 낸 시각.
    pub at: UnixMillis,
}

/// 저장된 규칙 개선안.
#[derive(Debug, Clone, PartialEq)]
pub struct Proposal {
    /// 번호.
    pub id: ProposalId,
    /// 지금 상태.
    pub state: ProposalState,
    /// 고치자는 내용.
    pub content: serde_json::Value,
    /// 근거 매매. 겹친 번호는 한 번만, 처음 나온 순서대로다.
    pub evidence: Vec<TradeId>,
    /// 위험을 키우는 변경인가.
    pub increases_risk: bool,
    /// 승인할 때 소비한 승인 번호. 냉각중 · 반영됨이면 언제나 있다.
    pub approval: Option<ApprovalId>,
    /// 낸 시각.
    pub created_at: UnixMillis,
    /// 마지막으로 바뀐 시각.
    pub updated_at: UnixMillis,
    /// 반영된 시각. 반영됨일 때만 있다.
    pub applied_at: Option<UnixMillis>,
}

impl Store {
    /// 규칙 개선안을 제안됨으로 만든다.
    ///
    /// - 서로 다른 근거 매매가 [`PROPOSAL_MIN_EVIDENCE`]건보다 적으면 [`StoreError::ProposalEvidenceTooFew`].
    /// - 없는 매매 번호가 있으면 [`StoreError::TradeNotFound`].
    /// - 거부되면 아무것도 만들지 않는다(번호도 쓰지 않는다).
    pub fn create_proposal(&self, new: NewProposal) -> Result<Proposal, StoreError> {
        let _ = new;
        todo!()
    }

    /// 규칙 개선안 한 건. 없으면 [`StoreError::ProposalNotFound`].
    pub fn get_proposal(&self, id: ProposalId) -> Result<Proposal, StoreError> {
        let _ = id;
        todo!()
    }

    /// 그 상태에 있는 규칙 개선안들. 번호 순서다.
    pub fn proposals_in_state(&self, state: ProposalState) -> Result<Vec<Proposal>, StoreError> {
        let _ = state;
        todo!()
    }

    /// 내가 승인한 개선안을 넘긴다: 위험을 키우지 않으면 반영됨(`applied_at = at`), 키우면 냉각중.
    /// 승인을 [`Store::consume_approval`]과 같은 규칙으로 같은 묶음 안에서 소비한다.
    ///
    /// - 제안됨이 아니면 [`StoreError::TransitionDenied`]([`crate::Transition::Proposal`],
    ///   [`crate::DenyReason::NotAllowed`])이고 개선안도 승인도 그대로다. 오류의 `to`는 그 개선안이
    ///   갔을 상태다(위험을 키우면 냉각중, 아니면 반영됨).
    /// - 상태를 먼저 보고 그다음 승인을 본다.
    /// - 승인을 쓸 수 없으면 그 오류이고 개선안도 승인도 그대로다(반영됨은 내 승인 뒤에만).
    pub fn approve_proposal(
        &self,
        id: ProposalId,
        approval: ApprovalUse,
        at: UnixMillis,
    ) -> Result<Proposal, StoreError> {
        let _ = (id, approval, at);
        todo!()
    }

    /// 제안됨 → 거절됨. 제안됨이 아니면 [`StoreError::TransitionDenied`]이고 그대로다.
    pub fn reject_proposal(&self, id: ProposalId, at: UnixMillis) -> Result<Proposal, StoreError> {
        let _ = (id, at);
        todo!()
    }

    /// 냉각중 → 반영됨(`applied_at = at`). 냉각중이 아니면 [`StoreError::TransitionDenied`]이고 그대로다.
    ///
    /// "다음 거래일이 됐는가"는 저장소가 알 수 없다(달력을 모른다). 호출하는 쪽 책임이다.
    pub fn apply_cooled_proposal(
        &self,
        id: ProposalId,
        at: UnixMillis,
    ) -> Result<Proposal, StoreError> {
        let _ = (id, at);
        todo!()
    }
}
