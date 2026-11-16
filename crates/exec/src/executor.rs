//! 주문 실행기 (C10 · E12 · E13 · E16 · E25 · E29 · E30 · E33 · V6).

mod run;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use chrono::{DateTime, Utc};
use pi_market::{Clock, Market, Quote, Symbol};
use pi_paper::{
    AccountKind, ApprovedOrder, Broker, CheckAuthority, CheckPermit, OrderGroup, OrderId,
    PaperBroker, PaperConfig, Side, Snapshot, UnfilledTimeout,
};
use pi_rules::ApprovalGrant;
use pi_store::{Amount, Store, TradeId, TradeState};
use rust_decimal::Decimal;

use crate::error::Error;
use crate::track::{State, apply, block_unfilled, buy_key, millis, parse_key};
use run::{SellAttempt, classify, order_request, paper_kind};

/// 파는 주문이 실패한 뒤 처음 다시 낼 때까지의 간격(E30). 실패할 때마다 두 배가 된다.
pub const SELL_RETRY_FIRST: Duration = Duration::from_secs(1);

/// 파는 주문을 다시 내는 간격의 가장 긴 값(E30). 두 배로 늘린 간격이 이것을 넘으면 이것이다.
pub const SELL_RETRY_MAX: Duration = Duration::from_secs(60);

/// 사는 주문 하나(C10). 규칙 엔진이 낸 점검 통과 표시와 함께 온다.
#[derive(Debug)]
pub struct Submission {
    /// 매매 번호. 판단됨 또는 승인대기여야 한다.
    pub trade_id: TradeId,
    /// 점검 통과 표시(E33). 내용의 종목은 매매의 종목과 같고 방향은 사기여야 한다.
    pub permit: CheckPermit,
    /// 이 매매를 낸 템플릿의 계좌 종류(E25). 증권사의 계좌 종류와 같아야 한다.
    pub template_account: AccountKind,
    /// 승인 소비 결과. 승인대기인 매매는 있어야 한다. 있으면 큰 금액 확인을 한 것으로 낸다.
    ///
    /// 있으면 저장소의 승인 기록(`pi-approval`이 남긴 것)과 대조한다: 그 번호의 승인이 승인됨 ·
    /// 소비됨이고, 지문이 같고, 승인한 주문 내용이 이 매매(신호 id `trade-<매매 번호>` · 종목 ·
    /// 계좌)와 점검 통과 표시(방향 · 수량 · 값 · 호가 유형)와 같아야 한다. 아니면
    /// [`Error::ApprovalMismatch`].
    pub approval: Option<ApprovalGrant>,
    /// 미체결 취소 시간(템플릿의 보유 기간에서 정한다).
    pub timeout: UnfilledTimeout,
}

/// 파는 주문의 목적.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SellPurpose {
    /// 손절. 내기 전에 같은 종목의 열린 파는 주문을 먼저 취소한다.
    StopLoss,
    /// 익절.
    TakeProfit,
    /// 청산. 내기 전에 같은 종목의 열린 파는 주문을 먼저 취소한다.
    Liquidation,
}

/// 파는 주문 하나.
#[derive(Debug)]
pub struct SellSubmission {
    /// 매매 번호. 보유중이어야 한다.
    pub trade_id: TradeId,
    /// 점검 통과 표시. 내용의 종목은 매매의 종목과 같고 방향은 팔기여야 한다. 수량은 그 매매의
    /// 남은 수량 이하여야 한다(아니면 [`Error::SellTooLarge`]).
    pub permit: CheckPermit,
    /// 목적.
    pub purpose: SellPurpose,
    /// 미체결 취소 시간.
    pub timeout: UnfilledTimeout,
}

/// 실패해서 다시 내기를 기다리는 파는 주문(E30).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SellRetry {
    /// 매매 번호.
    pub trade_id: TradeId,
    /// 목적.
    pub purpose: SellPurpose,
    /// 처음 낸 주문 내용. 다시 낼 때의 표시 내용이 이것과 같아야 한다.
    pub order: ApprovedOrder,
    /// 지금까지 실패한 횟수(1부터).
    pub attempts: u32,
    /// 다시 낼 수 있는 가장 이른 시각: 마지막 실패 시각 + `min(SELL_RETRY_FIRST × 2^(attempts-1), SELL_RETRY_MAX)`.
    pub retry_at: DateTime<Utc>,
}

/// 보유중인 매매의 손절 감시 자리. 손절값은 체결된 수량에 건다(E29).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Guard {
    /// 매매 번호.
    pub trade_id: TradeId,
    /// 시장.
    pub market: Market,
    /// 종목.
    pub symbol: Symbol,
    /// 체결된(아직 팔지 않은) 수량.
    pub quantity: Decimal,
    /// 손절값(매매 근거의 손절값).
    pub stop: Amount,
}

/// 매매 상태가 바뀐 것 하나.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TradeChange {
    /// 매매 번호.
    pub trade_id: TradeId,
    /// 바뀐 뒤 상태.
    pub to: TradeState,
}

/// 다시 켠 뒤 맞춰 본 결과(E16). 저장소의 주문됨 매매가 어디로 갔는지 나눈다.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Reconciled {
    /// 증권사에서 (일부라도) 체결돼 보유중으로 옮긴 매매.
    pub holding: Vec<TradeId>,
    /// 증권사에 주문이 없거나 하나도 체결되지 않고 끝나 막힘(미체결 취소)으로 옮긴 매매.
    pub blocked: Vec<TradeId>,
    /// 증권사에 아직 열린 주문이 있어 주문됨으로 둔 매매.
    pub open: Vec<TradeId>,
}

/// 주문 실행기. 모의계좌(`pi-paper`)와 저장소(`pi-store`) 사이에 선다.
///
/// - 실행 전에 저장한다: 매매를 주문됨으로 저장한 뒤에만 `place_order`를 부른다.
///   증권사가 받지 않으면 그 매매는 막힘(미체결 취소, 이유 남김)으로 옮긴다.
/// - 계좌가 바뀔 때마다(주문 · 취소 · 체결 처리 뒤) 계좌 스냅샷을 저장소에 저장한다(V6).
/// - 체결 알림은 [`Executor::on_quote`] · [`Executor::on_time`]에서 처리한다. 다른 호출이 먼저
///   알아채 구독으로만 온 알림도 이때 함께 처리한다.
pub struct Executor {
    /// 모의계좌.
    broker: Arc<PaperBroker>,
    /// 저장소.
    store: Store,
    /// 시계.
    clock: Arc<dyn Clock>,
    /// 안쪽 상태(준비됨 · 보유 장부 · 다시 낼 파는 주문 · 구독 · 마지막 스냅샷).
    state: Mutex<State>,
}

impl std::fmt::Debug for Executor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Executor").finish_non_exhaustive()
    }
}

impl Executor {
    /// 실행기를 연다. 저장소에 계좌 스냅샷이 있으면 가장 나중 것으로 계좌를 되살리고(V6),
    /// 없으면 `config`의 처음 돈으로 새 계좌를 연다. 함께 나오는 권한만이 이 계좌가 받는 점검
    /// 통과 표시를 낼 수 있다.
    ///
    /// 연 직후에는 [`Executor::ready`]가 거짓이다. [`Executor::reconcile`]을 마쳐야 새 주문을 받는다.
    /// 스냅샷이 말이 안 되면 [`Error::Snapshot`]이고 새 계좌로 열지 않는다.
    /// 되살릴 때 증권사를 부르지 않는다(계좌 상태가 스냅샷 그대로다).
    pub fn open(
        config: PaperConfig,
        store: Store,
        clock: Arc<dyn Clock>,
    ) -> Result<(Executor, CheckAuthority), Error> {
        let (broker, authority, saved) = match store.latest_paper_snapshot()? {
            Some(saved) => {
                let snapshot: Snapshot = serde_json::from_value(saved.state)
                    .map_err(|e| Error::Snapshot(e.to_string()))?;
                let (broker, authority) = PaperBroker::restore(config, snapshot)
                    .map_err(|e| Error::Snapshot(e.to_string()))?;
                let restored = broker.snapshot();
                (broker, authority, Some(restored))
            }
            None => {
                let (broker, authority) = PaperBroker::open(config).map_err(Error::Rejected)?;
                (broker, authority, None)
            }
        };
        let events = broker.subscribe();
        let executor = Executor {
            broker: Arc::new(broker),
            store,
            clock,
            state: Mutex::new(State::new(events, saved)),
        };
        Ok((executor, authority))
    }

    /// 모의계좌. 조립하는 쪽은 달력 · 기준가를 여기에 넣는다.
    pub fn broker(&self) -> &PaperBroker {
        &self.broker
    }

    /// 새 주문을 받을 수 있는가(E16). 열고 나서 [`Executor::reconcile`]을 마치기 전에는 거짓이다.
    pub fn ready(&self) -> bool {
        self.lock().ready
    }

    /// 저장소의 주문됨 매매와 증권사 주문 목록을 맞춰 본다(E16). 끝나면 [`Executor::ready`]가 참이다.
    ///
    /// - 증권사 주문이 (일부라도) 체결됐으면 보유중으로 옮기고 체결된 수량으로 손절 감시를 건다.
    /// - 증권사 주문이 아직 열려 있으면 주문됨으로 두고 이어서 지켜본다.
    /// - 증권사에 주문이 없거나 하나도 체결되지 않고 끝났으면 막힘(미체결 취소)으로 옮기고 이유를 남긴다.
    /// - 보유중인 매매의 손절 감시도 되살린다.
    pub async fn reconcile(&self) -> Result<Reconciled, Error> {
        let now = self.clock.now();
        let at = millis(now);
        let broker: &dyn Broker = self.broker.as_ref();
        let mut orders = broker
            .orders(OrderGroup::Open, None, now)
            .await
            .map_err(classify)?;
        orders.extend(
            broker
                .orders(OrderGroup::Closed, None, now)
                .await
                .map_err(classify)?,
        );
        let mut result = Reconciled::default();
        let mut changes = Vec::new();
        let mut state = self.lock();
        state.positions.clear();
        for trade in self.store.trades_in_state(TradeState::Ordered)? {
            let key = buy_key(trade.id);
            let buy = orders
                .iter()
                .find(|o| o.client_order_id.as_deref() == Some(key.as_str()));
            let Some(order) = buy else {
                let detail = "미체결 취소: 다시 켠 뒤 맞춰 보니 증권사에 이 매매의 주문이 없음";
                block_unfilled(&self.store, trade.id, detail.to_string(), at, &mut changes)?;
                result.blocked.push(trade.id);
                continue;
            };
            apply(&self.store, &mut state, order, at, &mut changes)?;
            match self.store.get_trade(trade.id)?.state {
                TradeState::Holding => result.holding.push(trade.id),
                TradeState::Blocked => result.blocked.push(trade.id),
                _ => result.open.push(trade.id),
            }
        }
        for trade in self.store.trades_in_state(TradeState::Holding)? {
            for side in [Side::Buy, Side::Sell] {
                let mine = orders.iter().filter(|o| {
                    o.client_order_id.as_deref().and_then(parse_key) == Some((trade.id, side))
                });
                for order in mine {
                    apply(&self.store, &mut state, order, at, &mut changes)?;
                }
            }
        }
        state.ready = true;
        drop(state);
        self.persist()?;
        Ok(result)
    }

    /// 사는 주문을 낸다(C10). 돌려주는 값은 증권사 주문 식별자다.
    ///
    /// 순서: 준비됨(E16) → 계좌 종류(E25: 템플릿 · 매매 · 증권사가 모두 같음) → 표시 내용과 매매
    /// (종목 · 사기) → 승인(승인대기면 승인 소비 결과 필요) → 매매를 주문됨으로 **저장** → `place_order`.
    /// 앞의 것에서 막히면 매매는 그대로이고 주문을 내지 않는다. 저장이 거부되면 [`Error::Store`]이고
    /// 주문을 내지 않는다.
    ///
    /// 증권사가 받지 않으면 매매를 막힘(미체결 취소, 이유 남김)으로 옮기고: 받지 않음 · 응답 없음은
    /// [`Error::AnomalyStop`](E13), 그 밖은 [`Error::Rejected`](점검 표시가 다름 E33 등).
    pub async fn submit(&self, submission: Submission) -> Result<OrderId, Error> {
        let now = self.clock.now();
        if !self.ready() {
            return Err(Error::NotReady);
        }
        let id = submission.trade_id;
        let trade = self.store.get_trade(id)?;
        let approved = submission.permit.approved().clone();
        self.check_account(submission.template_account)?;
        self.check_account(paper_kind(trade.account))?;
        self.check_account(approved.account_kind)?;
        if approved.symbol.as_str() != trade.symbol || approved.side != Side::Buy {
            return Err(Error::PermitMismatch);
        }
        if trade.state == TradeState::AwaitingApproval && submission.approval.is_none() {
            return Err(Error::ApprovalRequired);
        }
        self.store
            .transition_trade(id, TradeState::Ordered, millis(now))?;
        let confirm = submission.approval.is_some();
        let request = order_request(
            &approved,
            buy_key(id),
            submission.timeout,
            confirm,
            submission.permit,
        );
        let placed = self.broker.place_order(request, now).await;
        match placed {
            Ok(ack) => {
                self.persist()?;
                Ok(ack.order_id)
            }
            Err(e) => {
                let detail = format!("미체결 취소: 증권사가 주문을 받지 않음({e})");
                block_unfilled(&self.store, id, detail, millis(now), &mut Vec::new())?;
                self.persist()?;
                Err(classify(e))
            }
        }
    }

    /// 파는 주문을 낸다. 손절 · 청산은 먼저 같은 종목의 열린 파는 주문을 취소한다.
    ///
    /// 증권사가 받지 않거나 응답하지 않으면 [`Error::AnomalyStop`]이고, 그 주문은 다시 내기를
    /// 기다리는 것([`Executor::pending_sells`])으로 남는다(E30). 매매는 보유중 그대로다.
    pub async fn sell(&self, submission: SellSubmission) -> Result<OrderId, Error> {
        if !self.ready() {
            return Err(Error::NotReady);
        }
        let id = submission.trade_id;
        let trade = self.store.get_trade(id)?;
        if trade.state != TradeState::Holding {
            return Err(Error::NotHolding(trade.state));
        }
        let approved = submission.permit.approved().clone();
        self.check_account(paper_kind(trade.account))?;
        self.check_account(approved.account_kind)?;
        if approved.symbol.as_str() != trade.symbol || approved.side != Side::Sell {
            return Err(Error::PermitMismatch);
        }
        let attempt = SellAttempt {
            trade_id: id,
            purpose: submission.purpose,
            order: approved,
            timeout: submission.timeout,
            failed_before: 0,
        };
        self.send_sell(attempt, submission.permit).await
    }

    /// 실패한 파는 주문을 새 표시로 다시 낸다(E30). 표시 내용은 처음 내용과 같아야 한다.
    ///
    /// - 기다리는 것이 없으면 [`Error::NoPendingSell`].
    /// - 아직 때가 아니면 [`Error::RetryNotDue`](증권사를 부르지 않는다).
    /// - 또 실패하면 [`Error::AnomalyStop`]이고 횟수가 하나 늘며 간격이 두 배가 된다(가장 길게
    ///   [`SELL_RETRY_MAX`]).
    /// - 되면 기다리는 목록에서 빠진다.
    pub async fn retry_sell(
        &self,
        trade_id: TradeId,
        permit: CheckPermit,
    ) -> Result<OrderId, Error> {
        if !self.ready() {
            return Err(Error::NotReady);
        }
        let pending = self.lock().pending.get(&trade_id).cloned();
        let Some((retry, timeout)) = pending else {
            return Err(Error::NoPendingSell(trade_id));
        };
        if permit.approved() != &retry.order {
            return Err(Error::PermitMismatch);
        }
        if self.clock.now() < retry.retry_at {
            return Err(Error::RetryNotDue {
                retry_at: retry.retry_at,
            });
        }
        let state = self.store.get_trade(trade_id)?.state;
        if state != TradeState::Holding {
            self.lock().pending.remove(&trade_id);
            return Err(Error::NotHolding(state));
        }
        let attempt = SellAttempt {
            trade_id,
            purpose: retry.purpose,
            order: retry.order,
            timeout,
            failed_before: retry.attempts,
        };
        self.send_sell(attempt, permit).await
    }

    /// 다시 내기를 기다리는 파는 주문들. 매매 번호 순서다.
    pub fn pending_sells(&self) -> Vec<SellRetry> {
        self.lock()
            .pending
            .values()
            .map(|(retry, _)| retry.clone())
            .collect()
    }

    /// 다시 내기를 기다리는 파는 주문의 멱등성 키(`client_order_id`). 기다리는 것이 없으면 없음.
    ///
    /// 한 시도 묶음(처음 낸 것과 그 다시 내기들)은 같은 키를 쓴다. 키는 스냅샷에 함께 남아
    /// 다시 켠 뒤에도 같다(E30). 되면 그 주문의 `client_order_id`가 이 키다.
    pub fn pending_sell_key(&self, trade_id: TradeId) -> Option<String> {
        let _ = trade_id;
        todo!()
    }

    /// 손절 감시 자리들. 매매 번호 순서다.
    pub fn guards(&self) -> Vec<Guard> {
        self.lock().guards()
    }

    /// 시세를 모의계좌에 넘기고 생긴 체결 알림을 매매 전이로 옮긴다.
    ///
    /// - 사는 주문이 (일부라도) 체결되면 주문됨 → 보유중, 손절 감시를 체결된 수량으로 건다.
    /// - 일부 체결 뒤 남은 것이 취소되면 보유중 그대로, 손절 감시는 체결된 수량(E29).
    /// - 하나도 체결되지 않고 취소되면 막힘(미체결 취소)으로 옮기고 이유를 남긴다(E12).
    /// - 파는 주문이 다 체결되면 보유중 → 팔림, 손절 감시를 지운다.
    ///
    /// 바뀐 것이 있으면 계좌 스냅샷을 저장한다.
    pub fn on_quote(&self, quote: &Quote) -> Result<Vec<TradeChange>, Error> {
        let now = self.clock.now();
        let events = self.broker.on_quote(quote, now);
        self.absorb(events, now)
    }

    /// 시간이 흐른 것을 모의계좌에 알리고(마감 취소) 생긴 알림을 [`Executor::on_quote`]와 같이 옮긴다.
    pub fn on_time(&self) -> Result<Vec<TradeChange>, Error> {
        let now = self.clock.now();
        let events = self.broker.on_time(now);
        self.absorb(events, now)
    }
}
