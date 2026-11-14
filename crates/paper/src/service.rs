//! [`Broker`]의 구현: 모의계좌를 증권사 trait으로 쓴다. 조회도 먼저 `now`까지 마감이 된 주문을
//! 취소한다(시간이 흘렀다고 따로 알려 주지 않아도 마감 지난 주문이 열린 채로 보이지 않는다).

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use pi_market::{Currency, Symbol};
use rust_decimal::Decimal;
use tokio::sync::mpsc::{self, Receiver};

use crate::ledger::MarketCode;
use crate::orders;
use crate::{
    AccountKind, Broker, BrokerError, Commission, Holding, ModifyRequest, Order, OrderAck,
    OrderEvent, OrderGroup, OrderId, OrderRequest, PaperBroker,
};

#[async_trait]
impl Broker for PaperBroker {
    /// 언제나 [`AccountKind::Paper`].
    fn account_kind(&self) -> AccountKind {
        AccountKind::Paper
    }

    async fn place_order(
        &self,
        request: OrderRequest,
        now: DateTime<Utc>,
    ) -> Result<OrderAck, BrokerError> {
        orders::place(&mut self.book(), &self.config, &self.seal, request, now)
    }

    /// 확인하는 순서: 장애 흉내 → 원주문(없으면 `OrderNotFound`, 끝났으면 `AlreadyFilled` ·
    /// `AlreadyCanceled` · `AlreadyModified` · `AlreadyRejected`) → 정정의 모양(국내인데 수량이
    /// 없으면 `ModifyQuantityRequired`, 미국인데 수량이 있으면 `UsModifyQuantityNotSupported`)
    /// → 접수 때와 같은 확인(표시 · 모양 · 장 시간 · 상하한 · 살 돈 · 팔 수량).
    ///
    /// 새 주문의 수량은 국내는 요청의 수량, 미국은 원주문의 남은 수량이다. 표시가 승인한 내용은
    /// 이 새 주문의 내용과 같아야 한다. 살 돈 · 팔 수량은 원주문이 묶은 것을 푼 것으로 보고
    /// 셈한다. 받지 않으면 원주문은 그대로다. 새 주문은 원주문의 마감을 물려받는다.
    /// 이벤트는 원주문의 `Replaced`, 새 주문의 `Pending` 순서로 낸다.
    async fn modify_order(
        &self,
        order_id: &OrderId,
        request: ModifyRequest,
        now: DateTime<Utc>,
    ) -> Result<OrderId, BrokerError> {
        let mut book = self.book();
        orders::modify(&mut book, &self.config, &self.seal, order_id, request, now)
    }

    /// 묶인 돈 · 수량을 풀고 이유([`crate::CloseReason::CancelRequested`])를 남긴다.
    /// 끝난 주문이면 `AlreadyFilled` · `AlreadyCanceled` · `AlreadyModified` · `AlreadyRejected`.
    async fn cancel_order(
        &self,
        order_id: &OrderId,
        now: DateTime<Utc>,
    ) -> Result<OrderId, BrokerError> {
        orders::cancel(&mut self.book(), order_id, now)
    }

    async fn orders(
        &self,
        group: OrderGroup,
        symbol: Option<&Symbol>,
        now: DateTime<Utc>,
    ) -> Result<Vec<Order>, BrokerError> {
        let open = group == OrderGroup::Open;
        Ok(self
            .settled(now)
            .ledger
            .orders
            .iter()
            .filter(|order| order.is_open() == open)
            .filter(|order| symbol.is_none_or(|symbol| symbol.as_str() == order.symbol))
            .map(|order| order.view())
            .collect())
    }

    async fn order(&self, order_id: &OrderId, now: DateTime<Utc>) -> Result<Order, BrokerError> {
        let book = self.settled(now);
        book.ledger
            .order_index(order_id)
            .and_then(|index| book.ledger.orders.get(index))
            .map(|order| order.view())
            .ok_or(BrokerError::OrderNotFound)
    }

    async fn buying_power(
        &self,
        currency: Currency,
        now: DateTime<Utc>,
    ) -> Result<Decimal, BrokerError> {
        Ok(self.cash(&currency, now).available)
    }

    async fn sellable_quantity(
        &self,
        symbol: &Symbol,
        now: DateTime<Utc>,
    ) -> Result<Decimal, BrokerError> {
        let book = self.settled(now);
        Ok([MarketCode::Kr, MarketCode::Us]
            .into_iter()
            .map(|market| book.ledger.sellable(symbol.as_str(), market))
            .sum())
    }

    /// 설정의 국내 · 미국 수수료율을 그 순서로 돌려준다.
    async fn commissions(&self) -> Result<Vec<Commission>, BrokerError> {
        Ok([MarketCode::Kr, MarketCode::Us]
            .into_iter()
            .filter_map(|code| {
                let market = code.market();
                self.config.rules(market).map(|rules| Commission {
                    market,
                    commission_rate: rules.commission_rate,
                })
            })
            .collect())
    }

    async fn holdings(
        &self,
        symbol: Option<&Symbol>,
        now: DateTime<Utc>,
    ) -> Result<Vec<Holding>, BrokerError> {
        Ok(self.settled(now).ledger.holding_views(symbol))
    }

    /// 구독 하나가 쥐는 이벤트는 [`crate::PaperConfig::event_buffer`]개까지다. 받는 쪽이 읽지 않아
    /// 그 개수가 찬 뒤에 이벤트가 또 생기면 그 구독만 끊는다.
    fn subscribe(&self) -> Receiver<OrderEvent> {
        // 0개짜리 설정으로는 계좌를 열 수 없다. 그래도 통로는 한 칸 이상이어야 만들어진다.
        let (sender, receiver) = mpsc::channel(self.config.event_buffer.max(1));
        self.book().subscribers.push(sender);
        receiver
    }
}
