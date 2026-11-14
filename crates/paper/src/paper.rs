//! 모의계좌. [`Broker`]의 구현이다.
//!
//! # 2차 규칙 (1차 설명보다 앞선다)
//!
//! 아래 "주문을 받을 때의 확인" · "묶는 돈과 수량" · "체결" · "미체결 취소"는 1차의 설명이다.
//! 2차에서 다음이 더해지거나 바뀐다. 서로 다르면 이 절이 맞다.
//!
//! **유리하게 치우친 체결을 막는다**
//!
//! 1. 주문 뒤에 온 시세로만 체결한다: 시세의 받은 시각(`received_at`)이 주문 접수 시각보다
//!    **뒤**여야 하고, 거래소 시각(`exchange_time`)이 있으면 그것도 접수 시각보다 뒤여야 한다
//!    (앞이거나 같으면 그 주문은 그 시세로 체결하지 않는다). 지정가 · 시장가 모두 같다.
//! 2. 같은 시세를 두 번 쓰지 않는다: 그 종목에서 마지막으로 처리한 시세와 거래소 시각 · 값이 같은
//!    시세가 다시 오면 어떤 주문도 체결하지 않는다(받은 시각이 달라도 같다). 거래소 시각이 없는
//!    시세끼리는 값이 같으면 같은 시세로 본다(보수적). 마지막으로 처리한 시세는 상태 값에 담긴다.
//! 3. 받은 시각이 `now`보다 뒤인 시세로는 체결하지 않는다.
//! 4. 늦은 시세(N6)는 기억하지도 않는다. 시장가 사기가 묶는 돈은 기억한 시세 가운데 주문을 받는
//!    `now` 기준으로 늦지 않은(`now − received_at <= stale.max_age`) 것으로만 셈한다. 그런 시세가
//!    없으면 [`BrokerError::QuoteUnknown`].
//! 5. 확인되지 않은 숫자에는 기본값이 없다([`crate::UnverifiedRules`]).
//!
//! **실제와 다르게 끝나는 흐름을 없앤다**
//!
//! 6. 수수료 · 세금은 주문 단위로 누적해 뗀다: 체결마다 `맞춤(누적 체결 금액 × 비율) − 이미 뗀 것`.
//!    여러 번에 나눠 체결돼도 뗀 합은 한 번에 체결됐을 때와 같고, 사는 주문은 접수 때 묶은 돈을
//!    넘지 않는다. 받은 **지정가** 주문은 돈이 모자라다는 이유(`Rejected`)로 끝나지 않는다.
//! 7. 주문 내기 · 정정 · 취소 · 조회([`Broker`]의 모든 메서드와 [`PaperBroker::cash`])도 먼저
//!    `now`까지 마감이 된 주문을 취소한다([`PaperBroker::on_time`]과 같은 처리. 이벤트도 나간다).
//! 8. 주문은 장 마감을 넘기지 않는다: 마감은 접수한 세션의 끝보다 늦지 않다
//!    ([`crate::UnfilledTimeout`]).
//! 9. 큰 주문: 확인 5(모양) 다음에 주문 금액을 본다. 최대 주문 금액을 넘으면
//!    [`BrokerError::MaxOrderAmountExceeded`](확인 표시와 상관없이), 아니고 확인 기준 이상인데
//!    확인 표시가 없으면 [`BrokerError::ConfirmHighValueRequired`]. 정정도 같다.
//!
//! **되살리기와 자원**
//!
//! 10. 말이 안 되는 상태 값은 되살리지 않는다([`crate::SnapshotReason`]).
//! 11. 끝난 것은 덜어 낼 수 있다([`PaperBroker::archive_before`]). 지난 멱등성 키는 스스로 지운다.
//! 12. 체결 알림은 끝없이 쌓이지 않는다([`Broker::subscribe`], [`PaperConfig::event_buffer`]).
//!
//! **조립하는 쪽의 의무를 드러낸다**
//!
//! 13. 달력이 `now`를 덮지 않으면([`PaperBroker::set_calendar`]) 주문을 받지 않고
//!     ([`BrokerError::CalendarStale`]) 체결도 하지 않는다. 달력을 넣은 적이 없는 것도 같다
//!     (1차의 `OrderHoursClosed`가 아니다). 기준가는 넣은 날짜와 함께 기억하고, 오늘 것이 아니면
//!     [`BrokerError::ReferencePriceUnknown`]이다.
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

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use pi_market::{Currency, MarketCalendar, Quote, Symbol};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::Receiver;

use crate::book::Book;
use crate::ledger::{CashCode, Ledger, MarketCode};
use crate::{
    AccountKind, Archived, Broker, BrokerError, CashBalance, CheckAuthority, Commission, Fill,
    Holding, ModifyRequest, Order, OrderAck, OrderEvent, OrderGroup, OrderId, OrderRequest,
    PaperConfig, SnapshotReason,
};
use crate::{fill, orders};

/// 멱등성 키가 살아 있는 시간: 10분 (토스 명세의 `clientOrderId` 설명).
/// 접수한 지 딱 이 시간이 되면 같은 키도 새 주문으로 본다.
pub const IDEMPOTENCY_WINDOW: Duration = Duration::from_secs(600);

/// 지금 내보내는 [`Snapshot`]의 모양 번호.
const SNAPSHOT_VERSION: u32 = 1;

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
///
/// 글자(JSON)로 바꾼 모양에서 다음 이름과 뜻은 바뀌지 않는다(되살리기 검사의 테스트가 이
/// 이름으로 값을 고쳐 본다): `ledger.next_id`(다음 식별자 번호, 수), `ledger.holdings`(보유의
/// 배열. 원소에 `quantity`), `ledger.orders`(접수한 순서의 배열. 원소에 `status` · `quantity` ·
/// `held_cash`). 수량과 돈은 십진수 글자다.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    /// 값의 모양 번호.
    version: u32,
    /// 장부 전체.
    ledger: Ledger,
}

/// 모의계좌.
///
/// # 조립하는 쪽이 해야 하는 일
///
/// 이 계좌는 시계 · 시세 · 달력 · 저장소를 스스로 갖지 않는다. 조립하는 쪽이 다음을 **해야 한다.**
/// 빠뜨리면 조용히 돌지 않고 아래처럼 드러난다.
///
/// | 해야 한다 | 빠뜨리면 |
/// |---|---|
/// | 시장마다 달력을 **날마다** 넣어야 한다([`PaperBroker::set_calendar`]). 판 돈의 결제도 이때 본다 | 달력이 지금을 덮지 않게 되면 주문이 [`BrokerError::CalendarStale`]로 거부되고 체결이 멈춘다. 결제 전인 돈(`unsettled`)이 풀리지 않는다 |
/// | 국내 종목의 기준가를 **영업일마다** 그 날짜와 함께 넣어야 한다([`PaperBroker::set_reference_price`]) | 그 종목의 지정가 주문이 [`BrokerError::ReferencePriceUnknown`]으로 거부된다 |
/// | 시세를 받을 때마다 넘겨야 한다([`PaperBroker::on_quote`]). 받은 시각과 거래소 시각을 그대로 둔다 | 주문이 체결되지 않고 마감에 취소된다. 늦지 않은 시세가 없으면 시장가 사기가 [`BrokerError::QuoteUnknown`]으로 거부된다 |
/// | 끝난 것을 때때로 덜어 내 저장소로 옮겨야 한다([`PaperBroker::archive_before`]) (N1) | 끝난 주문과 체결 기록이 계좌와 상태 값([`Snapshot`])에 계속 쌓여 커진다 |
/// | 상태 값을 저장하고([`PaperBroker::snapshot`]) 다시 켤 때 되살려야 한다([`PaperBroker::restore`]). 되살린 뒤 달력과 기준가를 다시 넣어야 한다 | 껐다 켜면 계좌가 처음 돈으로 돌아간다. 달력을 넣기 전의 주문은 `CalendarStale`로 거부된다 |
/// | 체결 알림을 구독했으면 읽어야 한다([`Broker::subscribe`]) | [`PaperConfig::event_buffer`]개가 차면 그 구독이 끊긴다(닫힌 것으로 드러난다) |
///
/// 시간이 흘렀다고 따로 알려 줄 의무는 없다: 모든 호출이 먼저 `now`까지 마감이 된 주문을 취소한다.
/// 다만 알림을 제때 받으려면 [`PaperBroker::on_time`]을 주기적으로 부른다.
#[derive(Debug)]
pub struct PaperBroker {
    /// 설정. 열거나 되살릴 때 정해지고 바뀌지 않는다.
    config: PaperConfig,
    /// 이 계좌의 표식. 짝인 [`CheckAuthority`]가 낸 표시만 이것과 같은 것을 가리킨다.
    seal: Arc<()>,
    /// 상태. 모든 동작이 이 잠금 안에서 한 번에 끝난다(잠근 채로 기다리지 않는다).
    book: Mutex<Book>,
}

/// 되살릴 장부가 말이 되는가(음수인 돈 · 수량, 겹치는 식별자가 없는가).
/// 2차에서 더할 검사는 [`SnapshotReason`]에 적혀 있다.
fn check_ledger(ledger: &Ledger) -> Result<(), BrokerError> {
    let bad = |reason: SnapshotReason| Err(BrokerError::InvalidSnapshot(reason));
    let negative = |value: &Decimal| value.is_sign_negative() && !value.is_zero();
    for cash in [&ledger.krw, &ledger.usd] {
        if negative(&cash.available) || cash.unsettled.iter().any(|lot| negative(&lot.amount)) {
            return bad(SnapshotReason::NegativeCash);
        }
    }
    if ledger
        .holdings
        .iter()
        .any(|holding| holding.quantity <= Decimal::ZERO)
    {
        return bad(SnapshotReason::NonPositiveHolding);
    }
    for (index, order) in ledger.orders.iter().enumerate() {
        if negative(&order.held_cash)
            || negative(&order.filled_quantity)
            || order.filled_quantity > order.quantity
        {
            return bad(SnapshotReason::BadFilledQuantity);
        }
        let duplicated = ledger
            .orders
            .iter()
            .skip(index + 1)
            .any(|other| other.order_id == order.order_id);
        if duplicated {
            return bad(SnapshotReason::DuplicateOrderId);
        }
    }
    Ok(())
}

impl PaperBroker {
    fn assemble(config: PaperConfig, ledger: Ledger) -> (Self, CheckAuthority) {
        let seal = Arc::new(());
        let authority = CheckAuthority::new(Arc::clone(&seal));
        let broker = Self {
            config,
            seal,
            book: Mutex::new(Book::new(ledger)),
        };
        (broker, authority)
    }

    /// 상태를 잠근다. 잠근 채로 죽은 호출이 있었어도 상태는 그대로 쓴다.
    fn book(&self) -> MutexGuard<'_, Book> {
        self.book.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// 처음 돈으로 새 계좌를 연다. 함께 나오는 [`CheckAuthority`]만이 이 계좌가 받는 점검 통과
    /// 표시를 낼 수 있다. 처음 돈이나 비율이 음수면 [`BrokerError::InvalidConfig`].
    pub fn open(config: PaperConfig) -> Result<(Self, CheckAuthority), BrokerError> {
        config.validate_initial_cash()?;
        config.validate_rules()?;
        let ledger = Ledger::new(config.initial_cash.krw, config.initial_cash.usd);
        Ok(Self::assemble(config, ledger))
    }

    /// 내보낸 값에서 계좌를 되살린다. `config.initial_cash`는 쓰지 않는다. 권한은 새로 나온다
    /// (되살리기 전의 표시는 쓸 수 없다). 되살린 뒤 달력과 기준가를 다시 넣고 같은 시세 · 시각을
    /// 주면 끊기지 않은 계좌와 같은 결과가 나온다. 식별자도 이어서 매긴다.
    ///
    /// 말이 안 되는 상태 값은 되살리지 않는다 (V6): [`BrokerError::InvalidSnapshot`]에
    /// [`SnapshotReason`]을 담아 돌려준다. 다음 식별자가 이미 있는 식별자보다 크지 않음, 끝난
    /// 주문에 묶인 돈이 남음, 열린 파는 주문의 합이 보유를 넘음, 같은 종목의 보유가 둘, 주문 수량이
    /// 0 이하, 열린 사는 주문이 그 주문에 필요한 돈보다 많이 묶음 — 모두 거부한다.
    pub fn restore(
        config: PaperConfig,
        snapshot: Snapshot,
    ) -> Result<(Self, CheckAuthority), BrokerError> {
        config.validate_rules()?;
        if snapshot.version != SNAPSHOT_VERSION {
            return Err(BrokerError::InvalidSnapshot(
                SnapshotReason::UnknownVersion(snapshot.version),
            ));
        }
        check_ledger(&snapshot.ledger)?;
        Ok(Self::assemble(config, snapshot.ledger))
    }

    /// 계좌 전체를 값 하나로 내보낸다. 같은 상태는 같은 값(`==`)이다.
    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            version: SNAPSHOT_VERSION,
            ledger: self.book().ledger.clone(),
        }
    }

    /// 그 시장(`calendar.market`)의 달력을 넣는다. 다시 넣으면 바꾼다. 날마다 넣어야 한다.
    ///
    /// 달력이 시각 `t`를 **덮는다**: `이전 영업일의 첫 세션 시작 <= t < 다음 영업일의 마지막 세션 끝`.
    /// 덮지 않는 시각에는 주문을 받지 않고([`BrokerError::CalendarStale`]) 체결도 하지 않는다.
    /// 덮는데 그 시각에 열린 세션이 없으면 장이 닫힌 것이다([`BrokerError::OrderHoursClosed`]).
    ///
    /// 결제도 여기서 본다: 결제 전인 돈 가운데 결제 예정일이 이 달력의 기준일
    /// (`calendar.today.date`) **이하**인 것은 쓸 수 있는 돈으로 옮긴다.
    pub fn set_calendar(&self, calendar: MarketCalendar) {
        let Some(market) = MarketCode::of(calendar.market) else {
            return;
        };
        let mut book = self.book();
        book.settle(market, calendar.today.date);
        match market {
            MarketCode::Kr => book.kr_calendar = Some(calendar),
            MarketCode::Us => book.us_calendar = Some(calendar),
        }
    }

    /// 종목의 기준가(전일 종가)를 넣는다. 상 · 하한가를 셈하는 데 쓴다. 다시 넣으면 바꾼다.
    ///
    /// `date`는 이 기준가를 쓰는 영업일이다. 주문을 받을 때 달력에서 `now`가 든 세션의 날
    /// ([`pi_market::TradingDay::date`])과 **같은 날짜의 기준가만** 쓴다. 다른 날짜의 것뿐이면
    /// 기준가를 모르는 것이다([`BrokerError::ReferencePriceUnknown`]).
    pub fn set_reference_price(&self, symbol: Symbol, price: Decimal, date: NaiveDate) {
        // 2차 뼈대: 날짜를 받아 두기만 한다.
        let _ = date;
        self.book()
            .reference
            .insert(symbol.as_str().to_owned(), price);
    }

    /// 시세 하나를 준다. 먼저 마감이 된 주문을 취소하고, 그 종목의 열린 주문을 체결해 본다.
    /// 이 호출로 생긴 이벤트를 일어난 순서대로 돌려준다(구독자에게도 같은 것이 간다).
    ///
    /// 체결하지 않고 기억하지도 않는 시세: 늦은 시세(N6), 받은 시각이 `now`보다 뒤인 시세,
    /// 현금을 두지 않는 통화의 시세, 0 이하의 값. 그 밖의 시세는 체결하지 않더라도 그 종목의
    /// 마지막 시세로 기억한다. 마지막으로 처리한 시세와 같은 시세(거래소 시각 · 값이 같음)가 다시
    /// 오면 체결하지 않는다. 달력이 `now`를 덮지 않으면 체결하지 않는다(모듈 문서의 2차 규칙 1~4 · 13).
    pub fn on_quote(&self, quote: &Quote, now: DateTime<Utc>) -> Vec<OrderEvent> {
        fill::on_quote(&mut self.book(), &self.config, quote, now)
    }

    /// 시각만 알려 준다. 마감이 된 열린 주문을 취소한다. 생긴 이벤트를 돌려준다.
    pub fn on_time(&self, now: DateTime<Utc>) -> Vec<OrderEvent> {
        let mut events = Vec::new();
        self.book().expire(now, &mut events);
        events
    }

    /// 장애 흉내를 켜거나(`Some`) 끈다(`None`).
    pub fn set_fault(&self, fault: Option<Fault>) {
        self.book().fault = fault;
    }

    /// 계좌에서 끝난 것을 덜어 내 돌려준다 (N1). 조립하는 쪽이 저장소로 옮긴다.
    ///
    /// 덜어 내는 것(오래된 것부터, 접수한 · 일어난 순서대로 돌려준다):
    /// - 끝난 주문(전부 체결 · 취소 · 거부 · 정정됨) 가운데 접수 시각이 `before`보다 **앞**인 것.
    /// - 체결 기록 가운데 체결 시각이 `before`보다 앞인 것.
    ///
    /// 열린 주문은 건드리지 않는다. 덜어 낸 주문은 조회에 나오지 않고(상세는
    /// [`BrokerError::OrderNotFound`]) 상태 값에도 없다. 현금 · 보유 · 열린 주문 · 다음 식별자는
    /// 그대로이고, 덜어 낸 뒤 같은 입력을 주면 덜어 내지 않은 계좌와 같은 결과가 나온다
    /// (아직 살아 있는 멱등성 키는 덜어 낸 주문을 가리켜도 앞의 결과를 그대로 돌려준다).
    ///
    /// 지난 멱등성 키([`IDEMPOTENCY_WINDOW`]가 지난 것)는 이 호출과 상관없이 **스스로** 지운다:
    /// `now`를 받는 모든 호출이 그때 지난 키를 버린다.
    pub fn archive_before(&self, before: DateTime<Utc>) -> Archived {
        todo!("2차 11번: 끝난 것을 덜어 낸다 (before = {before})")
    }

    /// 그 통화의 현금. 다루지 않는 통화는 모두 0이다. 먼저 `now`까지 마감이 된 주문을 취소한다.
    pub fn cash(&self, currency: &Currency, now: DateTime<Utc>) -> CashBalance {
        // 2차 뼈대: 시각을 받아 두기만 한다.
        let _ = now;
        match CashCode::of(currency) {
            Some(code) => self.book().ledger.balance(code),
            None => CashBalance {
                available: Decimal::ZERO,
                held: Decimal::ZERO,
                unsettled: Decimal::ZERO,
            },
        }
    }

    /// 체결 기록 전부(일어난 순서대로).
    pub fn fills(&self) -> Vec<Fill> {
        self.book()
            .ledger
            .fills
            .iter()
            .map(|fill| fill.view())
            .collect()
    }
}

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
        // 2차 뼈대: 시각을 받아 두기만 한다.
        let _ = now;
        let open = group == OrderGroup::Open;
        Ok(self
            .book()
            .ledger
            .orders
            .iter()
            .filter(|order| order.is_open() == open)
            .filter(|order| symbol.is_none_or(|symbol| symbol.as_str() == order.symbol))
            .map(|order| order.view())
            .collect())
    }

    async fn order(&self, order_id: &OrderId, now: DateTime<Utc>) -> Result<Order, BrokerError> {
        // 2차 뼈대: 시각을 받아 두기만 한다.
        let _ = now;
        let book = self.book();
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
        // 2차 뼈대: 시각을 받아 두기만 한다.
        let _ = now;
        let book = self.book();
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
        // 2차 뼈대: 시각을 받아 두기만 한다.
        let _ = now;
        Ok(self.book().ledger.holding_views(symbol))
    }

    fn subscribe(&self) -> Receiver<OrderEvent> {
        todo!("2차 12번: 정한 개수(PaperConfig::event_buffer)까지만 쥐는 구독")
    }
}
