//! 답 없는 사기를 줄여 내기(6차 · E22 · FN2 3b3).
//!
//! 알림이 갔는데 답이 없어 닫힌 사는 요청을, 규칙 엔진이 금액 한도 안으로 줄여 다시 점검한 표시로 낸다.
//! 승인이 없으므로 증표를 받지 않는다. 부르는 쪽의 말을 믿지 않고 저장소의 승인 기록과 대조한다.

use pi_approval::ApprovalNumber;
use pi_paper::{AccountKind, CheckPermit, OrderId, UnfilledTimeout};
use pi_store::{TradeId, TradeState};
use rust_decimal::Decimal;

use super::Executor;
use super::run::fits_buy;
use crate::error::Error;
use crate::grant;

/// 답 없음으로 닫힌 사는 요청을 줄여서 내는 주문 하나(E22 · FN2 3b3).
#[derive(Debug)]
pub struct ReducedSubmission {
    /// 매매 번호. 승인대기여야 한다(아니면 [`Error::NotAwaitingApproval`]).
    pub trade_id: TradeId,
    /// 줄인 주문의 점검 통과 표시(E33). 종목은 매매의 종목과 같고, 방향은 사기이고, 이 매매에 묶여
    /// 있어야 한다(아니면 [`Error::PermitMismatch`]). 수량은 0보다 크고 요청 수량 이하, 저장소의
    /// 매매 수량이 있으면 그것 이하여야 한다.
    pub permit: CheckPermit,
    /// 이 매매를 낸 템플릿의 계좌 종류(E25). 증권사의 계좌 종류와 같아야 한다.
    pub template_account: AccountKind,
    /// 답 없음으로 닫힌 승인 요청의 번호. 저장소의 승인 기록과 대조한다
    /// (아니면 [`Error::UnansweredMismatch`]).
    pub unanswered: ApprovalNumber,
    /// 미체결 취소 시간.
    pub timeout: UnfilledTimeout,
}

impl Executor {
    /// 답 없는 사는 요청을 줄여서 낸다(E22 · FN2 3b3). 돌려주는 값은 증권사 주문 식별자다.
    ///
    /// 순서대로 본다. 어긋나면 매매는 그대로이고 저장도 주문도 하지 않는다.
    ///
    /// 1. 준비됨(E16) → [`Error::NotReady`]. 계좌 종류(E25: 템플릿 · 매매 · 표시가 증권사와 같음)
    ///    → [`Error::AccountKindMismatch`].
    /// 2. 승인 표시(`CheckPermit::granted()`)면 → [`Error::GrantedPermit`](10차). 줄임 길은 `reduce_within_limit`의
    ///    표시만 받는다.
    /// 3. 표시(E33): 종목이 매매와 같고, 방향이 사기이고, 이 매매에 묶임 → [`Error::PermitMismatch`].
    /// 4. 매매 상태가 승인대기 → 아니면 [`Error::NotAwaitingApproval`].
    /// 5. 승인 기록 → 하나라도 아니면 [`Error::UnansweredMismatch`]:
    ///    - 그 번호의 기록이 있고, 결과가 답 없음이고, 소비되지 않았다.
    ///    - 처리 기록이 없는 닫힌 요청 목록에 있고 할 일이 "줄여서 낸다"다(사는 요청 · 알림이 감 ·
    ///      신호 유효 시간 안 · 처리 기록 없음).
    ///    - 대상이 주문이고, 신호 id가 `trade-<매매 번호>`이고, 종목 · 계좌가 매매와 같고, 방향이
    ///      사기이고, 호가 유형이 표시와 같다. 값은 대조하지 않는다.
    ///    - `0 < 표시 수량 ≤ 요청 수량`.
    ///    - `표시 수량 × 표시 지정가 ≤ 요청 수량 × 요청 값`(답 없는 주문이 사용자가 본 요청보다
    ///      커지지 않는다).
    /// 6. 저장소의 매매 수량이 있으면 표시 수량이 그것 이하 → 아니면 [`Error::PermitMismatch`].
    /// 7. 매매를 주문됨으로 **저장** → `place_order`. 큰 금액 확인 표시는 붙이지 않는다.
    ///
    /// 증권사가 받지 않으면 [`Executor::submit`]과 같다: 매매를 막힘(미체결 취소, 이유 남김)으로
    /// 옮기고 받지 않음 · 응답 없음은 [`Error::AnomalyStop`](E13), 그 밖은 [`Error::Rejected`].
    /// 승인 기록은 읽기만 한다(처리 기록은 부르는 쪽이 남긴다).
    pub async fn submit_reduced(&self, submission: ReducedSubmission) -> Result<OrderId, Error> {
        let now = self.clock.now();
        if !self.ready() {
            return Err(Error::NotReady);
        }
        let id = submission.trade_id;
        let trade = self.store.get_trade(id)?;
        // 승인 표시(승인 증표로 낸 것)로 요청 전량이 나가는 길을 막는다(결정 14).
        if submission.permit.granted() {
            return Err(Error::GrantedPermit);
        }
        let approved = submission.permit.approved().clone();
        self.check_buy_accounts(submission.template_account, &trade, &approved)?;
        if !fits_buy(&submission.permit, &trade) {
            return Err(Error::PermitMismatch);
        }
        if trade.state != TradeState::AwaitingApproval {
            return Err(Error::NotAwaitingApproval(trade.state));
        }
        grant::check_unanswered(&self.approvals, &submission.unanswered, &trade, &approved)?;
        // 저장소에 매매 수량이 있으면 줄인 수량이 그것 이하여야 한다(매매 수량은 고쳐 쓰지 않는다).
        if let Some(quantity) = self.store.trade_quantity(id)?
            && approved.quantity > Decimal::from(quantity)
        {
            return Err(Error::PermitMismatch);
        }
        // 승인이 없으므로 큰 금액 확인 표시는 붙이지 않는다.
        self.place_buy(id, submission.permit, submission.timeout, false, now)
            .await
    }
}
