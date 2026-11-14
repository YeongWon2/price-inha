//! 체결. 시세가 올 때마다 그 종목의 열린 주문을 접수한 순서대로 본다.
//! 규칙은 [`crate::paper`]의 모듈 문서 "체결"에 적힌 그대로다.

use chrono::{DateTime, NaiveDate, Utc};
use pi_market::{Quote, is_stale};
use rust_decimal::Decimal;

use crate::book::{Book, settlement_date};
use crate::ledger::{CashCode, FillRecord, OrderRecord};
use crate::money;
use crate::{
    CloseReason, LimitFillPrice, MarketRules, MaxFill, OrderEvent, OrderEventKind, OrderStatus,
    OrderType, PaperConfig, ProceedsUsable, Side,
};

/// 주문 하나를 이번 시세로 어떻게 체결할지 셈한 것.
#[derive(Debug)]
struct Plan {
    /// 체결할 수량. 0이면 체결 없이 거부만 한다.
    quantity: Decimal,
    price: Decimal,
    amount: Decimal,
    commission: Decimal,
    tax: Decimal,
    settlement_date: NaiveDate,
    /// 판 돈을 바로 쓸 수 있는가.
    usable_now: bool,
    /// 돈이 모자라 주문을 거부로 끝내는가.
    reject: bool,
}

/// 이 주문이 이 시세에 체결되는 값. 체결 조건이 안 되면 `None`.
fn fill_price(order: &OrderRecord, quote: Decimal, rules: &MarketRules) -> Option<Decimal> {
    let price = match (order.order_type, order.price) {
        (OrderType::Market, _) => money::market_price(quote, order.side, rules)?,
        (OrderType::Limit, None) => return None,
        (OrderType::Limit, Some(limit)) => {
            let crossed = match order.side {
                Side::Buy => quote <= limit,
                Side::Sell => quote >= limit,
            };
            if !crossed {
                return None;
            }
            match rules.limit_fill_price {
                LimitFillPrice::Limit => limit,
                LimitFillPrice::Quote => quote,
            }
        }
    };
    (price > Decimal::ZERO).then_some(price)
}

/// 이번에 체결할 수량과, 돈이 모자라 거부로 끝내는지.
fn fill_quantity(
    book: &Book,
    order: &OrderRecord,
    price: Decimal,
    rules: &MarketRules,
) -> (Decimal, bool) {
    let want = match &rules.max_fill {
        MaxFill::Unlimited => order.remaining(),
        MaxFill::PerQuote(most) => order.remaining().min(*most),
    };
    if order.side == Side::Sell {
        return (want, false);
    }
    let budget = order.held_cash + book.ledger.cash(order.currency).available;
    let enough = money::buy_cost(want, price, rules).is_some_and(|cost| cost <= budget);
    if enough {
        (want, false)
    } else {
        (money::affordable_quantity(budget, price, want, rules), true)
    }
}

/// 체결할 수 있으면 그 계획. 늦은 시세 · 닫힌 장 · 못 쓰는 유형 · 조건 미달이면 `None`.
fn plan(
    book: &Book,
    config: &PaperConfig,
    order: &OrderRecord,
    quote: &Quote,
    now: DateTime<Utc>,
) -> Option<Plan> {
    let rules = config.rules(order.market.market())?;
    if CashCode::of(&rules.currency) != Some(order.currency) {
        return None;
    }
    let calendar = book.calendar(order.market)?;
    if is_stale(quote, calendar, now, &config.stale) {
        return None;
    }
    let session = calendar.session_at(now)?;
    if !rules.sessions.allows(session.kind, order.order_type) {
        return None;
    }
    let price = fill_price(order, quote.price, rules)?;
    let (quantity, reject) = fill_quantity(book, order, price, rules);
    if quantity <= Decimal::ZERO && !reject {
        return None;
    }
    let amount = quantity.checked_mul(price)?;
    // 떼는 돈이 체결 금액을 넘지 않게 한다(아주 작은 금액을 올림할 때).
    let commission = money::charge(amount, rules.commission_rate, rules.rounding)?.min(amount);
    let tax = match order.side {
        Side::Buy => Decimal::ZERO,
        Side::Sell => {
            money::charge(amount, rules.sell_tax_rate, rules.rounding)?.min(amount - commission)
        }
    };
    let settlement = rules.settlement;
    Some(Plan {
        quantity,
        price,
        amount,
        commission,
        tax,
        settlement_date: settlement_date(calendar, now, settlement.business_days)?,
        usable_now: settlement.proceeds_usable == ProceedsUsable::Immediately
            || settlement.business_days == 0,
        reject,
    })
}

/// 주문에 체결을 적는다.
fn record(order: &mut OrderRecord, plan: &Plan, now: DateTime<Utc>) -> FillRecord {
    order.filled_quantity += plan.quantity;
    order.filled_amount += plan.amount;
    order.commission += plan.commission;
    order.tax += plan.tax;
    order.filled_at = Some(now);
    order.settlement_date = Some(plan.settlement_date);
    order.status = if order.remaining().is_zero() {
        OrderStatus::Filled
    } else {
        OrderStatus::PartialFilled
    };
    FillRecord {
        order_id: order.order_id.clone(),
        market: order.market,
        symbol: order.symbol.clone(),
        side: order.side,
        quantity: plan.quantity,
        price: plan.price,
        amount: plan.amount,
        commission: plan.commission,
        tax: plan.tax,
        currency: order.currency,
        filled_at: now,
        settlement_date: plan.settlement_date,
    }
}

/// 사는 체결의 돈: 묶은 돈을 풀어 비용을 내고, 남은 수량만큼 다시 묶는다.
fn settle_buy(book: &mut Book, config: &PaperConfig, index: usize, fill: &FillRecord) {
    let Some(order) = book.ledger.orders.get(index) else {
        return;
    };
    let needed = match (
        order.is_open(),
        order.hold_price,
        config.rules(fill.market.market()),
    ) {
        (true, Some(price), Some(rules)) => money::buy_hold(order.remaining(), price, rules),
        _ => Some(Decimal::ZERO),
    };
    let released = order.held_cash;
    let cash = book.ledger.cash_mut(fill.currency);
    cash.available += released - fill.amount - fill.commission;
    // 다시 셈한 만큼 묶을 돈이 없으면 있는 만큼만 묶는다(쓸 수 있는 돈은 음수가 되지 않는다).
    let held = needed
        .unwrap_or(cash.available)
        .min(cash.available)
        .max(Decimal::ZERO);
    cash.available -= held;
    if let Some(order) = book.ledger.orders.get_mut(index) {
        order.held_cash = held;
    }
    book.ledger.add_holding(fill);
}

/// 파는 체결의 돈: 받는 돈(금액 − 수수료 − 세금)을 바로 쓰거나 결제일까지 둔다.
fn settle_sell(book: &mut Book, fill: &FillRecord, usable_now: bool) {
    let proceeds = fill.amount - fill.commission - fill.tax;
    if usable_now {
        book.ledger.cash_mut(fill.currency).available += proceeds;
    } else {
        book.defer(fill.currency, fill.market, fill.settlement_date, proceeds);
    }
    book.ledger
        .remove_holding(&fill.symbol, fill.market, fill.quantity);
}

/// 계획대로 체결하고 이벤트를 낸다.
fn apply(
    book: &mut Book,
    config: &PaperConfig,
    index: usize,
    plan: &Plan,
    now: DateTime<Utc>,
    out: &mut Vec<OrderEvent>,
) {
    if plan.quantity > Decimal::ZERO {
        let Some(order) = book.ledger.orders.get_mut(index) else {
            return;
        };
        let fill = record(order, plan, now);
        let kind = match order.status {
            OrderStatus::Filled => OrderEventKind::Fill,
            _ => OrderEventKind::PartialFill,
        };
        match fill.side {
            Side::Buy => settle_buy(book, config, index, &fill),
            Side::Sell => settle_sell(book, &fill, plan.usable_now),
        }
        book.ledger.fills.push(fill);
        book.emit(kind, index, out);
    }
    let open = book
        .ledger
        .orders
        .get(index)
        .is_some_and(OrderRecord::is_open);
    if plan.reject && open {
        book.close(
            index,
            OrderStatus::Rejected,
            CloseReason::InsufficientBuyingPower,
            None,
        );
        book.emit(OrderEventKind::Rejected, index, out);
    }
}

/// 시세 하나를 처리한다: 마감이 된 주문을 취소하고, 시세를 기억하고, 그 종목의 열린 주문을
/// 접수한 순서대로 체결해 본다.
pub(crate) fn on_quote(
    book: &mut Book,
    config: &PaperConfig,
    quote: &Quote,
    now: DateTime<Utc>,
) -> Vec<OrderEvent> {
    let mut out = Vec::new();
    book.expire(now, &mut out);
    // 현금을 두지 않는 통화의 시세나 0 이하의 값으로는 아무것도 하지 않는다.
    let Some(currency) = CashCode::of(&quote.currency) else {
        return out;
    };
    if quote.price <= Decimal::ZERO {
        return out;
    }
    let symbol = quote.symbol.as_str();
    book.ledger.remember_quote(symbol, currency, quote.price);
    let candidates: Vec<usize> = book
        .ledger
        .orders
        .iter()
        .enumerate()
        .filter(|(_, order)| {
            order.is_open() && order.symbol == symbol && order.currency == currency
        })
        .map(|(index, _)| index)
        .collect();
    for index in candidates {
        let planned = book
            .ledger
            .orders
            .get(index)
            .and_then(|order| plan(book, config, order, quote, now));
        if let Some(planned) = planned {
            apply(book, config, index, &planned, now, &mut out);
        }
    }
    out
}
