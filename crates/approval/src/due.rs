//! 기한이 된 기다림 닫기(E22)와 다시 켠 뒤 정리(결정 6번).

use pi_store::{ApprovalStatus, StoreError, UnixMillis};

use crate::approvals::{ApprovalNumber, Approvals, DueOutcome};
use crate::error::Error;
use crate::handling::due_action;
use crate::record;
use crate::subject::Subject;

impl Approvals {
    /// 기한이 된 기다림 요청을 닫고 처리 방침을 돌려준다(E22).
    ///
    /// - 주문이고 `wait_until ≤ now < expires_at`: 결과를 답 없음으로 바꾼다. 사는 주문은 알림이 갔으면
    ///   [`DueAction::ReduceWithinLimit`], 아니면 [`DueAction::DoNotBuy`]. 파는 주문은 [`DueAction::TeamDecides`].
    /// - 주문이고 `now ≥ expires_at`: 만료로 닫고 [`DueAction::Dropped`].
    /// - 실전 승격 · 규칙 개선 · 다시 시작: 바꾸지 않고 돌려주지도 않는다(계속 기다린다, V42).
    /// - 한 번 닫은 요청은 다시 돌려주지 않는다.
    pub fn expire_due(&self, now: UnixMillis) -> Result<Vec<DueOutcome>, Error> {
        let mut out = Vec::new();
        for found in record::pending_records(&self.store)? {
            let Subject::Order(order) = &found.subject else {
                continue;
            };
            let due = found.wait_until.is_some_and(|t| now.0 >= t.0);
            if !due && !found.expired_at(now) {
                continue;
            }
            let done = match self
                .store
                .resolve_approval(found.id, ApprovalStatus::NoAnswer, now)
            {
                Ok(done) => done,
                Err(StoreError::TransitionDenied { .. }) => continue,
                Err(err) => return Err(err.into()),
            };
            let action = due_action(done.status, order, self.notice_of(&found.number)?);
            out.push(DueOutcome {
                number: found.number.clone(),
                subject: found.subject.clone(),
                status: done.status,
                action,
            });
        }
        Ok(out)
    }

    /// 프로그램을 다시 켰을 때 부른다(결정 6번). 시각은 시계에서 읽는다.
    ///
    /// 기다림 요청 가운데 신호 유효 시간이 지난 것(지금 ≥ 만료 시각)은 만료로 닫고 그 번호들을 돌려준다.
    /// 유효 시간 안인 것과 만료 시각이 없는 것은 기다림으로 둔다.
    pub fn on_restart(&self) -> Result<Vec<ApprovalNumber>, Error> {
        let now = self.clock.now();
        let mut out = Vec::new();
        for found in record::pending_records(&self.store)? {
            if !found.expired_at(now) {
                continue;
            }
            match self
                .store
                .resolve_approval(found.id, ApprovalStatus::Expired, now)
            {
                Ok(done) if done.status == ApprovalStatus::Expired => out.push(found.number),
                Ok(_) | Err(StoreError::TransitionDenied { .. }) => {}
                Err(err) => return Err(err.into()),
            }
        }
        Ok(out)
    }
}
