//! 실행기의 안쪽 도우미: 잠금 · 스냅샷 저장 · 알림 옮기기 · 파는 주문 내기와 물러서기.

use std::sync::{MutexGuard, PoisonError};
use std::time::Duration;

use chrono::{DateTime, Utc};
use pi_market::Symbol;
use pi_paper::{
    AccountKind, ApprovedOrder, Broker, BrokerError, CheckPermit, OrderEvent, OrderGroup, OrderId,
    OrderRequest, Side, UnfilledTimeout,
};
use pi_store::{AccountKind as StoreAccount, TradeId};
use tokio::sync::mpsc::error::TryRecvError;

use super::{Executor, SELL_RETRY_FIRST, SELL_RETRY_MAX, SellPurpose, SellRetry, TradeChange};
use crate::error::Error;
use crate::track::{State, apply, millis};

/// 파는 주문 한 번 내기.
pub(super) struct SellAttempt {
    /// 매매 번호.
    pub(super) trade_id: TradeId,
    /// 목적.
    pub(super) purpose: SellPurpose,
    /// 주문 내용(처음 낸 것).
    pub(super) order: ApprovedOrder,
    /// 미체결 취소 시간.
    pub(super) timeout: UnfilledTimeout,
    /// 지금까지 실패한 횟수(처음 내면 0).
    pub(super) failed_before: u32,
}

impl Executor {
    /// 안쪽 상태를 잠근다. 잠근 채로 죽은 호출이 있었어도 상태는 그대로 쓴다.
    pub(super) fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// 계좌 스냅샷이 마지막으로 저장한 것과 다르면 저장한다(V6).
    pub(super) fn persist(&self) -> Result<(), Error> {
        let snapshot = self.broker.snapshot();
        let mut state = self.lock();
        if state.saved.as_ref() == Some(&snapshot) {
            return Ok(());
        }
        let value = serde_json::to_value(&snapshot).map_err(|e| Error::Snapshot(e.to_string()))?;
        self.store
            .save_paper_snapshot(&value, millis(self.clock.now()))?;
        state.saved = Some(snapshot);
        Ok(())
    }

    /// 구독으로 온 알림(다른 호출이 먼저 알아챈 것 포함)을 매매 전이로 옮기고 스냅샷을 저장한다.
    ///
    /// 구독이 끊겼으면(알림이 넘쳐 잃었으면) 다시 구독하고 이 호출이 받은 알림을 옮긴 뒤, 잃은
    /// 것이 있을 수 있으니 다시 맞춰 보기 전까지 새 주문을 받지 않는다(E16).
    pub(super) fn absorb(
        &self,
        returned: Vec<OrderEvent>,
        now: DateTime<Utc>,
    ) -> Result<Vec<TradeChange>, Error> {
        let at = millis(now);
        let mut changes = Vec::new();
        {
            let mut state = self.lock();
            let mut events = Vec::new();
            loop {
                match state.events.try_recv() {
                    Ok(event) => events.push(event),
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        state.events = self.broker.subscribe();
                        state.ready = false;
                        events.extend(returned);
                        break;
                    }
                }
            }
            for event in &events {
                apply(&self.store, &mut state, &event.order, at, &mut changes)?;
            }
        }
        self.persist()?;
        Ok(changes)
    }

    /// 계좌 종류가 증권사의 것과 같은가(E25).
    pub(super) fn check_account(&self, found: AccountKind) -> Result<(), Error> {
        let broker = self.broker.account_kind();
        if found == broker {
            Ok(())
        } else {
            Err(Error::AccountKindMismatch { broker, found })
        }
    }

    /// 파는 주문을 낸다. 손절 · 청산은 먼저 같은 종목의 열린 파는 주문을 취소한다.
    /// 받지 않음 · 응답 없음이면 다시 내기를 기다리는 것으로 남긴다(E30).
    pub(super) async fn send_sell(
        &self,
        attempt: SellAttempt,
        permit: CheckPermit,
    ) -> Result<OrderId, Error> {
        let now = self.clock.now();
        if matches!(
            attempt.purpose,
            SellPurpose::StopLoss | SellPurpose::Liquidation
        ) && let Err(e) = self.cancel_open_sells(&attempt.order.symbol, now).await
        {
            return self.sell_failed(attempt, e, now);
        }
        let key = self.lock().sell_key(attempt.trade_id, now);
        let request = order_request(&attempt.order, key, attempt.timeout, true, permit);
        match self.broker.place_order(request, now).await {
            Ok(ack) => {
                self.lock().pending.remove(&attempt.trade_id);
                self.persist()?;
                Ok(ack.order_id)
            }
            Err(e) => self.sell_failed(attempt, e, now),
        }
    }

    /// 파는 주문이 실패했다. 받지 않음 · 응답 없음은 횟수를 늘려 다시 내기를 기다리게 하고
    /// [`Error::AnomalyStop`], 그 밖은 [`Error::Rejected`](기다리는 것은 그대로).
    fn sell_failed(
        &self,
        attempt: SellAttempt,
        error: BrokerError,
        now: DateTime<Utc>,
    ) -> Result<OrderId, Error> {
        let result = classify(error);
        if matches!(result, Error::AnomalyStop(_)) {
            let attempts = attempt.failed_before.saturating_add(1);
            let wait =
                chrono::Duration::from_std(backoff(attempts)).unwrap_or(chrono::Duration::MAX);
            let retry = SellRetry {
                trade_id: attempt.trade_id,
                purpose: attempt.purpose,
                order: attempt.order,
                attempts,
                retry_at: now
                    .checked_add_signed(wait)
                    .unwrap_or(DateTime::<Utc>::MAX_UTC),
            };
            self.lock()
                .pending
                .insert(attempt.trade_id, (retry, attempt.timeout));
        }
        self.persist()?;
        Err(result)
    }

    /// 같은 종목의 열린 파는 주문을 모두 취소한다. 이미 끝난 주문은 넘어간다.
    async fn cancel_open_sells(
        &self,
        symbol: &Symbol,
        now: DateTime<Utc>,
    ) -> Result<(), BrokerError> {
        let broker: &dyn Broker = self.broker.as_ref();
        let open = broker.orders(OrderGroup::Open, Some(symbol), now).await?;
        for order in open.iter().filter(|o| o.side == Side::Sell) {
            match broker.cancel_order(&order.order_id, now).await {
                Ok(_)
                | Err(
                    BrokerError::AlreadyFilled
                    | BrokerError::AlreadyCanceled
                    | BrokerError::AlreadyModified
                    | BrokerError::AlreadyRejected
                    | BrokerError::OrderNotFound,
                ) => {}
                Err(other) => return Err(other),
            }
        }
        Ok(())
    }
}

/// `attempts`번 실패한 뒤 기다릴 간격: `min(SELL_RETRY_FIRST × 2^(attempts-1), SELL_RETRY_MAX)`.
pub(super) fn backoff(attempts: u32) -> Duration {
    let mut wait = SELL_RETRY_FIRST;
    for _ in 1..attempts {
        if wait >= SELL_RETRY_MAX {
            break;
        }
        wait = wait.saturating_mul(2);
    }
    wait.min(SELL_RETRY_MAX)
}

/// 증권사 오류를 나눈다: 받지 않음 · 응답 없음은 이상 감지 멈춤 요구(E13), 그 밖은 거부.
pub(super) fn classify(error: BrokerError) -> Error {
    match error {
        BrokerError::Unavailable | BrokerError::NoResponse => Error::AnomalyStop(error),
        other => Error::Rejected(other),
    }
}

/// 저장소의 계좌 종류를 증권사의 것으로.
pub(super) fn paper_kind(account: StoreAccount) -> AccountKind {
    match account {
        StoreAccount::Paper => AccountKind::Paper,
        StoreAccount::Live => AccountKind::Live,
    }
}

/// 점검 통과 표시와 함께 낼 주문 요청.
pub(super) fn order_request(
    order: &ApprovedOrder,
    key: String,
    timeout: UnfilledTimeout,
    confirm_high_value: bool,
    permit: CheckPermit,
) -> OrderRequest {
    OrderRequest {
        client_order_id: Some(key),
        market: order.market,
        symbol: order.symbol.clone(),
        side: order.side,
        order_type: order.order_type,
        quantity: order.quantity,
        price: order.price,
        account_kind: order.account_kind,
        timeout,
        confirm_high_value,
        permit: Some(permit),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_doubles_and_stops_at_the_longest() {
        assert_eq!(backoff(1), SELL_RETRY_FIRST);
        assert_eq!(backoff(2), SELL_RETRY_FIRST * 2);
        assert_eq!(backoff(4), SELL_RETRY_FIRST * 8);
        assert_eq!(backoff(7), SELL_RETRY_MAX);
        assert_eq!(backoff(u32::MAX), SELL_RETRY_MAX);
    }

    #[test]
    fn only_refusal_and_silence_are_anomaly_stops() {
        assert!(matches!(
            classify(BrokerError::Unavailable),
            Error::AnomalyStop(BrokerError::Unavailable)
        ));
        assert!(matches!(
            classify(BrokerError::NoResponse),
            Error::AnomalyStop(BrokerError::NoResponse)
        ));
        assert!(matches!(
            classify(BrokerError::CheckInvalid),
            Error::Rejected(BrokerError::CheckInvalid)
        ));
    }
}
