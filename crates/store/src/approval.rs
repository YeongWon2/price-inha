//! 승인 표 (9차 2번, M5). 원본: OpenClaw `operator_approvals`
//! (`docs/research/triggers-approval-memory-deep.md` 3.2 · 3.4).

use crate::model::UnixMillis;
use crate::{Store, StoreError};

/// 승인 요청 번호. 저장소가 1부터 붙이고 다시 쓰지 않는다(M5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ApprovalId(pub i64);

/// 승인 요청의 결과(M5). 처음은 [`ApprovalStatus::Pending`]이고, 기다림에서 **한 번만** 바뀐다.
///
/// 표 `approvals`의 칸 `status`에 아래 글자로 두고, 표의 CHECK가 이 다섯 글자만 받는다.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ApprovalStatus {
    /// 기다림 (`'pending'`)
    Pending,
    /// 승인 (`'approved'`)
    Approved,
    /// 거절 (`'rejected'`). 승인 확인에 실패한 것도 거절이다(호출하는 쪽이 정한다).
    Rejected,
    /// 답 없음 (`'no_answer'`)
    NoAnswer,
    /// 만료 (`'expired'`)
    Expired,
}

impl ApprovalStatus {
    /// 모든 결과.
    pub const ALL: [ApprovalStatus; 5] = [
        ApprovalStatus::Pending,
        ApprovalStatus::Approved,
        ApprovalStatus::Rejected,
        ApprovalStatus::NoAnswer,
        ApprovalStatus::Expired,
    ];
}

/// 새 승인 요청.
#[derive(Debug, Clone, PartialEq)]
pub struct NewApprovalRequest {
    /// 무엇을 승인하는지의 지문(예: 주문 내용의 SHA-256 글자). 그대로 저장한다.
    pub fingerprint: String,
    /// 사람에게 보일 내용(JSON). 그대로 저장한다.
    pub summary: serde_json::Value,
    /// 만든 시각.
    pub created_at: UnixMillis,
    /// 이 시각부터는 승인이 효력을 잃는다(답 · 소비 모두).
    pub expires_at: UnixMillis,
}

/// 저장된 승인 요청 한 건.
#[derive(Debug, Clone, PartialEq)]
pub struct ApprovalRequest {
    /// 번호.
    pub id: ApprovalId,
    /// 내용 지문.
    pub fingerprint: String,
    /// 사람에게 보일 내용.
    pub summary: serde_json::Value,
    /// 지금 결과.
    pub status: ApprovalStatus,
    /// 만든 시각.
    pub created_at: UnixMillis,
    /// 효력이 끝나는 시각.
    pub expires_at: UnixMillis,
    /// 기다림에서 바뀐 시각. 기다림이면 없음.
    pub resolved_at: Option<UnixMillis>,
    /// 실행에 쓴(소비한) 시각. 승인이고 소비했을 때만 있다.
    pub consumed_at: Option<UnixMillis>,
}

/// 승인을 실행에 쓰겠다는 요청: 번호와 **실행할 내용**의 지문.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApprovalUse {
    /// 쓸 승인 번호.
    pub id: ApprovalId,
    /// 실행할 내용의 지문. 승인한 내용의 지문과 글자까지 같아야 한다.
    pub fingerprint: String,
}

impl Store {
    /// 승인 요청을 기다림으로 만든다. 번호는 1부터 빠짐없이 커지고 다시 쓰지 않는다.
    pub fn create_approval(&self, new: NewApprovalRequest) -> Result<ApprovalRequest, StoreError> {
        let _ = new;
        todo!()
    }

    /// 승인 요청 한 건. 없으면 [`StoreError::ApprovalNotFound`].
    pub fn get_approval(&self, id: ApprovalId) -> Result<ApprovalRequest, StoreError> {
        let _ = id;
        todo!()
    }

    /// 그 결과인 승인 요청들. 번호 순서다.
    pub fn approvals_in_status(
        &self,
        status: ApprovalStatus,
    ) -> Result<Vec<ApprovalRequest>, StoreError> {
        let _ = status;
        todo!()
    }

    /// 기다림인 승인 요청의 결과를 정한다. 기다림에서 **한 번만** 바뀐다.
    ///
    /// - 기다림이 아니면(이미 답이 났으면) [`StoreError::TransitionDenied`]
    ///   ([`crate::Transition::Approval`], [`crate::DenyReason::NotAllowed`])이고 그대로다.
    ///   두 번째 답은 첫 답을 바꾸지 못한다.
    /// - `to`가 기다림이면 같은 오류다.
    /// - (원본과 같게) `at`이 `expires_at` 이상이면 어떤 답이든 [`ApprovalStatus::Expired`]로 닫고
    ///   그것을 돌려준다(오류가 아니다).
    /// - 되면 `resolved_at`은 `at`이다.
    pub fn resolve_approval(
        &self,
        id: ApprovalId,
        to: ApprovalStatus,
        at: UnixMillis,
    ) -> Result<ApprovalRequest, StoreError> {
        let _ = (id, to, at);
        todo!()
    }

    /// 승인을 실행에 쓴다(한 번만). `consumed_at`을 `at`으로 적는다.
    ///
    /// 아래를 이 순서로 보고, 하나라도 걸리면 아무것도 바꾸지 않는다:
    ///
    /// 1. 없으면 [`StoreError::ApprovalNotFound`].
    /// 2. 승인이 아니면 [`StoreError::ApprovalNotUsable`].
    /// 3. 이미 썼으면 [`StoreError::ApprovalAlreadyConsumed`].
    /// 4. `at`이 `expires_at` 이상이면 [`StoreError::ApprovalExpired`].
    /// 5. 지문이 다르면 [`StoreError::ApprovalFingerprintMismatch`](승인 뒤 바뀐 내용은 실행하지 않는다).
    ///
    /// 표에서도 지킨다: `approvals`의 CHECK와 트리거가 승인이 아닌 줄의 소비 칸, 한 번 적힌 소비 칸의
    /// 고치기, 기다림이 아닌 줄의 결과 바꾸기, 줄 지우기를 어떤 SQL로도 막는다.
    pub fn consume_approval(
        &self,
        approval: ApprovalUse,
        at: UnixMillis,
    ) -> Result<ApprovalRequest, StoreError> {
        let _ = (approval, at);
        todo!()
    }
}
