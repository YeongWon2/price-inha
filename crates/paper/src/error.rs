//! 오류. 거부 이유는 실제 증권사(토스증권 공식 명세의 주문 생성 · 정정 · 취소 API가
//! 돌려주는 오류 코드)와 같은 뜻으로 나눈다. 괄호 안은 명세의 오류 코드다.

use rust_decimal::Decimal;
use thiserror::Error;

use crate::AccountKind;

/// 요청 모양이 틀린 이유 (명세의 `invalid-request`).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum InvalidReason {
    /// 수량이 0 이하다.
    NonPositiveQuantity,
    /// 수량에 소수점이 있다. 모의계좌는 정수 수량만 받는다.
    FractionalQuantity,
    /// 값이 0 이하다.
    NonPositivePrice,
    /// 지정가 주문에 값이 없다.
    LimitPriceRequired,
    /// 시장가 주문에 값이 있다.
    MarketPriceNotAllowed,
    /// 값이 호가 단위에 맞지 않는다. `tick`은 그 값에 맞는 호가 단위다
    /// (명세: 올바른 호가 단위가 오류의 `data`에 실린다).
    TickSize {
        /// 그 값 구간의 호가 단위.
        tick: Decimal,
    },
    /// 국내 주식 정정에 수량이 없다(명세: 국내는 수량이 필수).
    ModifyQuantityRequired,
}

/// 증권사가 돌려주는 오류.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[non_exhaustive]
pub enum BrokerError {
    /// 요청 모양이 틀렸다 (`invalid-request`).
    #[error("주문 요청이 올바르지 않습니다: {0:?}")]
    InvalidRequest(InvalidReason),
    /// 살 돈이 모자란다. 수수료까지 넣어 본다 (`insufficient-buying-power`).
    #[error("주문 가능 금액이 부족합니다")]
    InsufficientBuyingPower,
    /// 팔 수 있는 수량이 모자란다. 없는 주식은 팔 수 없다(공매도 없음).
    #[error("판매 가능 수량이 부족합니다")]
    InsufficientSellableQuantity,
    /// 장이 닫혀 있거나 그 시장의 달력을 모른다 (`order-hours-closed`).
    #[error("지금은 주문을 접수할 수 없는 시간입니다")]
    OrderHoursClosed,
    /// 열린 세션에서 그 호가 유형을 쓸 수 없다 (`order-type-not-allowed`).
    #[error("지금 쓸 수 없는 호가 유형입니다")]
    OrderTypeNotAllowed,
    /// 값이 상 · 하한가를 벗어났다 (`price-out-of-range`).
    #[error("주문 가격이 허용 범위를 벗어났습니다")]
    PriceOutOfRange,
    /// 상 · 하한가가 있는 시장인데 그 종목의 기준가(전일 종가)를 모른다. 확인할 수 없으니 받지 않는다.
    #[error("기준가를 몰라 상·하한가를 확인할 수 없습니다")]
    ReferencePriceUnknown,
    /// 시장가로 살 때 묶을 돈을 셈할 시세를 아직 한 번도 받지 못했다.
    #[error("시세를 몰라 시장가 주문을 받을 수 없습니다")]
    QuoteUnknown,
    /// 같은 종목에 반대 방향의 열린 주문이 있다 (`opposite-pending-order-exists`).
    #[error("같은 종목에 반대 방향의 체결 대기 주문이 있습니다")]
    OppositePendingOrderExists,
    /// 같은 `client_order_id`로 내용이 다른 주문을 냈다 (`idempotency-key-conflict`).
    #[error("같은 주문 키로 다른 내용의 주문을 낼 수 없습니다")]
    IdempotencyKeyConflict,
    /// 점검 통과 표시가 없다 (E33).
    #[error("점검 통과 표시가 없는 주문은 받지 않습니다")]
    CheckRequired,
    /// 점검 통과 표시가 이 증권사와 짝인 발급자가 낸 것이 아니다 (E33).
    #[error("점검 통과 표시가 이 계좌의 것이 아닙니다")]
    CheckInvalid,
    /// 점검 통과 표시가 승인한 내용과 주문 내용이 다르다 (E33).
    #[error("점검을 통과한 내용과 주문 내용이 다릅니다")]
    CheckMismatch,
    /// 주문에 적힌 계좌 종류가 이 증권사의 종류와 다르다 (E25).
    #[error("주문의 계좌 종류({order:?})가 이 계좌의 종류({broker:?})와 다릅니다")]
    AccountKindMismatch {
        /// 주문에 적힌 계좌 종류.
        order: AccountKind,
        /// 이 증권사 구현의 계좌 종류.
        broker: AccountKind,
    },
    /// 이 계좌가 다루지 않는 시장이다.
    #[error("이 계좌가 다루지 않는 시장입니다")]
    UnsupportedMarket,
    /// 그런 주문이 없다 (`order-not-found`).
    #[error("주문을 찾을 수 없습니다")]
    OrderNotFound,
    /// 이미 전부 체결됐다 (`already-filled`).
    #[error("이미 체결된 주문입니다")]
    AlreadyFilled,
    /// 이미 취소됐다 (`already-canceled`).
    #[error("이미 취소된 주문입니다")]
    AlreadyCanceled,
    /// 이미 정정돼 다른 주문으로 바뀌었다 (`already-modified`).
    #[error("이미 정정된 주문입니다")]
    AlreadyModified,
    /// 거부된 주문이다 (`already-rejected`).
    #[error("거부된 주문은 정정하거나 취소할 수 없습니다")]
    AlreadyRejected,
    /// 미국 주식 정정에 수량을 줬다 (`us-modify-quantity-not-supported`).
    #[error("미국 주식 주문 정정은 가격만 바꿀 수 있습니다")]
    UsModifyQuantityNotSupported,
    /// 증권사가 주문을 받지 않는다 (E13 · E30. 명세의 `internal-error` · `maintenance`에 해당).
    #[error("증권사가 주문을 받지 않습니다")]
    Unavailable,
    /// 증권사가 응답하지 않는다 (E13 · E30).
    #[error("증권사가 응답하지 않습니다")]
    NoResponse,
    /// 설정 값이 틀렸다(음수인 처음 돈 · 비율 등).
    #[error("모의계좌 설정이 올바르지 않습니다: {0}")]
    InvalidConfig(String),
    /// 되살릴 수 없는 상태 값이다.
    #[error("모의계좌 상태 값을 되살릴 수 없습니다: {0}")]
    InvalidSnapshot(String),
}
