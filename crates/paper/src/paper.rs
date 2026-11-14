//! 모의계좌. [`Broker`]의 구현이다.
//!
//! # 주문을 받을 때의 확인 (순서대로, 처음 걸린 것을 돌려준다)
//!
//! 1. 장애 흉내가 켜져 있으면 [`BrokerError::Unavailable`] / [`BrokerError::NoResponse`].
//! 2. 계좌 종류가 다르면 [`BrokerError::AccountKindMismatch`] (E25).
//! 3. 점검 통과 표시: 없으면 `CheckRequired`, 이 계좌의 것이 아니면 `CheckInvalid`,
//!    승인 내용과 주문 내용이 다르면 `CheckMismatch` (E33).
//! 4. 멱등성 키: 같은 키 · 같은 내용이 [`IDEMPOTENCY_WINDOW`] 안에 있었으면 앞의 결과를 그대로
//!    돌려준다(새 주문 없음). 내용이 다르면 `IdempotencyKeyConflict`.
//! 5. 모양: 수량 · 값이 0 이하, 소수점 수량, 지정가인데 값 없음, 시장가인데 값 있음,
//!    호가 단위에 맞지 않음 → `InvalidRequest`. 다루지 않는 시장 → `UnsupportedMarket`.
//! 6. 장 시간: 그 시장의 달력이 없거나 `now`에 열린 세션이 없으면 `OrderHoursClosed`.
//!    세션은 열렸는데 그 세션에서 이 호가 유형만 못 쓰면 `OrderTypeNotAllowed`,
//!    어떤 유형도 못 쓰면 `OrderHoursClosed`.
//! 7. 상 · 하한가(제한폭이 있는 시장의 지정가): 기준가를 모르면 `ReferencePriceUnknown`,
//!    범위 밖이면 `PriceOutOfRange`. 상 · 하한가와 딱 같은 값은 받는다.
//! 8. 같은 종목에 반대 방향의 열린 주문이 있으면 `OppositePendingOrderExists`.
//! 9. 사기: 묶을 돈이 쓸 수 있는 돈보다 많으면 `InsufficientBuyingPower`.
//!    팔기: 수량이 팔 수 있는 수량보다 많으면 `InsufficientSellableQuantity`.
//!
//! 받지 않은 주문은 기록에 남지 않고 아무것도 묶지 않는다.
//!
//! # 묶는 돈과 수량
//!
//! - 지정가 사기: `남은 수량 × 지정가 + 올림(남은 수량 × 지정가 × 수수료율)`.
//! - 시장가 사기: 지정가 자리에 "마지막으로 받은 시세에 미끄러짐을 더해 호가 단위로 올린 값"을
//!   넣어 같은 식으로 셈한다. 그 종목의 시세를 한 번도 받지 못했으면 `QuoteUnknown`.
//! - 팔기: 남은 수량만큼 보유 수량을 묶는다.
//! - 체결될 때마다 남은 수량으로 다시 셈해 남는 만큼 푼다. 시장가 사기가 예상보다 비싸게
//!   체결돼 다시 셈한 값만큼 묶을 돈이 없으면 있는 만큼만 묶는다(쓸 수 있는 돈은 음수가 되지 않는다). 주문이 끝나면(전부 체결 · 취소 ·
//!   거부 · 정정됨) 그 주문이 묶은 것은 0이 된다.
//!
//! # 체결 ([`PaperBroker::on_quote`])
//!
//! - 늦은 시세([`pi_market::is_stale`], N6), 통화가 그 시장과 다른 시세, 장이 닫힌 때,
//!   지금 세션에서 그 호가 유형을 못 쓰는 때에는 체결하지 않는다.
//! - 지정가 사기는 시세 ≤ 지정가, 팔기는 시세 ≥ 지정가일 때. 체결 값은
//!   [`crate::LimitFillPrice`]를 따른다.
//! - 시장가는 접수한 **다음에 온** 시세에 체결한다. 값은 미끄러짐을 불리한 쪽으로 더한 값.
//! - 한 번에 체결되는 수량은 [`crate::MaxFill`]까지. 같은 종목의 주문은 접수한 순서대로 본다.
//! - 시장가 사기의 체결 비용(금액 + 수수료)이 그 주문이 묶은 돈과 쓸 수 있는 돈의 합보다
//!   크면, 살 수 있는 가장 큰 정수 수량만 체결하고 주문을 `Rejected`
//!   ([`crate::CloseReason::InsufficientBuyingPower`])로 끝낸다.
//! - 체결마다 수수료(사기 · 팔기)와 세금(팔기)을 뗀다. 판 돈(금액 − 수수료 − 세금)은
//!   [`crate::ProceedsUsable`]에 따라 바로 쓰거나 결제일까지 `unsettled`에 둔다.
//!
//! # 미체결 취소 (E12 · E29 · N4)
//!
//! [`PaperBroker::on_time`]과 [`PaperBroker::on_quote`]는 먼저 마감이 된(`now >= 마감`) 열린
//! 주문을 취소하고 이유([`crate::CloseReason::UnfilledTimeout`])를 남긴다. 체결된 만큼은 남는다.

use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use pi_market::{Currency, MarketCalendar, Quote, Symbol};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::UnboundedReceiver;

use crate::{
    AccountKind, Broker, BrokerError, CashBalance, CheckAuthority, Commission, Fill, Holding,
    ModifyRequest, Order, OrderAck, OrderEvent, OrderGroup, OrderId, OrderRequest, PaperConfig,
};

/// 멱등성 키가 살아 있는 시간: 10분 (토스 명세의 `clientOrderId` 설명).
/// 접수한 지 딱 이 시간이 되면 같은 키도 새 주문으로 본다.
pub const IDEMPOTENCY_WINDOW: Duration = Duration::from_secs(600);

/// 장애 흉내 (E13 · E30). 테스트와 시연에서 증권사 장애를 일으킨다.
///
/// 켜져 있는 동안 주문 생성 · 정정 · 취소가 실패한다. 아무것도 기록되거나 묶이지 않고,
/// 이미 있던 주문 · 돈 · 보유는 그대로다. 조회 · 시세 · 시간 처리는 평소대로 된다.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Fault {
    /// 증권사가 주문을 받지 않는다 → [`BrokerError::Unavailable`].
    RejectOrders,
    /// 증권사가 응답하지 않는다 → [`BrokerError::NoResponse`].
    NoResponse,
}

/// 계좌 전체를 담은 값 하나 (V6).
///
/// 현금(쓸 수 있는 돈 · 묶인 돈 · 결제 전인 돈과 그 결제일), 보유, 모든 주문(열린 것 · 끝난 것과
/// 저마다 묶은 것 · 마감), 체결 기록, 멱등성 키, 종목별 마지막 시세, 다음 식별자 번호가 들어 있다.
/// `serde`로 글자(JSON 등)로 바꿀 수 있다. 저장은 조립하는 쪽이 한다.
///
/// 들어 있지 않은 것: 달력 · 기준가(되살린 뒤 다시 넣는다), 장애 흉내, 구독자.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    /// 값의 모양 번호. 구현이 나머지 칸을 채운다.
    version: u32,
}

/// 모의계좌.
#[derive(Debug)]
pub struct PaperBroker {
    /// 구현이 상태(잠금 안)를 채운다.
    config: PaperConfig,
}

impl PaperBroker {
    /// 처음 돈으로 새 계좌를 연다. 함께 나오는 [`CheckAuthority`]만이 이 계좌가 받는 점검 통과
    /// 표시를 낼 수 있다. 처음 돈이나 비율이 음수면 [`BrokerError::InvalidConfig`].
    pub fn open(config: PaperConfig) -> Result<(Self, CheckAuthority), BrokerError> {
        todo!()
    }

    /// 내보낸 값에서 계좌를 되살린다. `config.initial_cash`는 쓰지 않는다. 권한은 새로 나온다
    /// (되살리기 전의 표시는 쓸 수 없다). 되살린 뒤 달력과 기준가를 다시 넣고 같은 시세 · 시각을
    /// 주면 끊기지 않은 계좌와 같은 결과가 나온다. 식별자도 이어서 매긴다.
    pub fn restore(
        config: PaperConfig,
        snapshot: Snapshot,
    ) -> Result<(Self, CheckAuthority), BrokerError> {
        todo!()
    }

    /// 계좌 전체를 값 하나로 내보낸다. 같은 상태는 같은 값(`==`)이다.
    pub fn snapshot(&self) -> Snapshot {
        todo!()
    }

    /// 그 시장(`calendar.market`)의 달력을 넣는다. 다시 넣으면 바꾼다.
    ///
    /// 결제도 여기서 본다: 결제 전인 돈 가운데 결제 예정일이 이 달력의 기준일
    /// (`calendar.today.date`) **이하**인 것은 쓸 수 있는 돈으로 옮긴다.
    pub fn set_calendar(&self, calendar: MarketCalendar) {
        todo!()
    }

    /// 종목의 기준가(전일 종가)를 넣는다. 상 · 하한가를 셈하는 데 쓴다.
    pub fn set_reference_price(&self, symbol: Symbol, price: Decimal) {
        todo!()
    }

    /// 시세 하나를 준다. 먼저 마감이 된 주문을 취소하고, 그 종목의 열린 주문을 체결해 본다.
    /// 이 호출로 생긴 이벤트를 일어난 순서대로 돌려준다(구독자에게도 같은 것이 간다).
    /// 체결하지 않더라도 이 시세를 그 종목의 마지막 시세로 기억한다.
    pub fn on_quote(&self, quote: &Quote, now: DateTime<Utc>) -> Vec<OrderEvent> {
        todo!()
    }

    /// 시각만 알려 준다. 마감이 된 열린 주문을 취소한다. 생긴 이벤트를 돌려준다.
    pub fn on_time(&self, now: DateTime<Utc>) -> Vec<OrderEvent> {
        todo!()
    }

    /// 장애 흉내를 켜거나(`Some`) 끈다(`None`).
    pub fn set_fault(&self, fault: Option<Fault>) {
        todo!()
    }

    /// 그 통화의 현금. 다루지 않는 통화는 모두 0이다.
    pub fn cash(&self, currency: &Currency) -> CashBalance {
        todo!()
    }

    /// 체결 기록 전부(일어난 순서대로).
    pub fn fills(&self) -> Vec<Fill> {
        todo!()
    }
}

#[async_trait]
impl Broker for PaperBroker {
    /// 언제나 [`AccountKind::Paper`].
    fn account_kind(&self) -> AccountKind {
        todo!()
    }

    async fn place_order(
        &self,
        request: OrderRequest,
        now: DateTime<Utc>,
    ) -> Result<OrderAck, BrokerError> {
        todo!()
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
        todo!()
    }

    /// 묶인 돈 · 수량을 풀고 이유([`crate::CloseReason::CancelRequested`])를 남긴다.
    /// 끝난 주문이면 `AlreadyFilled` · `AlreadyCanceled` · `AlreadyModified` · `AlreadyRejected`.
    async fn cancel_order(
        &self,
        order_id: &OrderId,
        now: DateTime<Utc>,
    ) -> Result<OrderId, BrokerError> {
        todo!()
    }

    async fn orders(
        &self,
        group: OrderGroup,
        symbol: Option<&Symbol>,
    ) -> Result<Vec<Order>, BrokerError> {
        todo!()
    }

    async fn order(&self, order_id: &OrderId) -> Result<Order, BrokerError> {
        todo!()
    }

    async fn buying_power(&self, currency: Currency) -> Result<Decimal, BrokerError> {
        todo!()
    }

    async fn sellable_quantity(&self, symbol: &Symbol) -> Result<Decimal, BrokerError> {
        todo!()
    }

    /// 설정의 국내 · 미국 수수료율을 그 순서로 돌려준다.
    async fn commissions(&self) -> Result<Vec<Commission>, BrokerError> {
        todo!()
    }

    async fn holdings(&self, symbol: Option<&Symbol>) -> Result<Vec<Holding>, BrokerError> {
        todo!()
    }

    fn subscribe(&self) -> UnboundedReceiver<OrderEvent> {
        todo!()
    }
}
