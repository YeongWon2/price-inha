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

/// 상태 값을 되살릴 수 없는 이유 (V6).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum SnapshotReason {
    /// 모르는 모양 번호다.
    UnknownVersion(u32),
    /// 현금(쓸 수 있는 돈 · 결제 전인 돈)이 음수다.
    NegativeCash,
    /// 보유 수량이 0 이하다.
    NonPositiveHolding,
    /// 같은 종목 · 같은 시장의 보유가 둘 이상이다.
    DuplicateHolding,
    /// 주문 식별자가 겹친다.
    DuplicateOrderId,
    /// 주문 수량이 0 이하다.
    NonPositiveOrderQuantity,
    /// 주문의 체결 수량이 음수이거나, 정수가 아니거나(3차), 주문 수량보다 많다.
    BadFilledQuantity,
    /// 주문이 묶은 돈이 음수다.
    NegativeHeldCash,
    /// 끝난 주문에 묶인 돈이 남아 있다.
    ClosedOrderHoldsCash,
    /// 열린 사는 주문이 묶은 돈이 **남은 수량 몫**보다 많다(3차). 남은 수량 몫은 이 주문을
    /// 받을 때 · 체결할 때와 같은 식이다: `남은 수량 × 묶는 돈을 셈하는 값 + 올림((누적 체결 금액 +
    /// 남은 수량 × 묶는 돈을 셈하는 값) × 수수료율) − 이미 뗀 수수료`. 묶인 돈이 계좌의 돈을 부풀린다.
    HeldCashTooLarge,
    /// 열린 파는 주문들의 남은 수량 합이 그 종목의 보유 수량보다 많다.
    SellExceedsHolding,
    /// 다음 식별자 번호가 이미 낸 식별자(주문 · 체결 기록 · 멱등성 키가 가리키는 것)의 번호보다
    /// 크지 않다. 되살린 뒤 새 주문의 식별자가 앞의 것과 겹치게 된다.
    NextIdNotAhead,
    /// 주문의 뗀 수수료 · 세금 · 누적 체결 금액이 음수이거나, 주문 금액을 넘는다(3차).
    /// 지정가 사기의 누적 체결 금액은 `체결 수량 × 지정가`를 넘을 수 없고, 뗀 수수료 · 세금은
    /// 누적 체결 금액을 넘을 수 없다.
    BadExecution,
    /// 주문 상태와 체결 수량이 어긋난다(3차): 전부 체결(`Filled`)인데 체결 수량이 주문 수량과
    /// 다름, 일부 체결(`PartialFilled`)인데 체결 수량이 0이거나 주문 수량과 같음, 접수(`Pending`)인데
    /// 체결된 것이 있음 등.
    StatusMismatch,
    /// 주문의 통화가 그 시장의 통화와 다르다(3차).
    CurrencyMismatch,
    /// 보유의 평균 산 값이 0 이하다(3차).
    NonPositiveAveragePrice,
    /// 마지막 시세가 말이 안 된다(3차): 값이 0 이하이거나, 받은 시각이 상태 값을 내보낸 시각
    /// ([`crate::Snapshot`] 참고)보다 뒤다.
    BadLastQuote,
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
    /// 장이 닫혀 있다 (`order-hours-closed`). 달력은 지금을 덮는데 그 시각에 열린 세션이 없다.
    #[error("지금은 주문을 접수할 수 없는 시간입니다")]
    OrderHoursClosed,
    /// 달력이 낡았다: 그 시장의 달력을 넣은 적이 없거나, 넣어 준 달력이 지금을 덮지 않는다
    /// ([`crate::PaperBroker::set_calendar`]의 "덮는다" 참고). 장이 닫힌 것과 구분된다.
    /// 조립하는 쪽이 달력을 날마다 넣지 않으면 이것으로 드러난다.
    #[error("달력이 지금 시각을 덮지 않아 주문을 받을 수 없습니다")]
    CalendarStale,
    /// 주문 금액이 확인 기준 이상인데 확인 표시가 없다 (`confirm-high-value-required`).
    #[error("{threshold} 이상의 주문은 확인이 필요합니다")]
    ConfirmHighValueRequired {
        /// 확인이 필요해지는 금액(이 금액 이상). 명세 오류의 `data.limits.threshold`.
        threshold: Decimal,
    },
    /// 주문 금액이 최대 주문 금액을 넘는다 (`max-order-amount-exceeded`).
    #[error("최대 주문 가능 금액({max_amount})을 초과하였습니다")]
    MaxOrderAmountExceeded {
        /// 최대 주문 금액. 명세 오류의 `data.limits.maxAmount`.
        max_amount: Decimal,
    },
    /// 열린 세션에서 그 호가 유형을 쓸 수 없다 (`order-type-not-allowed`).
    #[error("지금 쓸 수 없는 호가 유형입니다")]
    OrderTypeNotAllowed,
    /// 값이 상 · 하한가를 벗어났다 (`price-out-of-range`).
    #[error("주문 가격이 허용 범위를 벗어났습니다")]
    PriceOutOfRange,
    /// 상 · 하한가가 있는 시장인데 그 종목의 기준가(전일 종가)를 모른다: 넣은 적이 없거나,
    /// 넣은 기준가가 오늘(달력에서 지금이 든 세션의 날) 것이 아니다. 확인할 수 없으니 받지 않는다.
    #[error("기준가를 몰라 상·하한가를 확인할 수 없습니다")]
    ReferencePriceUnknown,
    /// 시장가로 살 때 묶을 돈을 셈할 **늦지 않은** 시세가 없다: 그 종목의 시세를 받은 적이 없거나,
    /// 마지막으로 기억한 시세가 지금 기준으로 늦었다(N6). 늦은 시세는 기억하지도 않는다.
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
    /// 되살릴 수 없는 상태 값이다 (V6).
    #[error("모의계좌 상태 값을 되살릴 수 없습니다: {0:?}")]
    InvalidSnapshot(SnapshotReason),
}
