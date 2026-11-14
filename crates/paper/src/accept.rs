//! 주문을 받을 때의 확인. 접수와 정정이 같은 확인을 거친다. 순서는 [`crate::paper`]의 모듈
//! 문서에 적힌 그대로다. 여기서는 아무것도 바꾸지 않는다(받을지 말지와 묶을 것만 셈한다).

use std::sync::Arc;

use chrono::{DateTime, Utc};
use pi_market::{Market, Session, Symbol};
use rust_decimal::Decimal;

use chrono::NaiveDate;

use crate::book::Book;
use crate::calendar;
use crate::fill::too_old;
use crate::ledger::{CashCode, MarketCode, OrderRecord};
use crate::money;
use crate::{
    AccountKind, ApprovedOrder, BrokerError, CheckPermit, InvalidReason, MarketRules, OrderType,
    PaperConfig, Side,
};

/// 확인할 주문 내용.
#[derive(Debug, Clone)]
pub(crate) struct Draft {
    pub market: Market,
    pub symbol: Symbol,
    pub side: Side,
    pub order_type: OrderType,
    pub quantity: Decimal,
    pub price: Option<Decimal>,
    /// 큰 주문 확인 표시.
    pub confirm_high_value: bool,
}

/// 확인을 통과한 주문에 대해 셈한 것.
#[derive(Debug)]
pub(crate) struct Checked {
    pub market: MarketCode,
    pub currency: CashCode,
    /// 사는 주문이 묶는 돈을 셈하는 값.
    pub hold_price: Option<Decimal>,
    /// 사는 주문이 묶을 돈. 파는 주문은 0이다.
    pub hold: Decimal,
    /// 접수한 세션.
    pub session: Session,
}

/// 점검 통과 표시 (E33): 있는가, 이 계좌의 것인가, 승인한 내용이 주문 내용과 같은가.
pub(crate) fn check_permit(
    permit: Option<&CheckPermit>,
    seal: &Arc<()>,
    draft: &Draft,
) -> Result<(), BrokerError> {
    let permit = permit.ok_or(BrokerError::CheckRequired)?;
    if !permit.sealed_by(seal) {
        return Err(BrokerError::CheckInvalid);
    }
    let ordered = ApprovedOrder {
        market: draft.market,
        symbol: draft.symbol.clone(),
        side: draft.side,
        order_type: draft.order_type,
        quantity: draft.quantity,
        price: draft.price,
        account_kind: AccountKind::Paper,
    };
    if permit.approved() != &ordered {
        return Err(BrokerError::CheckMismatch);
    }
    Ok(())
}

/// 모양: 수량 · 값 · 호가 단위.
fn check_shape(draft: &Draft, rules: &MarketRules) -> Result<(), BrokerError> {
    let invalid = |reason| Err(BrokerError::InvalidRequest(reason));
    if draft.quantity <= Decimal::ZERO {
        return invalid(InvalidReason::NonPositiveQuantity);
    }
    if !draft.quantity.fract().is_zero() {
        return invalid(InvalidReason::FractionalQuantity);
    }
    match (draft.order_type, draft.price) {
        (OrderType::Limit, None) => invalid(InvalidReason::LimitPriceRequired),
        (OrderType::Market, Some(_)) => invalid(InvalidReason::MarketPriceNotAllowed),
        (OrderType::Market, None) => Ok(()),
        (OrderType::Limit, Some(price)) if price <= Decimal::ZERO => {
            invalid(InvalidReason::NonPositivePrice)
        }
        (OrderType::Limit, Some(price)) => {
            let tick = rules.tick_for(price);
            if money::on_tick(price, tick) {
                Ok(())
            } else {
                invalid(InvalidReason::TickSize { tick })
            }
        }
    }
}

/// 시장가로 살 때 묶는 돈을 셈하는 값: 기억한 시세에 미끄러짐을 더해 호가 단위로 올린 값.
/// 기억한 시세가 없거나 `now` 기준으로 늦었으면(N6) 시세를 모르는 것이다.
fn market_hold_price(
    book: &Book,
    config: &PaperConfig,
    draft: &Draft,
    rules: &MarketRules,
    currency: CashCode,
    now: DateTime<Utc>,
) -> Result<Decimal, BrokerError> {
    let quote = book
        .ledger
        .last_quote(draft.symbol.as_str(), currency)
        .filter(|quote| !too_old(quote.received_at, now, &config.stale))
        .ok_or(BrokerError::QuoteUnknown)?;
    // 셈이 넘칠 만큼 큰 값으로는 어떤 돈으로도 살 수 없다.
    money::market_price(quote.price, Side::Buy, rules).ok_or(BrokerError::InsufficientBuyingPower)
}

/// 큰 주문: 주문 금액이 최대 주문 금액을 넘으면 확인 표시와 상관없이 받지 않고, 아니고 확인
/// 기준 이상인데 확인 표시가 없으면 받지 않는다. `value`는 주문 금액을 셈하는 값이다(지정가,
/// 또는 시장가 사기가 묶는 돈을 셈하는 값). 값이 없으면(시장가 팔기, 시세를 모르는 시장가 사기)
/// 보지 않는다.
fn check_amount(
    draft: &Draft,
    rules: &MarketRules,
    value: Option<Decimal>,
) -> Result<(), BrokerError> {
    let Some(value) = value else {
        return Ok(());
    };
    // 셈이 넘칠 만큼 큰 금액은 어떤 기준보다도 크다.
    let amount = draft.quantity.checked_mul(value);
    if let Some(max_amount) = rules.max_order_amount
        && amount.is_none_or(|amount| amount > max_amount)
    {
        return Err(BrokerError::MaxOrderAmountExceeded { max_amount });
    }
    if let Some(threshold) = rules.high_value_confirm_from
        && amount.is_none_or(|amount| amount >= threshold)
        && !draft.confirm_high_value
    {
        return Err(BrokerError::ConfirmHighValueRequired { threshold });
    }
    Ok(())
}

/// 장 시간: 달력이 `now`를 덮고, `now`에 열린 세션이 있고, 그 세션에서 이 호가 유형을 받는가.
/// 접수하는 세션과 그 세션의 날(오늘)을 돌려준다.
fn check_hours(
    book: &Book,
    market: MarketCode,
    rules: &MarketRules,
    order_type: OrderType,
    now: DateTime<Utc>,
) -> Result<(Session, NaiveDate), BrokerError> {
    // 달력을 넣은 적이 없거나 넣은 달력이 지금을 덮지 않으면 장이 닫힌 것과 구분한다.
    let calendar = book
        .covering(market, now)
        .ok_or(BrokerError::CalendarStale)?;
    let session = calendar
        .session_at(now)
        .ok_or(BrokerError::OrderHoursClosed)?;
    let today = calendar::trade_date(calendar, now).ok_or(BrokerError::OrderHoursClosed)?;
    if rules.sessions.allows(session.kind, order_type) {
        return Ok((session, today));
    }
    let any = rules.sessions.allows(session.kind, OrderType::Limit)
        || rules.sessions.allows(session.kind, OrderType::Market);
    Err(if any {
        BrokerError::OrderTypeNotAllowed
    } else {
        BrokerError::OrderHoursClosed
    })
}

/// 상 · 하한가: 제한폭이 있는 시장의 지정가만 본다. 상 · 하한가와 딱 같은 값은 받는다.
/// 기준가는 오늘(`today`) 것만 쓴다. 다른 날짜의 것뿐이면 기준가를 모르는 것이다.
fn check_price_band(
    book: &Book,
    draft: &Draft,
    rules: &MarketRules,
    today: NaiveDate,
) -> Result<(), BrokerError> {
    let (Some(price), Some(rate)) = (draft.price, rules.price_limit_rate) else {
        return Ok(());
    };
    let reference = book
        .reference_on(draft.symbol.as_str(), today)
        .ok_or(BrokerError::ReferencePriceUnknown)?;
    let (lower, upper) =
        money::price_band(reference, rate, rules).ok_or(BrokerError::PriceOutOfRange)?;
    if price < lower || price > upper {
        return Err(BrokerError::PriceOutOfRange);
    }
    Ok(())
}

/// 사는 주문이 묶을 돈. 쓸 수 있는 돈(`replacing`이 묶은 것은 푼 것으로 본다)을 넘으면 받지 않는다.
fn check_buying_power(
    book: &Book,
    draft: &Draft,
    rules: &MarketRules,
    currency: CashCode,
    hold_price: Decimal,
    replacing: Option<&OrderRecord>,
) -> Result<Decimal, BrokerError> {
    // 셈이 넘칠 만큼 큰 주문은 어떤 돈으로도 살 수 없다.
    let hold = money::buy_hold(draft.quantity, hold_price, rules)
        .ok_or(BrokerError::InsufficientBuyingPower)?;
    let freed = replacing.map_or(Decimal::ZERO, |order| order.held_cash);
    if hold > book.ledger.cash(currency).available + freed {
        return Err(BrokerError::InsufficientBuyingPower);
    }
    Ok(hold)
}

/// 모의계좌 모듈 문서의 확인 5~9와 2차 규칙 9 · 13. `replacing`은 정정할 때의 원주문이다(그
/// 주문이 묶은 돈 · 수량은 푼 것으로 보고, 반대 방향 확인에서도 뺀다).
pub(crate) fn check_draft(
    book: &Book,
    config: &PaperConfig,
    draft: &Draft,
    now: DateTime<Utc>,
    replacing: Option<&OrderRecord>,
) -> Result<Checked, BrokerError> {
    let market = MarketCode::of(draft.market).ok_or(BrokerError::UnsupportedMarket)?;
    let rules = config
        .rules(draft.market)
        .ok_or(BrokerError::UnsupportedMarket)?;
    let currency = CashCode::of(&rules.currency).ok_or(BrokerError::UnsupportedMarket)?;
    check_shape(draft, rules)?;
    // 사는 주문이 묶는 돈을 셈하는 값. 시장가는 늦지 않은 시세가 있어야 정해진다.
    let hold_price = match (draft.side, draft.price) {
        (Side::Sell, _) => None,
        (Side::Buy, Some(price)) => Some(Ok(price)),
        (Side::Buy, None) => Some(market_hold_price(book, config, draft, rules, currency, now)),
    };
    let value = draft
        .price
        .or_else(|| hold_price.clone().and_then(Result::ok));
    check_amount(draft, rules, value)?;
    let (session, today) = check_hours(book, market, rules, draft.order_type, now)?;
    check_price_band(book, draft, rules, today)?;

    let symbol = draft.symbol.as_str();
    let replaced_id = replacing.map(|order| &order.order_id);
    let opposite = book.ledger.orders.iter().any(|order| {
        order.is_open()
            && order.symbol == symbol
            && order.market == market
            && order.side != draft.side
            && Some(&order.order_id) != replaced_id
    });
    if opposite {
        return Err(BrokerError::OppositePendingOrderExists);
    }

    let (hold_price, hold) = match hold_price {
        Some(price) => {
            let price = price?;
            let hold = check_buying_power(book, draft, rules, currency, price, replacing)?;
            (Some(price), hold)
        }
        None => {
            let freed = replacing
                .filter(|order| order.side == Side::Sell)
                .map_or(Decimal::ZERO, OrderRecord::remaining);
            if draft.quantity > book.ledger.sellable(symbol, market) + freed {
                return Err(BrokerError::InsufficientSellableQuantity);
            }
            (None, Decimal::ZERO)
        }
    };
    Ok(Checked {
        market,
        currency,
        hold_price,
        hold,
        session,
    })
}
