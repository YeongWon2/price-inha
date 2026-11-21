//! 답 없음 · 만료 뒤 처리 기록(E22 "보이는 것", 4차).

use crate::approvals::{ApprovalNumber, Approvals, DueOutcome};
use crate::error::Error;

/// 답 없음 · 만료로 닫힌 주문 요청을 호출하는 쪽이 어떻게 처리했는지(E22).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum DueHandling {
    /// 금액 한도 안으로 줄여 냈다(FN2 3b3). 줄인 수량(0보다 크고 요청 수량 이하). 셈은 rules가 했다.
    Reduced {
        /// 줄인 수량.
        quantity: u64,
    },
    /// 사지 않았다(P20, 결정 5번).
    NotBought,
    /// 팀이 팔기로 판단했다(FN2 3b2).
    TeamSold,
    /// 팀이 들고 있기로 판단했다(FN2 3b2).
    TeamKept,
    /// 다시 점검에서 막혔다. 막힌 까닭 글(가리는 일은 부르는 쪽, E38).
    Blocked {
        /// 막힌 까닭(가린 것).
        reason: String,
    },
    /// 신호가 지나 아무것도 하지 않았다.
    Dropped,
}

impl Approvals {
    /// 답 없음 · 만료로 닫힌 주문 요청의 처리 기록을 한 번만 남긴다(E22).
    ///
    /// - 주문 대상이고 결과가 답 없음 또는 만료일 때만 받는다. 아니면(번호가 없어도) [`Error::NotDue`].
    /// - 두 번째 기록은 [`Error::AlreadyHandled`]이고 처음 기록이 그대로다.
    /// - 할 일([`expire_due`](Approvals::expire_due)와 같은 규칙으로 다시 셈)과 맞지 않으면 [`Error::NotDue`]:
    ///   `ReduceWithinLimit` ← `Reduced` · `Blocked` · `Dropped`, `DoNotBuy` ← `NotBought`,
    ///   `TeamDecides` ← `TeamSold` · `TeamKept` · `Blocked` · `Dropped`, `Dropped` ← `Dropped`.
    /// - `Reduced { quantity }`는 0보다 크고 요청 수량 이하여야 한다(아니면 [`Error::NotDue`]).
    /// - 저장소 이벤트로 남아 다시 켠 뒤에도 읽힌다.
    pub fn record_due_handling(
        &self,
        number: &ApprovalNumber,
        handling: DueHandling,
    ) -> Result<(), Error> {
        let _ = (number, handling);
        todo!()
    }

    /// 남긴 처리 기록. 없으면 `None`.
    pub fn due_handling(&self, number: &ApprovalNumber) -> Result<Option<DueHandling>, Error> {
        let _ = number;
        todo!()
    }

    /// 답 없음 · 만료로 닫힌 주문 요청 가운데 처리 기록이 없는 것을 닫힌 순서로 돌려준다.
    ///
    /// 할 일은 [`expire_due`](Approvals::expire_due)와 같은 규칙이되, 지금 시각(시계)이 신호 유효 시간의
    /// 끝 이후면 [`DueAction::Dropped`](crate::DueAction::Dropped)다. 읽기만 한다.
    pub fn unhandled_due(&self) -> Result<Vec<DueOutcome>, Error> {
        todo!()
    }
}
