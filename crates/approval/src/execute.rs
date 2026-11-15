//! 승인받은 실전 승격 · 규칙 개선을 저장소에 끝까지 넘긴다(2차 1번).
//!
//! 호출하는 쪽은 이 크레이트만 쓴다: 번호와 대상 번호를 주면 무효 번호 · 전체 정지를 확인하고,
//! 대상의 **지금** 모습에서 지문을 계산해 `pi-store`의 [`Store::promote_template_live`] ·
//! [`Store::approve_proposal`]을 부른다(승인 소비는 저장소가 같은 묶음 안에서 한다).
//!
//! [`Store::promote_template_live`]: pi_store::Store::promote_template_live
//! [`Store::approve_proposal`]: pi_store::Store::approve_proposal

use pi_store::{Proposal, ProposalId, Template, TemplateId};

use crate::approvals::{ApprovalNumber, Approvals};
use crate::error::Error;

impl Approvals {
    /// 승인 `number`로 템플릿 `template`을 승격대기 → 실전중으로 올린다. 시각은 시계에서 읽는다.
    ///
    /// 실행할 대상은 저장소에서 읽은 템플릿의 지금 판으로 만든
    /// [`Subject::LivePromotion`](crate::Subject::LivePromotion)이고, 그 지문을 승인한 지문과 비교한다.
    ///
    /// - 없는 번호 → [`Error::Denied`]([`RejectReason::NotFound`](crate::RejectReason::NotFound)).
    /// - 전체 정지 중 → `Denied(FullStop)`. 템플릿도 승인도 그대로다(다시 시작한 뒤에 쓸 수 있다).
    /// - 승인됨이 아님 `Denied(NotApproved)`, 이미 씀 `Denied(AlreadyConsumed)`.
    /// - 다른 대상의 승인이거나 승인 뒤 템플릿 값이 새 판이 됐으면 `Denied(FingerprintMismatch)`이고
    ///   그 승인은 무효가 된다([`Approvals::consume`]과 같다). 템플릿은 그대로다.
    /// - 템플릿이 없거나 승격대기가 아니면 [`Error::Store`]이고 승인은 쓰지 않는다.
    /// - 되면 실전중인 템플릿을 돌려주고, 승인은 소비된 것으로 보인다.
    pub fn promote_live(
        &self,
        number: &ApprovalNumber,
        template: TemplateId,
    ) -> Result<Template, Error> {
        let _ = (number, template);
        todo!()
    }

    /// 승인 `number`로 규칙 개선안 `proposal`을 승인한다(제안됨 → 반영됨, 위험을 키우면 냉각중).
    /// 시각은 시계에서 읽는다. 전체 정지 중에도 된다.
    ///
    /// 실행할 대상은 [`Subject::RuleImprovement`](crate::Subject::RuleImprovement)이고,
    /// 그 지문을 승인한 지문과 비교한다.
    ///
    /// - 없는 번호 → [`Error::Denied`]([`RejectReason::NotFound`](crate::RejectReason::NotFound)).
    /// - 승인됨이 아님 `Denied(NotApproved)`, 이미 씀 `Denied(AlreadyConsumed)`.
    /// - 다른 대상의 승인이면 `Denied(FingerprintMismatch)`이고 그 승인은 무효가 된다. 개선안은 그대로다.
    /// - 개선안이 없거나 제안됨이 아니면 [`Error::Store`]이고 승인은 쓰지 않는다.
    /// - 되면 바뀐 개선안을 돌려주고, 승인은 소비된 것으로 보인다.
    pub fn approve_proposal(
        &self,
        number: &ApprovalNumber,
        proposal: ProposalId,
    ) -> Result<Proposal, Error> {
        let _ = (number, proposal);
        todo!()
    }
}
