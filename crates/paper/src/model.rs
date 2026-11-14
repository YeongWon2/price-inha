//! 주문 · 계좌 값. 토스증권 공식 명세의 주문 · 계좌 API가 주고받는 값을 읽고 맞췄다
//! (주문 생성 · 정정 · 취소 · 목록 · 상세, 매수 가능 금액, 판매 가능 수량, 수수료, 보유,
//! 실시간 본인 주문 이벤트). 명세의 필드 이름은 문서 주석에 `backtick`으로 적는다.
//!
//! 명세에 있지만 이번에 다루지 않는 것: 금액 기반 주문(`orderAmount`), 소수점 수량,
//! 주문 유효 조건(`timeInForce`의 `CLS` · `OPG`), 고액 주문 확인(`confirmHighValueOrder`).

use std::time::Duration;

use chrono::{DateTime, NaiveDate, Utc};
use pi_market::{Currency, Market, Symbol};
use rust_decimal::Decimal;

use crate::CheckPermit;

/// 계좌 종류 (E25 · V29).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AccountKind {
    /// 모의계좌. 이번에 만드는 것은 이것뿐이다.
    Paper,
    /// 실전 계좌. 이 크레이트에는 실전 구현이 없다. 주문에 이 값이 적혀 오면 모의계좌는 받지 않는다.
    Live,
}

/// 주문 식별자 (`orderId`). 증권사가 낸 글자 그대로다.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct OrderId(String);

impl OrderId {
    /// 글자 그대로 식별자를 만든다.
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// 식별자 글자.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// 주문 방향 (`side`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Side {
    /// 사기 (`BUY`).
    Buy,
    /// 팔기 (`SELL`).
    Sell,
}

/// 호가 유형 (`orderType`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OrderType {
    /// 지정가 (`LIMIT`).
    Limit,
    /// 시장가 (`MARKET`).
    Market,
}

/// 주문 상태 (`OrderStatus`). 이름과 뜻은 명세 그대로다.
///
/// 모의계좌가 실제로 만드는 상태는 `Pending` → (`PartialFilled`) → `Filled` / `Canceled` /
/// `Rejected` / `Replaced`다. 나머지(`PendingCancel` · `PendingReplace` · `CancelRejected` ·
/// `ReplaceRejected`)는 실제 증권사 구현을 위해 이름만 둔다.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum OrderStatus {
    /// 체결 대기 (`PENDING`).
    Pending,
    /// 취소 대기 (`PENDING_CANCEL`).
    PendingCancel,
    /// 정정 대기 (`PENDING_REPLACE`).
    PendingReplace,
    /// 일부 체결 (`PARTIAL_FILLED`).
    PartialFilled,
    /// 전부 체결 (`FILLED`).
    Filled,
    /// 취소됨 (`CANCELED`). 일부 체결됐는지는 [`Execution::filled_quantity`]로 본다.
    Canceled,
    /// 거부됨 (`REJECTED`). 일부 체결됐는지는 [`Execution::filled_quantity`]로 본다.
    Rejected,
    /// 취소 거부 (`CANCEL_REJECTED`).
    CancelRejected,
    /// 정정 거부 (`REPLACE_REJECTED`).
    ReplaceRejected,
    /// 정정돼 다른 주문으로 바뀜 (`REPLACED`).
    Replaced,
}

impl OrderStatus {
    /// 진행 중인가. 명세의 주문 목록 필터 `OPEN` 묶음과 같다:
    /// `Pending` · `PartialFilled` · `PendingCancel` · `PendingReplace`.
    pub fn is_open(self) -> bool {
        todo!()
    }
}

/// 주문 목록 필터 (주문 목록 API의 `status` 값).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OrderGroup {
    /// 진행 중인 주문 (`OPEN`).
    Open,
    /// 끝난 주문 (`CLOSED`): 전부 체결 · 취소 · 거부 · 정정됨.
    Closed,
}

/// 체결되지 않은 주문을 언제 취소하는가 (N4 · E12 · E29).
///
/// 접수한 시각부터 잰다. 그 시각이 **되면**(`지금 >= 마감`) 남은 부분을 취소한다.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UnfilledTimeout {
    /// 접수하고 이만큼 지나면 취소한다.
    After(Duration),
    /// 주문을 접수한 세션이 끝나면 취소한다(장 마감까지).
    SessionClose,
}

impl UnfilledTimeout {
    /// 단타의 기본값: 1분 (N4).
    pub const SCALP: Self = Self::After(Duration::from_secs(60));
    /// 스윙의 기본값: 10분 (N4).
    pub const SWING: Self = Self::After(Duration::from_secs(600));
    /// 중장기의 기본값: 장 마감까지 (N4).
    pub const LONG_TERM: Self = Self::SessionClose;
}

/// 주문이 다 체결되지 않고 끝난 이유.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum CloseReason {
    /// 취소를 요청받았다.
    CancelRequested,
    /// 정한 시간 안에 체결되지 않았다 (E12 · E29).
    UnfilledTimeout,
    /// 시장가로 사는 주문을 체결하려는데 돈이 모자랐다(시세가 접수 때보다 올랐다).
    InsufficientBuyingPower,
    /// 정정돼 다른 주문으로 바뀌었다.
    Replaced,
}

/// 주문 요청 (주문 생성 API의 수량 기반 요청 + 우리 쪽 확인 값).
///
/// `market` · `account_kind` · `timeout` · `permit`은 명세에 없는 우리 쪽 값이다.
/// 표시([`CheckPermit`])는 복제할 수 없어 이 값도 복제할 수 없다. 주문 하나에 표시 하나다.
#[derive(Debug)]
pub struct OrderRequest {
    /// 멱등성 키 (`clientOrderId`). 같은 키 · 같은 내용으로 10분 안에 다시 내면 새 주문을
    /// 만들지 않고 앞의 결과를 그대로 돌려준다. 내용이 다르면 거부한다.
    pub client_order_id: Option<String>,
    /// 시장. 어느 시장 규칙(호가 단위 · 수수료 · 세션)을 쓸지 정한다.
    pub market: Market,
    /// 종목 (`symbol`).
    pub symbol: Symbol,
    /// 방향 (`side`).
    pub side: Side,
    /// 호가 유형 (`orderType`).
    pub order_type: OrderType,
    /// 수량 (`quantity`). 양의 정수만 받는다.
    pub quantity: Decimal,
    /// 값 (`price`). 지정가면 반드시 있고, 시장가면 없어야 한다.
    pub price: Option<Decimal>,
    /// 이 주문이 가려는 계좌 종류. 증권사 구현의 종류와 다르면 받지 않는다 (E25).
    pub account_kind: AccountKind,
    /// 체결되지 않으면 언제 취소할지 (N4).
    pub timeout: UnfilledTimeout,
    /// 규칙 엔진이 낸 점검 통과 표시. 없으면 받지 않는다 (E33).
    pub permit: Option<CheckPermit>,
}

/// 정정 요청 (주문 정정 API의 요청).
#[derive(Debug)]
pub struct ModifyRequest {
    /// 바꿀 호가 유형 (`orderType`).
    pub order_type: OrderType,
    /// 바꿀 수량 (`quantity`). 국내는 반드시 있어야 하고, 미국은 줄 수 없다(명세 그대로).
    /// 뜻: 정정 뒤 새 주문의 수량이다(아직 체결되지 않은 부분을 이 수량으로 다시 낸다).
    pub quantity: Option<Decimal>,
    /// 바꿀 값 (`price`). 지정가면 반드시 있고, 시장가면 없어야 한다.
    pub price: Option<Decimal>,
    /// 바뀐 내용에 대한 점검 통과 표시. 없으면 받지 않는다 (E33).
    pub permit: Option<CheckPermit>,
}

/// 주문 접수 결과 (`OrderResponse`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrderAck {
    /// 증권사가 낸 주문 식별자 (`orderId`). 정정 · 취소 · 조회에 쓴다.
    pub order_id: OrderId,
    /// 요청에 준 값 그대로 (`clientOrderId`).
    pub client_order_id: Option<String>,
}

/// 체결 결과 (`OrderExecution`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Execution {
    /// 체결된 수량 (`filledQuantity`). 없으면 0.
    pub filled_quantity: Decimal,
    /// 평균 체결 값 (`averageFilledPrice`). 체결이 없으면 `None`. 반올림하지 않는다.
    pub average_filled_price: Option<Decimal>,
    /// 체결 금액의 합 (`filledAmount`). 없으면 0.
    pub filled_amount: Decimal,
    /// 수수료의 합 (`commission`). 없으면 0.
    pub commission: Decimal,
    /// 세금의 합 (`tax`). 없으면 0.
    pub tax: Decimal,
    /// 마지막 체결 시각 (`filledAt`).
    pub filled_at: Option<DateTime<Utc>>,
    /// 마지막 체결의 결제 예정일 (`settlementDate`).
    pub settlement_date: Option<NaiveDate>,
}

/// 주문 하나 (`Order`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Order {
    /// 주문 식별자 (`orderId`).
    pub order_id: OrderId,
    /// 요청에 준 멱등성 키.
    pub client_order_id: Option<String>,
    /// 시장.
    pub market: Market,
    /// 종목 (`symbol`).
    pub symbol: Symbol,
    /// 방향 (`side`).
    pub side: Side,
    /// 호가 유형 (`orderType`).
    pub order_type: OrderType,
    /// 상태 (`status`).
    pub status: OrderStatus,
    /// 주문 값 (`price`). 시장가면 `None`.
    pub price: Option<Decimal>,
    /// 주문 수량 (`quantity`).
    pub quantity: Decimal,
    /// 통화 (`currency`).
    pub currency: Currency,
    /// 접수 시각 (`orderedAt`).
    pub ordered_at: DateTime<Utc>,
    /// 취소 시각 (`canceledAt`). 취소되지 않았으면 `None`.
    pub canceled_at: Option<DateTime<Utc>>,
    /// 체결 결과 (`execution`).
    pub execution: Execution,
    /// 다 체결되지 않고 끝났으면 그 이유.
    pub close_reason: Option<CloseReason>,
}

/// 체결 한 건(체결 기록). 시세 한 번에 주문 하나가 체결된 만큼이다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fill {
    /// 어느 주문의 체결인가.
    pub order_id: OrderId,
    /// 시장.
    pub market: Market,
    /// 종목.
    pub symbol: Symbol,
    /// 방향.
    pub side: Side,
    /// 체결 수량.
    pub quantity: Decimal,
    /// 체결 값.
    pub price: Decimal,
    /// 체결 금액 = 수량 × 값.
    pub amount: Decimal,
    /// 이 체결에서 뗀 수수료.
    pub commission: Decimal,
    /// 이 체결에서 뗀 세금(사기는 0).
    pub tax: Decimal,
    /// 통화.
    pub currency: Currency,
    /// 체결 시각(시세를 준 호출의 `now`).
    pub filled_at: DateTime<Utc>,
    /// 결제 예정일.
    pub settlement_date: NaiveDate,
}

/// 보유 종목 하나 (`HoldingsItem` 가운데 수량과 평균 산 값).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Holding {
    /// 종목 (`symbol`).
    pub symbol: Symbol,
    /// 시장 (`marketCountry`).
    pub market: Market,
    /// 통화 (`currency`).
    pub currency: Currency,
    /// 보유 수량 (`quantity`). 팔려고 묶인 수량도 들어 있다.
    pub quantity: Decimal,
    /// 평균 산 값 (`averagePurchasePrice`). 산 체결 값의 수량 가중 평균이다(수수료는 넣지 않는다).
    /// 팔아도 바뀌지 않는다. 반올림하지 않는다.
    pub average_purchase_price: Decimal,
}

/// 시장별 수수료율 (`Commission`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Commission {
    /// 시장 (`marketCountry`).
    pub market: Market,
    /// 수수료율, 소수 비율 (`commissionRate`). 예: `0.00015` = 0.015%.
    pub commission_rate: Decimal,
}

/// 통화 하나의 현금. 세 칸의 합이 그 통화의 돈 전부다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CashBalance {
    /// 지금 주문에 쓸 수 있는 돈. 매수 가능 금액(`cashBuyingPower`)과 같다.
    pub available: Decimal,
    /// 열린 사는 주문에 묶인 돈.
    pub held: Decimal,
    /// 판 돈 가운데 아직 결제일이 되지 않아 쓸 수 없는 돈.
    pub unsettled: Decimal,
}

impl CashBalance {
    /// 세 칸의 합.
    pub fn total(&self) -> Decimal {
        todo!()
    }
}

/// 주문 이벤트 종류 (실시간 본인 주문 이벤트의 `event`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum OrderEventKind {
    /// 접수 (`PENDING`).
    Pending,
    /// 일부 체결 (`PARTIAL_FILL`).
    PartialFill,
    /// 전부 체결 (`FILL`).
    Fill,
    /// 취소 중 (`CANCELING`). 모의계좌는 내지 않는다.
    Canceling,
    /// 취소됨 (`CANCELED`).
    Canceled,
    /// 정정 중 (`REPLACING`). 모의계좌는 내지 않는다.
    Replacing,
    /// 정정됨 (`REPLACED`). 원주문에 대해 낸다.
    Replaced,
    /// 거부됨 (`REJECTED`).
    Rejected,
    /// 취소 거부 (`CANCEL_REJECTED`). 모의계좌는 내지 않는다.
    CancelRejected,
    /// 정정 거부 (`REPLACE_REJECTED`). 모의계좌는 내지 않는다.
    ReplaceRejected,
}

/// 주문 이벤트 하나(체결 알림). 주문 상태가 바뀔 때마다 하나씩 나온다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrderEvent {
    /// 종류.
    pub kind: OrderEventKind,
    /// 그 시점의 주문 모습(주문 상세와 같은 모양).
    pub order: Order,
}
