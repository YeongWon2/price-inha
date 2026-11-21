//! 승인 답하기(C7 · E7 · E8 · T14): 승인 증표는 승인 모듈이 소비에 성공한 뒤에만 여기서 만든다.
//!
//! 순서: `pi_approval::Approvals::answer` → (승인이면) `consume`(한 번만) → 쉬는 날 검사(E19) → 증표 →
//! `pi_rules::approved`(지금의 시세 · 장부로 다시 점검, 내용이 다르면 막힘) → 실행.
//! 소비가 규칙 확인보다 먼저다.

use pi_approval::{Answer, ApprovalNumber, Decision, RejectReason, Subject, fingerprint};
use pi_rules::{ApprovalGrant, Blocked, PendingApproval, Purpose, Reason};
use pi_store::{BlockReason, StoreError, TradeId};

use super::base::{exec_text, lock, millis};
use super::{
    ANSWER_CONTENT_CHANGED, ANSWER_FINGERPRINT_MISMATCH, ANSWER_NOT_FOUND, ANSWER_REST_DAY,
    OrderRequest, Trading,
};
use crate::TradingError;

/// 승인을 기다리는 주문 안 하나.
pub(super) struct Waiting {
    pub(super) trade: TradeId,
    /// 점검에 넣은 주문 안(매매 상태는 점검에 넘긴 것).
    pub(super) request: OrderRequest,
    pub(super) pending: PendingApproval,
    /// 승인할 내용. 소비할 때 이것의 지문을 승인한 지문과 비교한다.
    pub(super) subject: Subject,
}

/// 답하기의 결과: 실행했는가와 아니면 그 이유.
pub(crate) struct Answered {
    pub(crate) approved: bool,
    pub(crate) reason: Option<&'static str>,
}

pub(super) fn rejected(reason: &'static str) -> Answered {
    Answered {
        approved: false,
        reason: Some(reason),
    }
}

pub(crate) fn reject_word(reason: RejectReason) -> &'static str {
    match reason {
        RejectReason::UserRejected => "user_rejected",
        RejectReason::NotFound => ANSWER_NOT_FOUND,
        RejectReason::AlreadyResolved => "already_resolved",
        RejectReason::Expired => "expired",
        RejectReason::FingerprintMismatch => ANSWER_FINGERPRINT_MISMATCH,
        RejectReason::NotApproved => "not_approved",
        RejectReason::AlreadyConsumed => "already_consumed",
        RejectReason::FullStop => "full_stop",
    }
}

/// 다시 점검에서 막힌 까닭: 승인이 덮지 못한 내용 바뀜(E7)과 그 밖.
fn blocked_word(blocked: &Blocked) -> &'static str {
    if blocked.reason == Reason::ApprovalMismatch {
        ANSWER_CONTENT_CHANGED
    } else {
        "blocked"
    }
}

impl Trading {
    /// 승인 답 하나를 처리한다([`super::APPROVAL_ANSWER_PATH`]).
    pub(crate) async fn answer(
        &self,
        number: &str,
        shown: &str,
        answer: &str,
    ) -> Result<Answered, TradingError> {
        let number = ApprovalNumber::new(number);
        let decision = Decision::from_wire(answer);
        // 실전 승격을 묻는 번호면 주문이 아니라 템플릿을 올린다(10차 10a 7번).
        if let Some(template) = self.promotion_of(&number)? {
            return self.answer_promotion(&number, template, shown, decision);
        }
        let answered = self
            .approvals
            .answer(&number, shown, decision)
            .map_err(exec_text)?;
        match answered {
            Answer::Approved => self.run_approved(&number).await,
            Answer::Rejected(reason @ (RejectReason::NotFound | RejectReason::AlreadyResolved)) => {
                Ok(rejected(reject_word(reason)))
            }
            Answer::Rejected(reason) => {
                let waiting = lock(&self.waiting).remove(number.as_str());
                if let Some(waiting) = waiting {
                    self.block_rejected(waiting.trade, reject_word(reason))?;
                }
                Ok(rejected(reject_word(reason)))
            }
        }
    }

    /// 승인됐다: 소비 → 증표 → 다시 점검 → 실행.
    async fn run_approved(&self, number: &ApprovalNumber) -> Result<Answered, TradingError> {
        // 다시 점검할 주문 안이 없으면(다시 켠 뒤 등) 승인한 내용과 같은지 볼 수 없어 내지 않는다.
        let Some(waiting) = lock(&self.waiting).remove(number.as_str()) else {
            return Ok(rejected(ANSWER_CONTENT_CHANGED));
        };
        if let Err(error) = self.approvals.consume(number, &waiting.subject) {
            self.block_rejected(waiting.trade, &error.to_string())?;
            return Ok(rejected(match error {
                pi_approval::Error::Denied(reason) => reject_word(reason),
                _ => "consume_failed",
            }));
        }
        // 주문 안을 낼 때와 같이 쉬는 날에는 사는 주문 · 익절을 내지 않는다(E19).
        if let Some(reason) = self.rest_reason(self.clock.now())
            && !matches!(
                waiting.request.draft.purpose,
                Purpose::StopLoss | Purpose::Liquidation
            )
        {
            self.block_rejected(waiting.trade, &format!("쉬는 날: {reason}"))?;
            return Ok(rejected(ANSWER_REST_DAY));
        }
        let grant = ApprovalGrant::new(number.as_str(), fingerprint(&waiting.subject).as_str());
        let Some(params) = self.params()? else {
            self.block_rejected(waiting.trade, "고른 템플릿이 없습니다")?;
            return Ok(rejected("blocked"));
        };
        let now = self.clock.now();
        let request = &waiting.request;
        let inputs = match self.inputs(request, now).await? {
            Ok(inputs) => inputs,
            Err(blocked) => {
                self.block_rejected(waiting.trade, &blocked.detail())?;
                return Ok(rejected(blocked_word(&blocked)));
            }
        };
        let context = inputs.context(self, &params, request);
        match pi_rules::approved(waiting.pending, &grant, &request.draft, &context) {
            Err(blocked) => {
                self.block_rejected(waiting.trade, &blocked.detail())?;
                Ok(rejected(blocked_word(&blocked)))
            }
            Ok(passed) => {
                let purpose = request.draft.purpose;
                match self.execute(waiting.trade, purpose, passed, &params).await {
                    Ok(_) => Ok(Answered {
                        approved: true,
                        reason: None,
                    }),
                    Err(TradingError::Broker(_)) => Ok(rejected("broker_failed")),
                    Err(error) => Err(error),
                }
            }
        }
    }

    /// 승인대기인 매매를 막힘(승인 거절)으로 옮긴다. 이미 승인대기가 아니면 그대로 둔다.
    pub(super) fn block_rejected(&self, trade: TradeId, detail: &str) -> Result<(), TradingError> {
        let reason = BlockReason {
            kind: pi_store::BlockKind::ApprovalRejected,
            detail: format!("승인 거절: {detail}"),
        };
        match self
            .store
            .block_trade(trade, reason, millis(self.clock.now()))
        {
            Ok(_) | Err(StoreError::TransitionDenied { .. }) => Ok(()),
            Err(error) => Err(error.into()),
        }
    }
}
