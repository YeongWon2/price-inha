//! 모의계좌. [`Broker`]의 구현이다.
//!
//! 규칙 번호는 이 문서 어디서나 `N차 규칙 M`(N차 절의 M번째 규칙)으로 적는다.
//!
//! # 5차 규칙 (4차 · 3차 · 2차 · 1차 설명보다 앞선다)
//!
//! 아래 설명과 다르면 이 절이 맞다.
//!
//! 1. **먼 앞날 거래소 시각은 버린다**: 거래소 시각이 받은 시각보다 [`MAX_EXCHANGE_LEAD`](5분)
//!    **넘게** 앞서면 그 시세의 거래소 시각으로 쓰지 않는다. 그 시세는 거래소 시각이 없는 시세와
//!    똑같이 다룬다(체결에 쓰고, 마지막 시세로도 시각 없이 기억한다. 4차 규칙 1 · 2의 기억은
//!    바꾸지 않는다). 그래서 한 번 잘못 온 시각이 그 종목의 체결을 계속 막지 않는다. 딱 5분 앞선
//!    것은 거래소 시각으로 쓴다. 모양 3을 올릴 때도 같다(그런 마지막 시세로는 기억을 채우지 않는다).
//! 2. **모양 3 → 4로 올린 계좌도 같은 시각의 쓴 거래를 다시 쓰지 않는다**(4차 규칙 4).
//! 3. **같은 시각 값 목록의 상한**: 가장 늦은 거래소 시각에 쓴 거래의 값은 종목마다
//!    [`MAX_SAME_TIME_PRICES`](64)개까지 기억한다. 넘으면 가장 오래 기억한 것부터 버린다. 버린
//!    값은 다시 오면 처음 보는 거래가 된다(기억을 끝없이 키우지 않으려는 맞바꿈).
//! 4. **되살릴 때 기억 목록을 검사한다**: `ledger.trade_marks`가 종목 · 통화 순서로 정렬돼 있고
//!    (같은 종목 · 통화가 둘이 아님), 저마다 값이 [`MAX_SAME_TIME_PRICES`]개를 넘지 않고, 기억한
//!    시각이 그 종목 마지막 시세의 거래소 시각 이상이어야 한다. 어긋나면 되살리지 않는다
//!    ([`SnapshotReason::BadTradeMark`]). 마지막 시세에 거래소 시각이 없거나, 거래소 시각이 있는
//!    마지막 시세에 기억이 없는 것은 받아들인다.
//! 5. **시각 없는 시세는 같은 거래를 거듭 쓸 수 있다**: 거래소 시각이 없는 시세는 4차 규칙 2의
//!    기억에 들지 않는다. 마지막 시세와 값이 같을 때만 같은 시세로 보므로(2차 규칙 2), 시각 없는
//!    같은 값이 다른 값을 사이에 두고 다시 오면 다시 체결에 쓴다. 거래소 시각을 주지 않는 시세
//!    출처에서 이것은 피할 수 없어 결정 범위 안으로 두고 동작은 바꾸지 않는다.
//!
//! # 4차 규칙 (3차 · 2차 · 1차 설명보다 앞선다)
//!
//! 아래 3차 · 2차 · 1차 설명과 다르면 이 절이 맞다.
//!
//! 1. **옛 시세는 종목마다 가장 늦은 거래소 시각에 견준다**: 계좌는 종목마다 지금까지 본 가장 늦은
//!    거래소 시각을 따로 기억한다(마지막 시세와 따로다). 거래소 시각이 그보다 **앞**인 시세는
//!    체결도 기억도 하지 않는다. 거래소 시각이 없는 시세는 이 기억을 바꾸지 않으니, 그런 시세가
//!    사이에 끼어도 옛 시세가 막힌다(3차 규칙 2를 바꾼다).
//! 2. **같은 거래소 시각의 거래는 한 번씩만 쓴다**: 가장 늦은 거래소 시각에 이미 쓴 거래(값)를
//!    기억한다. 같은 시각 · 같은 값의 시세가 다시 오면(받은 시각이 달라도, 사이에 다른 값이나 시각
//!    없는 시세가 끼어도) 마지막 시세로 기억만 하고 어떤 주문도 체결하지 않는다. 같은 시각의
//!    서로 다른 값은 저마다 한 번씩 쓴다. 이 기억은 상태 값에 담겨 되살린 뒤에도 남는다.
//! 3. **받은 시각도 주문 뒤여야 한다**: 거래소 시각이 있어도 받은 시각이 주문 접수 시각보다 뒤가
//!    아니면 그 주문의 체결에 쓰지 않는다(거래소 시계가 앞선 경우. 3차 규칙 1을 바꾼다).
//!    거래소 시각이 있으면 그것도 접수 시각보다 뒤여야 한다.
//! 4. **옛 모양의 상태 값을 올려 되살린다**: [`Snapshot`]의 모양 2 · 3은 지금 모양(4)으로 올린 뒤
//!    검사해 되살린다. 2 → 3: 내보낸 시각이 없으니 장부에 적힌 가장 늦은 시각(마지막 시세의 받은
//!    시각, 주문의 접수 · 체결 · 취소 시각, 체결 기록의 시각, 멱등성 키의 접수 시각)으로 채운다.
//!    3 → 4: 종목마다의 가장 늦은 거래소 시각과 쓴 거래를 마지막 시세(거래소 시각이 있는 것)에서
//!    채운다. 올린 뒤 말이 안 되면 되살리지 않는다. 그 밖의 모양 번호는 되살리지 않는다.
//! 5. **정정의 1원**: 같은 값 · 남은 수량 그대로의 정정이 1원 때문에 거부되지 않는 것은 원 미만
//!    **버림**일 때뿐이다(3차 규칙 4의 목표를 좁힌다, `docs/dev/decisions.md` 2026-11-15 (가)).
//!    정정한 새 주문의 수수료는 따로 셈하므로 올림 · 반올림에서는 1원이 더 필요할 수 있다.
//!
//! # 3차 규칙 (2차 · 1차 설명보다 앞선다)
//!
//! 아래 2차 · 1차 설명과 다르면 이 절이 맞다.
//!
//! 1. **주문 뒤의 시세만 체결에 쓴다** (`docs/dev/decisions.md` 2026-11-15): 거래소 시각이 있는
//!    시세는 그 시각이 주문 접수 시각보다 뒤여야 하고, 거래소 시각이 없는 시세는 **받은 시각**이
//!    접수 시각보다 뒤여야 그 주문의 체결에 쓴다. 그 밖의 (늦지 않은) 시세는 체결에 쓰지 않고
//!    기억만 한다(시장가 사기가 묶는 돈을 셈하는 데 쓴다).
//! 2. **옛 시세가 최신을 덮지 않는다**: 거래소 시각이 그 종목의 마지막으로 기억한 시세의 거래소
//!    시각보다 **앞**인 시세는 체결도 기억도 하지 않는다. 그래서 서로 다른 두 시세가 번갈아 와도
//!    새 체결이 생기지 않는다. 거래소 시각이 둘 중 하나에만 있으면 앞뒤를 견주지 않는다.
//! 3. **되살리기 검사를 넓힌다**: [`crate::SnapshotReason`]의 3차 항목들.
//! 4. **수수료 예비를 늘리지 않는다**: 사는 주문이 묶는 수수료 몫은
//!    `올림((누적 체결 금액 + 남은 수량 × 묶는 돈을 셈하는 값) × 수수료율) − 이미 뗀 수수료`다.
//!    체결마다 묶인 돈은 그 체결에 낸 돈(금액 + 수수료)만큼만 줄어든다(지정가 사기). 풀었다가
//!    되가져가는 일이 없어 쓸 수 있는 돈이 체결 때문에 줄지 않는다.
//! 5. **마감 취소 시각은 마감 시각이다**: 마감이 된 주문을 어느 호출이 알아챘든(시간 알림 ·
//!    조회 · 시세 · 주문 내기 · 정정 · 취소) 그 주문의 취소 시각(`canceled_at`)은 그 주문의 마감
//!    시각이다. 알아챈 호출의 `now`가 아니다.
//! 6. **비율에 위쪽 끝이 있다**: 수수료율 · 세율 · 시장가 미끄러짐이 1(100%) 이상이면 계좌를
//!    열지도 되살리지도 않는다([`BrokerError::InvalidConfig`]).
//! 7. **덜어 낸 것은 받았다고 알려야 지운다**: [`PaperBroker::pending_archive`]가 덜어 낼 것을
//!    돌려주고(장부는 그대로), 조립하는 쪽이 저장한 뒤 [`PaperBroker::confirm_archived`]로 알리면
//!    그때 장부에서 지운다. 옛 길 [`PaperBroker::archive_before`]도 남는다(한 번에 떼어 낸다).
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
//!    이어서 큰 주문(2차 규칙 9): `MaxOrderAmountExceeded` / `ConfirmHighValueRequired`.
//! 6. 장 시간: 그 시장의 달력이 `now`를 덮지 않으면 `CalendarStale`(2차 규칙 13), 덮는데 `now`에
//!    열린 세션이 없으면 `OrderHoursClosed`. 세션은 열렸는데 그 세션에서 이 호가 유형만 못 쓰면
//!    `OrderTypeNotAllowed`, 어떤 유형도 못 쓰면 `OrderHoursClosed`.
//! 7. 상 · 하한가(제한폭이 있는 시장의 지정가): 오늘의 기준가를 모르면 `ReferencePriceUnknown`,
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
//! - 시장가 사기: 지정가 자리에 "마지막으로 기억한 시세에 미끄러짐을 더해 호가 단위로 올린 값"을
//!   넣어 같은 식으로 셈한다. 늦지 않은 시세를 기억하고 있지 않으면 `QuoteUnknown`(2차 규칙 4).
//! - 팔기: 남은 수량만큼 보유 수량을 묶는다.
//! - 체결될 때마다 남은 수량으로 다시 셈해 남는 만큼 푼다. 다시 셈한 값만큼 묶을 돈이 없으면
//!   있는 만큼만 묶는다(쓸 수 있는 돈은 음수가 되지 않는다). 시장가 사기가 예상보다 비싸게
//!   체결됐을 때와, 누적해 뗀 수수료가 남은 수량으로 다시 셈한 몫보다 한 단위 앞섰을 때 그렇다.
//!   지정가 주문은 그래도 남은 수량을 다 체결할 돈을 쥐고 있다: 앞으로 뗄 수수료
//!   `맞춤(a + b) − 맞춤(a)`는 어느 방향으로 맞추든 `올림(b)`를 넘지 않는다(a는 지금까지의, b는
//!   남은 수량의 수수료). 주문이 끝나면(전부 체결 · 취소 · 거부 · 정정됨) 묶은 것은 0이 된다.
//!
//! # 체결 ([`PaperBroker::on_quote`])
//!
//! - 늦은 시세([`pi_market::is_stale`], N6), 통화가 그 시장과 다른 시세, 장이 닫힌 때,
//!   지금 세션에서 그 호가 유형을 못 쓰는 때에는 체결하지 않는다. 2차 규칙 1~3 · 13도 본다.
//! - 지정가 사기는 시세 ≤ 지정가, 팔기는 시세 ≥ 지정가일 때. 체결 값은
//!   [`crate::LimitFillPrice`]를 따른다.
//! - 시장가는 접수한 **다음에 온** 시세에 체결한다. 값은 미끄러짐을 불리한 쪽으로 더한 값.
//! - 한 번에 체결되는 수량은 [`crate::MaxFill`]까지. 같은 종목의 주문은 접수한 순서대로 본다.
//! - 시장가 사기의 체결 비용(금액 + 수수료)이 그 주문이 묶은 돈과 쓸 수 있는 돈의 합보다
//!   크면, 살 수 있는 가장 큰 정수 수량만 체결하고 주문을 `Rejected`
//!   ([`crate::CloseReason::InsufficientBuyingPower`])로 끝낸다. 지정가 주문은 이렇게 끝나지 않는다.
//! - 체결마다 수수료(사기 · 팔기)와 세금(팔기)을 주문 단위로 누적해 뗀다(2차 규칙 6). 이번
//!   체결 금액보다 많이 떼지는 않는다. 판 돈(금액 − 수수료 − 세금)은
//!   [`crate::ProceedsUsable`]에 따라 바로 쓰거나 결제일까지 `unsettled`에 둔다.
//!
//! # 미체결 취소 (E12 · E29 · N4)
//!
//! `now`를 받는 모든 호출은 먼저 마감이 된(`now >= 마감`) 열린 주문을 취소하고 이유
//! ([`crate::CloseReason::UnfilledTimeout`])를 남긴다. 체결된 만큼은 남는다.

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use chrono::{DateTime, NaiveDate, Utc};
use pi_market::{Currency, MarketCalendar, Quote, Symbol};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::book::Book;
use crate::ledger::{CashCode, Ledger, MarketCode};
use crate::{
    AccountSnapshot, Archived, BrokerError, CashBalance, CheckAuthority, Fill, OrderEvent,
    PaperConfig, SnapshotReason,
};
use crate::{fill, marks, verify};

/// 멱등성 키가 살아 있는 시간: 10분 (토스 명세의 `clientOrderId` 설명).
/// 접수한 지 딱 이 시간이 되면 같은 키도 새 주문으로 본다.
pub const IDEMPOTENCY_WINDOW: Duration = Duration::from_secs(600);

/// 거래소 시각이 받은 시각보다 앞서도 되는 가장 긴 시간: 5분 (5차 규칙 1).
/// 받은 시각보다 이보다 **넘게** 앞선 거래소 시각은 그 시세의 거래소 시각으로 쓰지 않는다
/// (거래소 시각이 없는 시세와 같이 다룬다). 딱 이만큼 앞선 것은 쓴다.
pub const MAX_EXCHANGE_LEAD: Duration = Duration::from_secs(300);

/// 종목마다 가장 늦은 거래소 시각에 쓴 거래의 값을 기억하는 개수의 상한: 64 (5차 규칙 3).
/// 넘으면 가장 오래 기억한 값부터 버린다.
pub const MAX_SAME_TIME_PRICES: usize = 64;

/// 지금 내보내는 [`Snapshot`]의 모양 번호. 2: 마지막 시세에 거래소 시각 · 받은 시각이 들어갔다.
/// 3: 내보낸 시각(이 계좌가 마지막으로 받은 `now`, `ledger.last_now`)이 들어갔다.
/// 4: 종목마다의 가장 늦은 거래소 시각과 그 시각에 쓴 거래(`ledger.trade_marks`)가 들어갔다.
pub(crate) const SNAPSHOT_VERSION: u32 = 4;

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
///
/// 3차에 더해 바뀌지 않는 이름: `ledger.holdings[]`의 `average_purchase_price`,
/// `ledger.orders[]`의 `filled_quantity` · `filled_amount` · `commission` · `tax` · `currency`
/// (`"Krw"` · `"Usd"`), `ledger.last_quotes`(종목마다 마지막으로 기억한 시세의 배열. 원소에
/// `price`(십진수 글자) · `received_at`(RFC 3339 시각 글자)).
///
/// **내보낸 시각**: 상태 값에는 이 계좌가 마지막으로 받은 `now`(어느 호출이든)가 함께 들어가고,
/// 그것을 내보낸 시각으로 본다. 마지막 시세의 받은 시각이 이보다 뒤인 상태 값은 되살리지 않는다
/// ([`SnapshotReason::BadLastQuote`]).
///
/// 4차에 더한 이름: `ledger.trade_marks`(종목마다 가장 늦은 거래소 시각 `traded_at`과 그 시각에 쓴
/// 거래의 값 `prices`). 모양 2 · 3에는 없고, 읽을 때 채운다(4차 규칙 4).
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
/// | 마감 취소 알림을 받으려면 구독해야 한다. 조회 · 주문 내기 · 정정 · 취소가 먼저 알아채 취소한 것의 알림은 그 호출이 돌려주지 않고 **구독으로만** 온다 | 구독하지 않으면 그렇게 취소된 주문을 알림으로 알 수 없다(조회로만 보인다) |
/// | 모든 호출의 `now`는 **한 시계**에서 와야 한다(뒤로 가지 않게) | 시각이 뒤로 가면 마감 · 늦은 시세 · 멱등성 키의 판단이 어긋난다 |
/// | 덜어 낼 것을 받아([`PaperBroker::pending_archive`]) 저장한 **뒤에** 받았다고 알려야 한다([`PaperBroker::confirm_archived`]) | 알리지 않으면 장부에서 지워지지 않고 쌓인다. 저장 전에 알리면 꺼졌을 때 기록이 사라진다 |
///
/// 시간이 흘렀다고 따로 알려 줄 의무는 없다: 모든 호출이 먼저 `now`까지 마감이 된 주문을 취소한다.
/// 다만 알림을 제때 받으려면 [`PaperBroker::on_time`]을 주기적으로 부른다.
#[derive(Debug)]
pub struct PaperBroker {
    /// 설정. 열거나 되살릴 때 정해지고 바뀌지 않는다.
    pub(crate) config: PaperConfig,
    /// 이 계좌의 표식. 짝인 [`CheckAuthority`]가 낸 표시만 이것과 같은 것을 가리킨다.
    pub(crate) seal: Arc<()>,
    /// 상태. 모든 동작이 이 잠금 안에서 한 번에 끝난다(잠근 채로 기다리지 않는다).
    book: Mutex<Book>,
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
    pub(crate) fn book(&self) -> MutexGuard<'_, Book> {
        self.book.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// 상태를 잠그고 먼저 `now`까지 마감이 된 주문을 취소한다(지난 멱등성 키도 버린다).
    /// 조회는 모두 이것을 거친다. 생긴 이벤트는 구독자에게 간다.
    pub(crate) fn settled(&self, now: DateTime<Utc>) -> MutexGuard<'_, Book> {
        let mut book = self.book();
        book.advance(now, &mut Vec::new());
        book
    }

    /// 처음 돈으로 새 계좌를 연다. 함께 나오는 [`CheckAuthority`]만이 이 계좌가 받는 점검 통과
    /// 표시를 낼 수 있다. 처음 돈이나 비율이 음수면 [`BrokerError::InvalidConfig`].
    /// 수수료율 · 세율 · 시장가 미끄러짐이 1(100%) 이상이어도 같다(3차 규칙 6. 되살리기도 같다).
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
    /// 0 이하, 열린 사는 주문이 남은 수량 몫보다 많이 묶음, 뗀 수수료 · 세금 · 누적 체결 금액이
    /// 말이 안 됨, 체결 수량이 정수가 아님, 상태와 체결 수량이 어긋남, 주문 통화가 시장과 다름,
    /// 평균 산 값이 0 이하, 마지막 시세의 값이 0 이하이거나 받은 시각이 내보낸 시각보다 뒤 — 모두
    /// 거부한다(3차 규칙 3). 종목마다 기억한 거래의 값이 0 이하인 것도 같다
    /// ([`SnapshotReason::BadLastQuote`]). 종목마다의 기억 목록이 정렬되지 않음 · 같은 종목 ·
    /// 통화가 둘 · 값이 [`MAX_SAME_TIME_PRICES`]개 넘음 · 기억한 시각이 마지막 시세의 거래소
    /// 시각보다 앞인 것도 거부한다([`SnapshotReason::BadTradeMark`], 5차 규칙 4). 모양 2 · 3의
    /// 상태 값은 지금 모양으로 올린 뒤 검사한다(4차 규칙 4).
    pub fn restore(
        config: PaperConfig,
        snapshot: Snapshot,
    ) -> Result<(Self, CheckAuthority), BrokerError> {
        config.validate_rules()?;
        let Some(ledger) = marks::upgrade(snapshot.version, snapshot.ledger) else {
            return Err(BrokerError::InvalidSnapshot(
                SnapshotReason::UnknownVersion(snapshot.version),
            ));
        };
        verify::check_ledger(&ledger, &config)?;
        Ok(Self::assemble(config, ledger))
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
    ///
    /// 종목마다 최근 세 날짜의 기준가만 기억한다(다음 영업일 것을 미리 넣어도 오늘 것이 남는다).
    pub fn set_reference_price(&self, symbol: Symbol, price: Decimal, date: NaiveDate) {
        self.book().set_reference(symbol.as_str(), price, date);
    }

    /// 시세 하나를 준다. 먼저 마감이 된 주문을 취소하고, 그 종목의 열린 주문을 체결해 본다.
    /// 이 호출로 생긴 이벤트를 일어난 순서대로 돌려준다(구독자에게도 같은 것이 간다).
    ///
    /// 체결하지 않고 기억하지도 않는 시세: 늦은 시세(N6), 받은 시각이 `now`보다 뒤인 시세,
    /// 현금을 두지 않는 통화의 시세, 0 이하의 값. 그 밖의 시세는 체결하지 않더라도 그 종목의
    /// 마지막 시세로 기억한다. 마지막으로 처리한 시세와 같은 시세(거래소 시각 · 값이 같음)가 다시
    /// 오면 체결하지 않는다. 달력이 `now`를 덮지 않으면 체결하지 않는다(모듈 문서의 2차 규칙 1~4 · 13).
    ///
    /// 이미 처리한 시세가 다시 오면 기억한 시세의 받은 시각만 더 늦은 쪽으로 고친다(같은 값을 방금
    /// 다시 받았으니, 시장가 사기가 묶는 돈을 셈할 때 그만큼 새것으로 본다).
    ///
    /// 3차: 거래소 시각이 마지막으로 기억한 시세의 거래소 시각보다 앞인 시세도 체결 · 기억하지
    /// 않는다(3차 규칙 2). 어느 주문의 체결에 쓰는지는 3차 규칙 1을 따른다.
    pub fn on_quote(&self, quote: &Quote, now: DateTime<Utc>) -> Vec<OrderEvent> {
        fill::on_quote(&mut self.book(), &self.config, quote, now)
    }

    /// 시각만 알려 준다. 마감이 된 열린 주문을 취소한다. 생긴 이벤트를 돌려준다.
    pub fn on_time(&self, now: DateTime<Utc>) -> Vec<OrderEvent> {
        let mut events = Vec::new();
        self.book().advance(now, &mut events);
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
        self.book().archive(before)
    }

    /// 덜어 낼 것을 돌려준다. **장부에서는 아직 지우지 않는다** (3차 규칙 7, N1).
    ///
    /// 고르는 것과 순서는 [`PaperBroker::archive_before`]와 같다. 계좌 · 조회 · 상태 값은 그대로이고,
    /// 다시 부르면 (그사이 바뀐 것이 없으면) 같은 것을 돌려준다. 조립하는 쪽이 이것을 저장한 뒤
    /// [`PaperBroker::confirm_archived`]로 알리면 그때 지운다. 저장하기 전에 꺼져도 상태 값에
    /// 남아 있어 기록이 사라지지 않는다.
    pub fn pending_archive(&self, before: DateTime<Utc>) -> Archived {
        self.book().pending_archive(before)
    }

    /// 저장을 마친 것을 알린다: `archived`에 든 끝난 주문(식별자로)과 체결 기록(같은 값)을 장부에서
    /// 지운다 (3차 규칙 7). 그 밖의 것은 건드리지 않는다. 이미 지운 것이나 장부에 없는 것은 넘어간다
    /// (두 번 알려도 같다). 지운 뒤의 계좌는 같은 것을 [`PaperBroker::archive_before`]로 떼어 낸
    /// 계좌와 같다.
    pub fn confirm_archived(&self, archived: &Archived) {
        self.book().confirm_archived(archived);
    }

    /// 그 통화의 현금. 다루지 않는 통화는 모두 0이다. 먼저 `now`까지 마감이 된 주문을 취소한다.
    pub fn cash(&self, currency: &Currency, now: DateTime<Utc>) -> CashBalance {
        let book = self.settled(now);
        match CashCode::of(currency) {
            Some(code) => book.ledger.balance(code),
            None => CashBalance {
                available: Decimal::ZERO,
                held: Decimal::ZERO,
                unsettled: Decimal::ZERO,
            },
        }
    }

    /// 보유 전체와 통화별 현금을 **같은 잠금 안에서** 한 번에 읽는다 (9차 1). 먼저 `now`까지
    /// 마감이 된 주문을 취소한다([`PaperBroker::cash`]와 같다). 체결 · 주문 · 정산과 같은 잠금
    /// 하나만 잡으므로 교착이 없고, 그 사이에 체결이 끼어 섞인 장부가 나오지 않는다.
    /// 기존 [`crate::Broker::holdings`] · [`PaperBroker::cash`]는 그대로다.
    pub fn account_snapshot(&self, now: DateTime<Utc>) -> AccountSnapshot {
        let book = self.settled(now);
        AccountSnapshot {
            holdings: book.ledger.holding_views(None),
            krw: book.ledger.balance(CashCode::Krw),
            usd: book.ledger.balance(CashCode::Usd),
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
