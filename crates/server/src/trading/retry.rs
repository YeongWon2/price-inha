//! (10차 10b 0번 가) 실패한 파는 주문 다시 내기(E30): 하루 tick마다 실행기가 다시 내기를 기다리는 파는 주문
//! (`Executor::pending_sells`)을 보고, 때가 된 것을 그 자리의 규칙 점검을 거쳐 `Executor::retry_sell`로 다시 낸다.
//!
//! - 다시 낼 주문 안은 실행기의 손절 주문 안(`stop_draft`: 그 매매의 근거 · 자료)에 처음 낸 주문의 목적 · 호가 유형 ·
//!   값 · 수량을 얹어 만든다. 점검 통과 표시는 규칙 엔진의 길(`pi_rules::issue_for_trade`)로만 받는다.
//! - 또 실패해 실행기가 이상 감지 멈춤을 요구하면 팀을 이상 감지 멈춤으로 옮긴다(첫 실패와 같은 길).
//! - (10b 보강 5번) 막힌 다시 내기(점검에서 막힘 · 점검 입력이 모자람 · 승인을 요구함 · 보유 자리나 시세가 없음 ·
//!   표시 내용이 처음 낸 것과 다름 `PermitMismatch`)는 **그 대기만** 물러선다: 다음에 볼 때까지의 간격을 1초에서
//!   막힐 때마다 두 배로(최대 10분) 늘린다. 같은 까닭의 기록은 처음 한 번만 남긴다. 손절 대기가 막힌 때부터
//!   10분 넘게 이어지면 사용자에게 한 번 알린다("손절을 다시 내지 못하고 있습니다" — 종목 · 수량만, 까닭 글은
//!   싣지 않는다). 다시 냈거나 대기가 사라지면 물러서기를 지운다(기억은 기다리는 대기만큼만 둔다).

use chrono::{DateTime, Duration, Utc};
use pi_exec::{SellPurpose, SellRetry};
use pi_rules::{Outcome, Purpose, SymbolStatus};

use super::base::lock;
use super::{OrderRequest, Trading};
use crate::TradingError;

/// 막힌 다시 내기의 첫 물러서기 간격(초).
const BACKOFF_FIRST_SECONDS: i64 = 1;
/// 막힌 다시 내기의 물러서기 간격 상한(초, 10분).
const BACKOFF_MAX_SECONDS: i64 = 600;
/// 손절 대기가 이만큼(초) 넘게 막혀 있으면 알린다(10분).
const STUCK_STOP_SECONDS: i64 = 600;

/// 막힌 다시 내기 하나의 물러서기.
pub(super) struct HeldBack {
    /// 다음에 다시 볼 시각.
    next_at: DateTime<Utc>,
    /// 지금 간격(초).
    interval: i64,
    /// 마지막으로 남긴 까닭(같은 까닭은 다시 남기지 않는다).
    reason: String,
    /// 처음 막힌 시각.
    since: DateTime<Utc>,
    /// 오래 막혔다고 알렸나.
    told: bool,
}

/// 다시 내 본 결과.
enum Tried {
    /// 다시 냈다(또는 아직 때가 아니라 실행기가 받지 않았다).
    Done,
    /// 막혔다(까닭).
    Blocked(String),
}

/// 다음 물러서기 간격(초): 처음은 1초, 그 뒤로 두 배씩, 최대 10분.
fn next_interval(current: i64) -> i64 {
    if current <= 0 {
        BACKOFF_FIRST_SECONDS
    } else {
        current.saturating_mul(2).min(BACKOFF_MAX_SECONDS)
    }
}

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
    /// 막힌 대기는 물러선 간격이 지나야 다시 본다.
    pub(super) async fn retry_sells(&self) {
        let now = self.clock.now();
        let pending = self.executor.pending_sells();
        lock(&self.held_back).retain(|trade, _| pending.iter().any(|p| p.trade_id == *trade));
        for pending in pending {
            if pending.retry_at > now {
                continue;
            }
            let trade = pending.trade_id;
            let waiting = lock(&self.held_back)
                .get(&trade)
                .is_some_and(|held| held.next_at > now);
            if waiting {
                continue;
            }
            match self.retry_one(&pending, now).await {
                Ok(Tried::Done) => {
                    lock(&self.held_back).remove(&trade);
                }
                Ok(Tried::Blocked(reason)) => self.hold_back(&pending, reason, now),
                Err(error) => {
                    tracing::warn!(%error, trade = trade.0, "실패한 파는 주문을 다시 내지 못했습니다");
                }
            }
        }
    }

    /// 막힌 대기를 물러서게 한다: 간격을 두 배로(최대 10분), 새 까닭이면 한 번 남기고, 손절이 오래 막혔으면 알린다.
    fn hold_back(&self, pending: &SellRetry, reason: String, now: DateTime<Utc>) {
        let trade = pending.trade_id;
        let (new_reason, tell) = {
            let mut held = lock(&self.held_back);
            let entry = held.entry(trade).or_insert_with(|| HeldBack {
                next_at: now,
                interval: 0,
                reason: String::new(),
                since: now,
                told: false,
            });
            entry.interval = next_interval(entry.interval);
            entry.next_at = now + Duration::seconds(entry.interval);
            let new_reason = entry.reason != reason;
            if new_reason {
                entry.reason = reason.clone();
            }
            let stuck = pending.purpose == SellPurpose::StopLoss
                && now - entry.since > Duration::seconds(STUCK_STOP_SECONDS);
            let tell = stuck && !entry.told;
            entry.told |= tell;
            (new_reason, tell)
        };
        if new_reason {
            tracing::warn!(trade = trade.0, detail = %reason, "다시 낼 파는 주문이 막혔습니다(물러서며 다시 봅니다)");
        }
        if tell {
            let order = &pending.order;
            tracing::warn!(trade = trade.0, "손절을 다시 내지 못하고 있습니다");
            self.announce_stuck_stop(
                order.symbol.as_str(),
                &order.quantity.normalize().to_string(),
            );
        }
    }

    /// 그 자리의 시세 · 장부 · 팀 상태로 다시 점검하고, 지나면 다시 낸다.
    async fn retry_one(
        &self,
        pending: &SellRetry,
        now: DateTime<Utc>,
    ) -> Result<Tried, TradingError> {
        let purpose = purpose_of(pending.purpose);
        let Some(request) = self.retry_request(pending, purpose) else {
            return Ok(Tried::Blocked("보유 자리나 시세가 없습니다".to_string()));
        };
        let params = self.params_for(purpose)?.ok_or(TradingError::NoTemplate)?;
        let inputs = match self.inputs(&request, now).await? {
            Ok(inputs) => inputs,
            Err(blocked) => return Ok(Tried::Blocked(blocked.detail())),
        };
        let passed = match pi_rules::check(&request.draft, &inputs.context(self, &params, &request))
        {
            Outcome::Pass(passed) => passed,
            Outcome::Blocked(blocked) => return Ok(Tried::Blocked(blocked.detail())),
            Outcome::NeedsApproval(_) => {
                return Ok(Tried::Blocked(
                    "승인을 요구합니다(다시 내지 않습니다)".to_string(),
                ));
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
                Ok(Tried::Done)
            }
            Err(pi_exec::Error::RetryNotDue { .. }) => Ok(Tried::Done),
            Err(pi_exec::Error::PermitMismatch) => Ok(Tried::Blocked(
                "표시 내용이 처음 낸 주문과 다릅니다(PermitMismatch)".to_string(),
            )),
            Err(pi_exec::Error::AnomalyStop(error)) => {
                self.anomaly()?;
                Err(TradingError::Broker(error.to_string()))
            }
            Err(error) => Err(super::base::exec_text(error)),
        }
    }

    /// 다시 낼 주문 안: 그 매매의 손절 주문 안(근거 · 자료)에 처음 낸 주문의 목적 · 호가 · 값 · 수량을 얹는다.
    /// 보유 자리나 시세가 없으면 없음(물러서며 다시 본다).
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 간격은_두_배씩_늘고_10분을_넘지_않는다() {
        let mut interval = 0;
        let mut seen = Vec::new();
        for _ in 0..12 {
            interval = next_interval(interval);
            seen.push(interval);
        }
        assert_eq!(&seen[..4], &[1, 2, 4, 8]);
        assert_eq!(seen.last().copied(), Some(BACKOFF_MAX_SECONDS));
    }
}
