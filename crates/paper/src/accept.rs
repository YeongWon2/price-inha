//! 주문을 받을 때의 확인. 접수와 정정이 같은 확인을 거친다. 순서는 [`crate::paper`]의 모듈
//! 문서에 적힌 그대로다. 여기서는 아무것도 바꾸지 않는다(받을지 말지와 묶을 것만 셈한다).

use std::sync::Arc;

use chrono::{DateTime, Utc};
use pi_market::{Market, Session, Symbol};
use rust_decimal::Decimal;

use crate::book::Book;
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

/// 장 시간: 달력이 있고, `now`에 열린 세션이 있고, 그 세션에서 이 호가 유형을 받는가.
fn check_hours(
    book: &Book,
    market: MarketCode,
    rules: &MarketRules,
    order_type: OrderType,
    now: DateTime<Utc>,
) -> Result<Session, BrokerError> {
    let session = book
        .calendar(market)
        .and_then(|calendar| calendar.session_at(now))
        .ok_or(BrokerError::OrderHoursClosed)?;
    if rules.sessions.allows(session.kind, order_type) {
        return Ok(session);
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
fn check_price_band(book: &Book, draft: &Draft, rules: &MarketRules) -> Result<(), BrokerError> {
    let (Some(price), Some(rate)) = (draft.price, rules.price_limit_rate) else {
        return Ok(());
    };
    let reference = book
        .reference
        .get(draft.symbol.as_str())
        .ok_or(BrokerError::ReferencePriceUnknown)?;
    let (lower, upper) =
        money::price_band(*reference, rate, rules).ok_or(BrokerError::PriceOutOfRange)?;
    if price < lower || price > upper {
        return Err(BrokerError::PriceOutOfRange);
    }
    Ok(())
}

/// 사는 주문이 묶을 돈과 그것을 셈한 값. 쓸 수 있는 돈(`replacing`이 묶은 것은 푼 것으로 본다)을
/// 넘으면 받지 않는다.
fn check_buying_power(
    book: &Book,
    draft: &Draft,
    rules: &MarketRules,
    currency: CashCode,
    replacing: Option<&OrderRecord>,
) -> Result<(Decimal, Decimal), BrokerError> {
    let hold_price = match draft.price {
        Some(price) => price,
        None => {
            let quote = book
                .ledger
                .last_quote(draft.symbol.as_str(), currency)
                .ok_or(BrokerError::QuoteUnknown)?;
            money::market_price(quote, Side::Buy, rules)
                .ok_or(BrokerError::InsufficientBuyingPower)?
        }
    };
    // 셈이 넘칠 만큼 큰 주문은 어떤 돈으로도 살 수 없다.
    let hold = money::buy_hold(draft.quantity, hold_price, rules)
        .ok_or(BrokerError::InsufficientBuyingPower)?;
    let freed = replacing.map_or(Decimal::ZERO, |order| order.held_cash);
    if hold > book.ledger.cash(currency).available + freed {
        return Err(BrokerError::InsufficientBuyingPower);
    }
    Ok((hold_price, hold))
}

/// 모의계좌 모듈 문서의 확인 5~9. `replacing`은 정정할 때의 원주문이다(그 주문이 묶은 돈 ·
/// 수량은 푼 것으로 보고, 반대 방향 확인에서도 뺀다).
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
    let session = check_hours(book, market, rules, draft.order_type, now)?;
    check_price_band(book, draft, rules)?;

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

    let (hold_price, hold) = match draft.side {
        Side::Buy => {
            let (price, hold) = check_buying_power(book, draft, rules, currency, replacing)?;
            (Some(price), hold)
        }
        Side::Sell => {
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
