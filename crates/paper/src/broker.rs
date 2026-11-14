//! 증권사 trait. 주문을 내는 쪽(에이전트 실행기 · 규칙 엔진)은 이것 하나만 안다.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use pi_market::{Currency, Symbol};
use rust_decimal::Decimal;
use tokio::sync::mpsc::UnboundedReceiver;

use crate::{
    AccountKind, BrokerError, Commission, Holding, ModifyRequest, Order, OrderAck, OrderEvent,
    OrderGroup, OrderId, OrderRequest,
};

/// 증권사. 모양은 토스증권 공식 명세의 주문 · 계좌 API를 따른다(메서드마다 어느 API인지 적는다).
///
/// 모의계좌([`crate::PaperBroker`])가 이것의 구현이다. 나중에 실제 증권사 구현을 더해도
/// 주문을 내는 쪽 코드는 바뀌지 않는다. 시각은 부르는 쪽이 `now`로 준다.
#[async_trait]
pub trait Broker: Send + Sync {
    /// 이 구현의 계좌 종류 (E25 · V29).
    fn account_kind(&self) -> AccountKind;

    /// 주문 생성 API. 받으면 주문 식별자를 돌려주고, 받지 않으면 이유를 돌려준다
    /// (이때는 주문 기록이 남지 않고 돈 · 수량도 묶이지 않는다).
    async fn place_order(
        &self,
        request: OrderRequest,
        now: DateTime<Utc>,
    ) -> Result<OrderAck, BrokerError>;

    /// 주문 정정 API. 원주문은 `Replaced`가 되고, 새로 발급된 식별자의 주문이 그 자리를 잇는다.
    /// 돌려주는 값은 새 주문의 식별자다(원주문의 식별자와 다르다).
    async fn modify_order(
        &self,
        order_id: &OrderId,
        request: ModifyRequest,
        now: DateTime<Utc>,
    ) -> Result<OrderId, BrokerError>;

    /// 주문 취소 API. 원주문은 `Canceled`가 된다. 돌려주는 값은 취소 요청에 새로 발급된
    /// 식별자다(원주문의 식별자와 다르다. 명세 그대로).
    async fn cancel_order(
        &self,
        order_id: &OrderId,
        now: DateTime<Utc>,
    ) -> Result<OrderId, BrokerError>;

    /// 주문 목록 API. 접수한 순서대로 돌려준다. `symbol`을 주면 그 종목만.
    async fn orders(
        &self,
        group: OrderGroup,
        symbol: Option<&Symbol>,
    ) -> Result<Vec<Order>, BrokerError>;

    /// 주문 상세 API. 모든 상태의 주문을 볼 수 있다.
    async fn order(&self, order_id: &OrderId) -> Result<Order, BrokerError>;

    /// 매수 가능 금액 API. 그 통화로 지금 살 수 있는 현금(묶인 돈 · 결제 전인 돈은 뺀다).
    async fn buying_power(&self, currency: Currency) -> Result<Decimal, BrokerError>;

    /// 판매 가능 수량 API. 보유 수량에서 열린 파는 주문에 묶인 수량을 뺀 것. 없는 종목은 0.
    async fn sellable_quantity(&self, symbol: &Symbol) -> Result<Decimal, BrokerError>;

    /// 매매 수수료 API. 시장별 수수료율.
    async fn commissions(&self) -> Result<Vec<Commission>, BrokerError>;

    /// 보유 주식 API. 종목 글자 순서로 돌려준다. `symbol`을 주면 그 종목만. 없으면 빈 목록.
    async fn holdings(&self, symbol: Option<&Symbol>) -> Result<Vec<Holding>, BrokerError>;

    /// 실시간 본인 주문 이벤트(체결 알림). 구독한 뒤에 일어난 이벤트를 빠짐없이 순서대로 받는다.
    fn subscribe(&self) -> UnboundedReceiver<OrderEvent>;
}
