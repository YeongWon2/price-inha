//! 승인을 실행에 쓴다: 소비, 그리고 승인받은 실전 승격 · 규칙 개선을 저장소에 끝까지 넘긴다(2차 1번).
//!
//! 호출하는 쪽은 이 크레이트만 쓴다: 번호와 대상 번호를 주면 무효 번호 · 전체 정지를 확인하고,
//! 대상의 **지금** 모습에서 지문을 계산해 `pi-store`의 [`Store::promote_template_live`] ·
//! [`Store::approve_proposal`]을 부른다(승인 소비는 저장소가 같은 묶음 안에서 한다).
//!
//! [`Store::promote_template_live`]: pi_store::Store::promote_template_live
//! [`Store::approve_proposal`]: pi_store::Store::approve_proposal

use pi_store::{
    ApprovalStatus, ApprovalUse, Proposal, ProposalId, StoreError, TeamState, Template, TemplateId,
    UnixMillis,
};

use crate::approvals::{ApprovalNumber, Approvals};
use crate::error::{Error, RejectReason};
use crate::record::Record;
use crate::subject::{Fingerprint, Subject, fingerprint, same_fingerprint};

impl Approvals {
    /// 승인을 실행에 쓴다(한 번만). `executing`은 **실행할** 대상이고, 그 지문을 승인한 지문과 비교한다.
    ///
    /// 쓸 수 없으면 [`Error::Denied`]이고 실행하지 않는다:
    /// 없는 번호 `NotFound`, 승인됨이 아님 `NotApproved`, 이미 씀 `AlreadyConsumed`,
    /// 만료 `Expired`, 지문이 다름 `FingerprintMismatch`, 전체 정지 중의 실전 승격 `FullStop`.
    ///
    /// 지문이 달라 거부된 승인은 거절로 바뀐 것으로 다룬다: 그 뒤에는 같은 번호를 어떤 대상으로도 쓸 수 없다(E7).
    /// 동시에 여러 번 불러도 한 번만 통과한다.
    pub fn consume(&self, number: &ApprovalNumber, executing: &Subject) -> Result<(), Error> {
        let now = self.clock.now();
        let found = self.known_record(number)?;
        self.use_approval(found, number, executing, now, |using, at| {
            self.store.consume_approval(using, at).map(|_| ())
        })
    }

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
        let now = self.clock.now();
        let found = self.known_record(number)?;
        let current = self.store.get_template(template)?;
        let executing = Subject::LivePromotion {
            template,
            revision: current.revision,
        };
        self.use_approval(found, number, &executing, now, |using, at| {
            self.store.promote_template_live(template, using, at)
        })
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
        let now = self.clock.now();
        let found = self.known_record(number)?;
        let executing = Subject::RuleImprovement { proposal };
        self.use_approval(found, number, &executing, now, |using, at| {
            self.store.approve_proposal(proposal, using, at)
        })
    }

    /// 번호의 승인 줄. 없는 번호면 `NotFound`, 실행 때 지문이 달라 무효가 된 번호면
    /// `FingerprintMismatch`(E7: 어떤 대상으로도 다시 쓸 수 없다).
    fn known_record(&self, number: &ApprovalNumber) -> Result<Record, Error> {
        let mut memo = self.memo()?;
        let Some(found) = memo.find(&self.store, number)? else {
            return Err(Error::Denied(RejectReason::NotFound));
        };
        if memo.is_voided(number) {
            return Err(Error::Denied(RejectReason::FingerprintMismatch));
        }
        Ok(found)
    }

    /// 승인을 먼저 본다(대상의 상태보다 앞): 쓸 수 없으면 그 까닭이고 지문이 달랐으면 그 번호를 무효로
    /// 기록한다. 쓸 수 있는데 실행할 대상이 실전 승격이고 전체 정지 중이면 `FullStop`(승인은 그대로).
    /// 아니면 `apply`로 저장소에 넘긴다. 저장소가 같은 규칙으로 같은 묶음 안에서 다시 보고 소비한다
    /// (동시에 불러도 한 번만). 저장소가 대상을 거부하면 [`Error::Store`]이고 승인은 쓰지 않는다.
    fn use_approval<T>(
        &self,
        found: Record,
        number: &ApprovalNumber,
        executing: &Subject,
        now: UnixMillis,
        apply: impl FnOnce(ApprovalUse, UnixMillis) -> Result<T, StoreError>,
    ) -> Result<T, Error> {
        let fp = fingerprint(executing);
        if let Some(err) = unusable(&found, &fp, now) {
            return Err(self.consume_denied(err, number, now));
        }
        if matches!(executing, Subject::LivePromotion { .. })
            && self.store.team_state()? == TeamState::FullStop
        {
            return Err(Error::Denied(RejectReason::FullStop));
        }
        let using = ApprovalUse {
            id: found.id,
            fingerprint: fp.as_str().to_string(),
        };
        apply(using, now).map_err(|err| self.consume_denied(err, number, now))
    }
}

/// 저장소의 소비 규칙과 같은 순서로 본 쓸 수 없는 까닭(승인됨 → 아직 안 씀 → 만료 전 → 같은 지문).
fn unusable(found: &Record, fp: &Fingerprint, now: UnixMillis) -> Option<StoreError> {
    let id = found.id;
    if found.status != ApprovalStatus::Approved {
        Some(StoreError::ApprovalNotUsable {
            id,
            status: found.status,
        })
    } else if found.consumed {
        Some(StoreError::ApprovalAlreadyConsumed { id })
    } else if found.expired_at(now) {
        Some(StoreError::ApprovalExpired { id })
    } else if !same_fingerprint(found.fingerprint.as_str(), fp.as_str()) {
        Some(StoreError::ApprovalFingerprintMismatch { id })
    } else {
        None
    }
}
