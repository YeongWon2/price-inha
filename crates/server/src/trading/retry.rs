//! (10차 10b 0번 가) 실패한 파는 주문 다시 내기(E30): 하루 tick마다 실행기가 다시 내기를 기다리는 파는 주문
//! (`Executor::pending_sells`)을 보고, 때가 된 것을 그 자리의 규칙 점검을 거쳐 `Executor::retry_sell`로 다시 낸다.
//!
//! - 다시 낼 주문 안은 실행기의 손절 주문 안(`stop_draft`: 그 매매의 근거 · 자료)에 처음 낸 주문의 목적 · 호가 유형 ·
//!   값 · 수량을 얹어 만든다. 점검 통과 표시는 규칙 엔진의 길(`pi_rules::issue_for_trade`)로만 받는다.
//! - 표시 내용이 처음 낸 것과 다르면 실행기가 거절한다(`PermitMismatch`): 기록만 하고 다음 tick에 다시 본다.
//! - 또 실패해 실행기가 이상 감지 멈춤을 요구하면 팀을 이상 감지 멈춤으로 옮긴다(첫 실패와 같은 길).

use chrono::{DateTime, Utc};
use pi_exec::{SellPurpose, SellRetry};
use pi_rules::{Outcome, Purpose, SymbolStatus};

use super::{OrderRequest, Trading};
use crate::TradingError;

/// 실행기의 파는 목적 → 규칙 엔진의 목적.
fn purpose_of(purpose: SellPurpose) -> Purpose {
    match purpose {
        SellPurpose::StopLoss => Purpose::StopLoss,
        SellPurpose::TakeProfit => Purpose::TakeProfit,
        SellPurpose::TeamExit => Purpose::TeamExit,
        _ => Purpose::Liquidation,
    }
}

impl Trading {
    /// 다시 내기를 기다리는 파는 주문 가운데 때가 된 것을 다시 낸다. 실패는 기록만 하고 다음 tick에 다시 본다.
    pub(super) async fn retry_sells(&self) {
        let now = self.clock.now();
        for pending in self.executor.pending_sells() {
            if pending.retry_at > now {
                continue;
            }
            let trade = pending.trade_id.0;
            if let Err(error) = self.retry_one(&pending, now).await {
                tracing::warn!(%error, trade, "실패한 파는 주문을 다시 내지 못했습니다");
            }
        }
    }

    /// 그 자리의 시세 · 장부 · 팀 상태로 다시 점검하고, 지나면 다시 낸다.
    async fn retry_one(&self, pending: &SellRetry, now: DateTime<Utc>) -> Result<(), TradingError> {
        let purpose = purpose_of(pending.purpose);
        let Some(request) = self.retry_request(pending, purpose) else {
            return Ok(());
        };
        let params = self.params_for(purpose)?.ok_or(TradingError::NoTemplate)?;
        let inputs = match self.inputs(&request, now).await? {
            Ok(inputs) => inputs,
            Err(blocked) => {
                tracing::warn!(detail = %blocked.detail(), "다시 낼 파는 주문의 점검 입력이 모자랍니다");
                return Ok(());
            }
        };
        let passed = match pi_rules::check(&request.draft, &inputs.context(self, &params, &request))
        {
            Outcome::Pass(passed) => passed,
            Outcome::Blocked(blocked) => {
                tracing::warn!(detail = %blocked.detail(), "다시 낼 파는 주문이 점검에서 막혔습니다");
                return Ok(());
            }
            Outcome::NeedsApproval(_) => {
                tracing::warn!("다시 낼 파는 주문이 승인을 요구합니다(다시 내지 않습니다)");
                return Ok(());
            }
        };
        let trade = pending.trade_id;
        let permit = pi_rules::issue_for_trade(&self.authority, passed, trade.0);
        match self.executor.retry_sell(trade, permit).await {
            Ok(id) => {
                tracing::info!(trade = trade.0, "실패한 파는 주문을 다시 냈습니다");
                if purpose == Purpose::StopLoss {
                    self.note_stop(trade, &super::Placed::Ordered(id));
                }
                Ok(())
            }
            Err(pi_exec::Error::RetryNotDue { .. }) => Ok(()),
            Err(pi_exec::Error::AnomalyStop(error)) => {
                self.anomaly()?;
                Err(TradingError::Broker(error.to_string()))
            }
            Err(error) => Err(super::base::exec_text(error)),
        }
    }

    /// 다시 낼 주문 안: 그 매매의 손절 주문 안(근거 · 자료)에 처음 낸 주문의 목적 · 호가 · 값 · 수량을 얹는다.
    /// 보유 자리나 시세가 없으면 없음(다음 tick에 다시 본다).
    fn retry_request(&self, pending: &SellRetry, purpose: Purpose) -> Option<OrderRequest> {
        let guard = self
            .executor
            .guards()
            .into_iter()
            .find(|guard| guard.trade_id == pending.trade_id)?;
        let quote = self.last_quote(guard.symbol.as_str())?;
        let mut draft = match self.executor.stop_draft(&guard, &quote) {
            Ok(draft) => draft,
            Err(error) => {
                tracing::warn!(%error, "다시 낼 파는 주문 안을 만들지 못했습니다");
                return None;
            }
        };
        let order = &pending.order;
        draft.purpose = purpose;
        draft.order_type = order.order_type;
        draft.limit_price = order.price;
        draft.ai_quantity = Some(order.quantity);
        Some(OrderRequest {
            trade_id: pending.trade_id,
            draft,
            reference_price: quote.price,
            // 파는 주문은 VI · 거래정지를 모른다고 막지 않는다.
            status: SymbolStatus {
                vi: None,
                halted: None,
            },
        })
    }
}
