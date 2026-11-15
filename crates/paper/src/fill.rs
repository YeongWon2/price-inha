//! 체결. 시세가 올 때마다 그 종목의 열린 주문을 접수한 순서대로 본다.
//! 규칙은 [`crate::paper`]의 모듈 문서 "2차 규칙"과 "체결"에 적힌 그대로다.

use chrono::{DateTime, NaiveDate, Utc};
use pi_market::{Quote, StalePolicy, is_stale};
use rust_decimal::Decimal;

use crate::book::Book;
use crate::calendar::settlement_date;
use crate::ledger::{CashCode, FillRecord, OrderRecord, QuoteRecord};
use crate::marks::TradeCheck;
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

/// 받은 지 늦은 시세 기준을 넘겼는가 (N6). 딱 기준만큼이면 아직 아니다. 받은 시각이 `now`보다
/// 뒤면(시계가 어긋남) 나이를 잴 수 없으니 늦었다고 하지 않는다.
pub(crate) fn too_old(
    received_at: DateTime<Utc>,
    now: DateTime<Utc>,
    policy: &StalePolicy,
) -> bool {
    now.signed_duration_since(received_at)
        .to_std()
        .is_ok_and(|age| age > policy.max_age)
}

/// 이 주문이 `quantity`를 `price`에 체결할 때 떼는 수수료와 세금. 주문 단위로 누적해 뗀다:
/// `맞춤(누적 체결 금액 × 비율) − 이미 뗀 것`. 이번 체결 금액보다 많이 떼지 않는다.
fn charges(
    order: &OrderRecord,
    amount: Decimal,
    rules: &MarketRules,
) -> Option<(Decimal, Decimal)> {
    let total = order.filled_amount.checked_add(amount)?;
    let commission_total = money::charge_total(total, rules.commission_rate, rules.rounding)?;
    let commission = (commission_total - order.commission).clamp(Decimal::ZERO, amount);
    let tax = match order.side {
        Side::Buy => Decimal::ZERO,
        Side::Sell => {
            let tax_total = money::charge_total(total, rules.sell_tax_rate, rules.rounding)?
                .min(total - commission_total);
            (tax_total - order.tax).clamp(Decimal::ZERO, amount - commission)
        }
    };
    Some((commission, tax))
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
    let cost = |quantity: Decimal| {
        let amount = quantity.checked_mul(price)?;
        amount.checked_add(charges(order, amount, rules)?.0)
    };
    if cost(want).is_some_and(|cost| cost <= budget) {
        return (want, false);
    }
    // 지정가 주문은 접수 때 묶은 돈으로 언제나 모자라지 않는다(누적 수수료는 묶은 수수료 몫을
    // 넘지 않는다). 그래서 돈이 모자라 거부로 끝나는 것은 시장가뿐이다.
    let affordable = money::affordable_quantity(budget, price, want, rules, cost);
    (affordable, order.order_type == OrderType::Market)
}

/// 이 시세가 이 주문을 접수한 **뒤의** 것인가 (4차 규칙 3): 받은 시각이 접수 시각보다 뒤여야
/// 하고, 거래소 시각이 있으면 그것도 접수 시각보다 뒤여야 한다(거래소 시계가 앞서 거래소 시각만
/// 뒤인 시세는 쓰지 않는다).
fn after_order(order: &OrderRecord, quote: &Quote) -> bool {
    quote.received_at > order.ordered_at
        && quote
            .exchange_time
            .is_none_or(|traded_at| traded_at > order.ordered_at)
}

/// 체결할 수 있으면 그 계획. 주문 전의 시세 · 낡은 달력 · 늦은 시세 · 닫힌 장 · 못 쓰는 유형 ·
/// 조건 미달이면 `None`.
fn plan(
    book: &Book,
    config: &PaperConfig,
    order: &OrderRecord,
    quote: &Quote,
    now: DateTime<Utc>,
) -> Option<Plan> {
    let rules = config.rules(order.market.market())?;
    if CashCode::of(&rules.currency) != Some(order.currency) || !after_order(order, quote) {
        return None;
    }
    let calendar = book.covering(order.market, now)?;
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
    let (commission, tax) = charges(order, amount, rules)?;
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

/// 사는 체결의 돈: 묶은 돈을 풀어 비용을 내고, 남은 수량 몫(3차 규칙 4)만큼 다시 묶는다.
/// 지정가 사기는 남은 수량 몫이 언제나 "묶었던 돈 − 이번에 낸 돈" 이하라 쓸 수 있는 돈을
/// 되가져가지 않는다.
fn settle_buy(book: &mut Book, config: &PaperConfig, index: usize, fill: &FillRecord) {
    let Some(order) = book.ledger.orders.get(index) else {
        return;
    };
    let needed = match (
        order.is_open(),
        order.hold_price,
        config.rules(fill.market.market()),
    ) {
        (true, Some(price), Some(rules)) => money::open_buy_hold(order, price, rules),
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

/// 이 시세를 그 종목의 마지막으로 처리한 시세로 기억한다. 이미 처리한 시세(거래소 시각 · 값이
/// 같음. 거래소 시각이 없는 것끼리는 값이 같으면 같다)였으면 참을 돌려준다. 그때는 받은 시각만
/// 더 늦은 쪽으로 고친다(같은 값을 방금 다시 받았다).
fn remember(book: &mut Book, quote: &Quote, currency: CashCode) -> bool {
    let symbol = quote.symbol.as_str();
    let last = book.ledger.last_quote(symbol, currency);
    let seen = last
        .is_some_and(|last| last.exchange_time == quote.exchange_time && last.price == quote.price);
    let received_at = match last {
        Some(last) if seen => last.received_at.max(quote.received_at),
        _ => quote.received_at,
    };
    book.ledger.remember_quote(QuoteRecord {
        symbol: symbol.to_owned(),
        currency,
        price: quote.price,
        exchange_time: quote.exchange_time,
        received_at,
    });
    seen
}

/// 시세 하나를 처리한다: 마감이 된 주문을 취소하고, 쓸 수 있는 시세면 기억하고, 처음 보는
/// 시세면 그 종목의 열린 주문을 접수한 순서대로 체결해 본다.
pub(crate) fn on_quote(
    book: &mut Book,
    config: &PaperConfig,
    quote: &Quote,
    now: DateTime<Utc>,
) -> Vec<OrderEvent> {
    let mut out = Vec::new();
    book.advance(now, &mut out);
    // 현금을 두지 않는 통화의 시세나 0 이하의 값으로는 아무것도 하지 않는다.
    let Some(currency) = CashCode::of(&quote.currency) else {
        return out;
    };
    if quote.price <= Decimal::ZERO {
        return out;
    }
    // 받은 시각이 앞날인 시세와 늦은 시세(N6)는 체결도 기억도 하지 않는다.
    if quote.received_at > now || too_old(quote.received_at, now, &config.stale) {
        return out;
    }
    let symbol = quote.symbol.as_str();
    // 거래소 시각이 있으면 그 종목의 가장 늦은 거래소 시각에 견준다(4차 규칙 1 · 2). 옛 시세는
    // 체결도 기억도 하지 않는다. 그 시각에 이미 쓴 거래는 기억만 하고 체결하지 않는다.
    let used = match quote.exchange_time {
        Some(traded_at) => {
            match book
                .ledger
                .check_trade(symbol, currency, traded_at, quote.price)
            {
                TradeCheck::Older => return out,
                TradeCheck::Used => true,
                TradeCheck::Fresh => false,
            }
        }
        None => false,
    };
    if remember(book, quote, currency) || used {
        return out;
    }
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
