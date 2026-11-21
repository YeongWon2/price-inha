//! (10차 10a 5번) 답 없음(FN2 3b · E22): 기다릴 시간이 지난 승인 요청을 닫고, 할 일을 하고, 한 번만 남긴다.
//!
//! 아무도 부르지 않아도 뒤에서 돈다([`Trading::tick`]). 순서는 `pi_approval::Approvals::expire_due` →
//! `unhandled_due`(다시 켠 뒤에도 남은 할 일을 준다) → 할 일 → `record_due_handling`(한 번만)이다.
//!
//! - 사는 주문이고 알림이 갔다: `pi_rules::reduce_within_limit`로 금액 한도 안으로 줄여 다시 점검하고,
//!   `pi_exec::Executor::submit_reduced`로 낸다(승인이 없다. 실행기가 승인 기록과 스스로 대조한다, exec 6차).
//!   실행기를 거친 뒤에만 줄여 냈다고 남긴다. 막히거나 실행기가 거부하면 막힌 까닭을 남긴다(가린 글, 길이 상한).
//! - 재시작 틈(exec 6차 조정자 결정 5번): 주문은 나갔는데 처리 기록 전에 꺼졌으면, 다시 켠 뒤 그 매매는 이미
//!   주문됨 · 보유중이다. 막힘으로 닫지 않고 그 매매의 수량으로 줄여 냈다고만 남긴다.
//! - 사는 주문인데 알림이 가지 않았다: 사지 않는다(P20).
//! - 신호 유효 시간이 지났다: 버렸다고만 남긴다(늦게 사거나 팔지 않는다).
//! - 파는 주문: 규칙 엔진은 사는 주문만 승인으로 보내므로 지금은 이 갈래로 오는 길이 없다. 오면 들고 있기로 남긴다.

use pi_approval::{ApprovalNumber, DueAction, DueHandling, DueOutcome, Subject};
use pi_exec::ReducedSubmission;
use pi_paper::AccountKind;
use pi_store::{TradeId, TradeState};
use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;

use super::Trading;
use super::answer::Waiting;
use super::base::{exec_text, lock, millis};
use super::order::timeout_of;
use crate::TradingError;

/// 처리 기록에 남기는 막힌 까닭 글의 길이 상한(글자 수).
const REASON_MAX_CHARS: usize = 200;

/// 줄인 주문이 이미 나간 매매인가(주문됨을 지나왔다). 재시작 틈에서 막힘으로 닫지 않을 것을 가린다.
fn already_out(state: TradeState) -> bool {
    matches!(
        state,
        TradeState::Ordered | TradeState::Holding | TradeState::Sold | TradeState::Reviewed
    )
}

/// 까닭 글을 상한까지만 남긴다.
fn capped(text: &str) -> String {
    text.chars().take(REASON_MAX_CHARS).collect()
}

/// 승인할 주문 내용의 신호 id(`trade-<매매 번호>`)에서 매매 번호를 읽는다.
fn trade_of(subject: &Subject) -> Option<TradeId> {
    let Subject::Order(order) = subject else {
        return None;
    };
    let id = order.signal_id.strip_prefix("trade-")?.parse().ok()?;
    Some(TradeId(id))
}

impl Trading {
    /// 기한이 된 승인 요청을 닫고, 처리 기록이 없는 답 없음 · 만료마다 할 일을 한 뒤 한 번만 남긴다.
    pub(super) async fn settle_due(&self) -> Result<(), TradingError> {
        let _turn = self.due_turn.lock().await;
        let now = millis(self.clock.now());
        self.approvals.expire_due(now).map_err(exec_text)?;
        for due in self.approvals.unhandled_due().map_err(exec_text)? {
            let handling = self.handle_due(&due).await;
            match self.approvals.record_due_handling(&due.number, handling) {
                Ok(()) | Err(pi_approval::Error::AlreadyHandled) => {}
                Err(error) => {
                    tracing::warn!(%error, "답 없음 처리 기록을 남기지 못했습니다");
                }
            }
        }
        Ok(())
    }

    /// 답 없는 요청 하나의 할 일을 하고, 남길 처리 기록을 돌려준다.
    async fn handle_due(&self, due: &DueOutcome) -> DueHandling {
        let waiting = lock(&self.waiting).remove(due.number.as_str());
        let trade = waiting
            .as_ref()
            .map(|waiting| waiting.trade)
            .or_else(|| trade_of(&due.subject));
        if due.action == DueAction::ReduceWithinLimit
            && let Some(done) = trade.and_then(|trade| self.sent_before(trade, &due.subject))
        {
            return done;
        }
        match (due.action, waiting) {
            (DueAction::Dropped, _) => {
                self.close_unanswered(trade, "신호 유효 시간이 지나 내지 않음");
                DueHandling::Dropped
            }
            (DueAction::DoNotBuy, _) => {
                self.close_unanswered(trade, "승인 요청 알림이 가지 않아 사지 않음");
                DueHandling::NotBought
            }
            (DueAction::ReduceWithinLimit, Some(waiting)) => {
                match self.reduce(waiting, &due.number).await {
                    Ok(quantity) => DueHandling::Reduced { quantity },
                    Err(reason) => {
                        // 까닭 글은 저장소에 남는다: 가리고(E38) 길이를 막는다.
                        let reason = capped(&self.desk.redact(&reason));
                        self.close_unanswered(trade, &reason);
                        DueHandling::Blocked { reason }
                    }
                }
            }
            (DueAction::ReduceWithinLimit, None) => {
                // 다시 켠 뒤라 다시 점검할 주문 안이 없다: 승인한 적 없는 내용을 지어내 내지 않는다.
                let reason = "다시 점검할 주문 안이 없어 내지 않음".to_string();
                self.close_unanswered(trade, &reason);
                DueHandling::Blocked { reason }
            }
            (DueAction::TeamDecides, _) => {
                tracing::info!(
                    number = due.number.as_str(),
                    "답 없는 파는 신호: 들고 있기로 남긴다"
                );
                DueHandling::TeamKept
            }
        }
    }

    /// 재시작 틈: 그 매매가 이미 주문됨을 지나왔으면(줄인 주문이 나간 뒤 처리 기록 전에 꺼졌다) 그 매매의
    /// 수량으로 줄여 냈다고 남길 기록. 아직 승인대기 · 막힘이거나 수량을 알 수 없으면 없음(보통 길로 간다).
    fn sent_before(&self, trade: TradeId, subject: &Subject) -> Option<DueHandling> {
        let found = self.store.get_trade(trade).ok()?;
        if !already_out(found.state) {
            return None;
        }
        let Subject::Order(asked) = subject else {
            return None;
        };
        let quantity = self.store.trade_quantity(trade).ok().flatten()?;
        (quantity > 0 && quantity <= asked.quantity).then_some(DueHandling::Reduced { quantity })
    }

    /// 승인대기인 매매를 막힘으로 닫는다(답이 없었다는 까닭과 함께). 실패는 기록만 한다.
    fn close_unanswered(&self, trade: Option<TradeId>, detail: &str) {
        let Some(trade) = trade else {
            return;
        };
        if let Err(error) = self.block_rejected(trade, &format!("답 없음 · {detail}")) {
            tracing::warn!(%error, trade = trade.0, "답 없는 매매를 닫지 못했습니다");
        }
    }

    /// 답 없는 사기를 지금의 금액 한도 안으로 줄여 다시 점검하고 실행기의 줄여 내기 길로 낸다.
    /// 낸 수량, 아니면 막힌 까닭.
    async fn reduce(&self, waiting: Waiting, number: &ApprovalNumber) -> Result<u64, String> {
        let text = |error: TradingError| error.to_string();
        let Waiting {
            trade,
            request,
            pending,
            ..
        } = waiting;
        let now = self.clock.now();
        if let Some(reason) = self.rest_reason(now) {
            return Err(format!("쉬는 날: {reason}"));
        }
        let params = self
            .params()
            .map_err(text)?
            .ok_or_else(|| "고른 템플릿이 없습니다".to_string())?;
        let inputs = self
            .inputs(&request, now)
            .await
            .map_err(text)?
            .map_err(|blocked| blocked.detail())?;
        let context = inputs.context(self, &params, &request);
        let passed = pi_rules::reduce_within_limit(pending, &request.draft, &context)
            .map_err(|blocked| blocked.detail())?;
        let quantity = passed
            .order()
            .quantity
            .trunc()
            .to_u64()
            .ok_or_else(|| "주문 수량을 셀 수 없습니다".to_string())?;
        // 줄인 수량을 매매에 남긴다(재시작 틈에서 이 수량으로 기록한다. 실행기는 표시 수량이 이것 이하인지 본다).
        self.keep_quantity(trade, Decimal::from(quantity))
            .map_err(text)?;
        let template_account = if self.live_in_use().map_err(text)? {
            AccountKind::Live
        } else {
            AccountKind::Paper
        };
        let submission = ReducedSubmission {
            trade_id: trade,
            permit: pi_rules::issue_for_trade(&self.authority, passed, trade.0),
            template_account,
            unanswered: number.clone(),
            timeout: timeout_of(params.unfilled_cancel),
        };
        match self.executor.submit_reduced(submission).await {
            Ok(_) => Ok(quantity),
            Err(pi_exec::Error::AnomalyStop(error)) => {
                // 증권사가 받지 않거나 응답하지 않는다: 이상 감지 멈춤으로 옮긴다(E13).
                self.anomaly().map_err(text)?;
                Err(format!("증권사가 주문을 받지 않음: {error}"))
            }
            Err(error) => Err(format!("주문 실행기가 받지 않음: {error}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use pi_approval::{OrderSubject, OrderType, Side, SignalKind};
    use pi_store::{AccountKind, UnixMillis};

    use super::*;

    #[test]
    fn 이미_나간_매매는_막힘으로_닫지_않는다() {
        // 재시작 틈: 주문됨 · 보유중 · 팔림 · 복기됨은 줄인 주문이 이미 나간 것이다.
        for state in [
            TradeState::Ordered,
            TradeState::Holding,
            TradeState::Sold,
            TradeState::Reviewed,
        ] {
            assert!(already_out(state), "{state:?}");
        }
        for state in [
            TradeState::AwaitingApproval,
            TradeState::Blocked,
            TradeState::Judged,
            TradeState::Discussing,
        ] {
            assert!(!already_out(state), "{state:?}");
        }
    }

    #[test]
    fn 까닭_글은_상한까지만_남긴다() {
        assert_eq!(capped(&"가".repeat(500)).chars().count(), REASON_MAX_CHARS);
        assert_eq!(capped("짧은 글"), "짧은 글");
    }

    #[test]
    fn 신호_id에서_매매_번호를_읽는다() {
        let subject = Subject::Order(OrderSubject {
            account: AccountKind::Paper,
            symbol: "000660".to_string(),
            side: Side::Buy,
            quantity: 5,
            price: rust_decimal::Decimal::from(100),
            order_type: OrderType::Limit,
            signal_id: "trade-42".to_string(),
            signal_kind: SignalKind::Buy,
            signal_valid_until: UnixMillis(0),
        });
        assert_eq!(trade_of(&subject), Some(TradeId(42)));
    }
}
