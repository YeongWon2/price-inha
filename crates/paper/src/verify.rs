//! 되살릴 상태 값이 말이 되는지 보는 검사 (V6). 말이 안 되는 값은 되살리지 않는다.
//! 무엇을 보는지는 [`SnapshotReason`]에 하나씩 적혀 있다.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;

use crate::ledger::{CashCode, Ledger, MarketCode, OrderRecord};
use crate::money;
use crate::{
    BrokerError, MAX_EXCHANGE_LEAD, MAX_SAME_TIME_PRICES, OrderId, OrderStatus, OrderType,
    PaperConfig, Side, SnapshotReason,
};

/// 음수인가(0은 아니다).
fn negative(value: &Decimal) -> bool {
    value.is_sign_negative() && !value.is_zero()
}

/// 현금과 보유: 음수인 돈, 0 이하인 보유, 같은 종목 · 시장의 보유가 둘.
fn check_assets(ledger: &Ledger) -> Result<(), SnapshotReason> {
    for cash in [&ledger.krw, &ledger.usd] {
        if negative(&cash.available) || cash.unsettled.iter().any(|lot| negative(&lot.amount)) {
            return Err(SnapshotReason::NegativeCash);
        }
    }
    for (index, holding) in ledger.holdings.iter().enumerate() {
        if holding.quantity <= Decimal::ZERO {
            return Err(SnapshotReason::NonPositiveHolding);
        }
        if holding.average_purchase_price <= Decimal::ZERO {
            return Err(SnapshotReason::NonPositiveAveragePrice);
        }
        let duplicated = ledger
            .holdings
            .iter()
            .skip(index + 1)
            .any(|other| other.symbol == holding.symbol && other.market == holding.market);
        if duplicated {
            return Err(SnapshotReason::DuplicateHolding);
        }
    }
    Ok(())
}

/// 열린 주문이 묶어도 되는 가장 큰 돈: 사는 주문은 남은 수량 몫(3차 규칙 4. 받을 때 · 체결할 때와
/// 같은 식), 파는 주문은 0. 셈할 수 없는 주문(묶는 돈을 셈하는 값이 없거나 넘침)은 접수될 수
/// 없었으니 0이다.
fn hold_limit(order: &OrderRecord, config: &PaperConfig) -> Decimal {
    let rules = config.rules(order.market.market());
    match (order.side, order.hold_price, rules) {
        (Side::Buy, Some(price), Some(rules)) => money::open_buy_hold(order, price, rules),
        _ => None,
    }
    .unwrap_or(Decimal::ZERO)
}

/// 상태와 체결 수량이 맞는가: 전부 체결은 다 체결됐고, 접수는 체결이 없고, 일부 체결은 그 사이다.
/// 그 밖의 상태(취소 · 거부 · 정정됨 등)는 다 체결되지 않았다.
fn status_matches(order: &OrderRecord) -> bool {
    let filled = order.filled_quantity;
    match order.status {
        OrderStatus::Filled => filled == order.quantity,
        OrderStatus::Pending => filled.is_zero(),
        OrderStatus::PartialFilled => filled > Decimal::ZERO && filled < order.quantity,
        _ => filled < order.quantity,
    }
}

/// 뗀 수수료 · 세금 · 누적 체결 금액이 말이 되는가: 음수가 아니고, 뗀 것의 합이 누적 체결 금액을
/// 넘지 않고, 지정가 사기의 누적 체결 금액이 `체결 수량 × 지정가`를 넘지 않는다.
fn execution_fits(order: &OrderRecord) -> bool {
    let amounts = [order.filled_amount, order.commission, order.tax];
    if amounts.iter().any(negative) {
        return false;
    }
    let charged = order.commission.checked_add(order.tax);
    if charged.is_none_or(|charged| charged > order.filled_amount) {
        return false;
    }
    match (order.side, order.order_type, order.price) {
        (Side::Buy, OrderType::Limit, Some(limit)) => order
            .filled_quantity
            .checked_mul(limit)
            .is_some_and(|most| order.filled_amount <= most),
        _ => true,
    }
}

/// 주문 하나: 수량, 체결 수량, 통화, 상태, 체결 금액, 묶은 돈.
fn check_order(order: &OrderRecord, config: &PaperConfig) -> Result<(), SnapshotReason> {
    if order.quantity <= Decimal::ZERO {
        return Err(SnapshotReason::NonPositiveOrderQuantity);
    }
    if negative(&order.filled_quantity)
        || order.filled_quantity > order.quantity
        || !order.filled_quantity.fract().is_zero()
    {
        return Err(SnapshotReason::BadFilledQuantity);
    }
    let currency = config
        .rules(order.market.market())
        .and_then(|rules| CashCode::of(&rules.currency));
    if currency != Some(order.currency) {
        return Err(SnapshotReason::CurrencyMismatch);
    }
    if !status_matches(order) {
        return Err(SnapshotReason::StatusMismatch);
    }
    if !execution_fits(order) {
        return Err(SnapshotReason::BadExecution);
    }
    if negative(&order.held_cash) {
        return Err(SnapshotReason::NegativeHeldCash);
    }
    if !order.is_open() {
        if !order.held_cash.is_zero() {
            return Err(SnapshotReason::ClosedOrderHoldsCash);
        }
        return Ok(());
    }
    if order.held_cash > hold_limit(order, config) {
        return Err(SnapshotReason::HeldCashTooLarge);
    }
    Ok(())
}

/// 열린 파는 주문들의 남은 수량 합이 그 종목 · 시장의 보유를 넘지 않는가.
fn check_sells(ledger: &Ledger) -> Result<(), SnapshotReason> {
    let mut locked: BTreeMap<(&str, MarketCode), Decimal> = BTreeMap::new();
    for order in &ledger.orders {
        if order.is_open() && order.side == Side::Sell {
            *locked
                .entry((order.symbol.as_str(), order.market))
                .or_default() += order.remaining();
        }
    }
    for ((symbol, market), quantity) in locked {
        let owned = ledger
            .holding(symbol, market)
            .map_or(Decimal::ZERO, |holding| holding.quantity);
        if quantity > owned {
            return Err(SnapshotReason::SellExceedsHolding);
        }
    }
    Ok(())
}

/// 마지막 시세: 값이 0 이하이거나, 받은 시각이 내보낸 시각(`last_now`)보다 뒤면 말이 안 된다.
/// 내보낸 시각이 없는데(아무 호출도 받지 않음) 시세가 있는 것도 같다. 종목마다 기억한 거래의
/// 값이 0 이하인 것도 같다(4차 규칙 2).
fn check_quotes(ledger: &Ledger) -> Result<(), SnapshotReason> {
    let bad = ledger.last_quotes.iter().any(|quote| {
        quote.price <= Decimal::ZERO
            || ledger
                .last_now
                .is_none_or(|exported| quote.received_at > exported)
    });
    let bad_mark = ledger
        .trade_marks
        .iter()
        .any(|mark| mark.prices.iter().any(|price| *price <= Decimal::ZERO));
    if bad || bad_mark {
        return Err(SnapshotReason::BadLastQuote);
    }
    Ok(())
}

/// 종목마다의 거래 기억(5차 규칙 4): 종목 · 통화 순서로 엄격히 정렬(같은 것이 둘이 아님),
/// 값은 [`MAX_SAME_TIME_PRICES`]개까지, 기억한 시각은 그 종목 마지막 시세의 거래소 시각 이상.
/// 마지막 시세에 거래소 시각이 없거나, 거래소 시각이 있는 마지막 시세에 기억이 없는 것은
/// 받아들인다(기억이 없으면 그 시각부터 다시 기억할 뿐 옛 시세를 열지 않는다).
/// 기억한 시각이 마지막 시각(`last_now`) + [`MAX_EXCHANGE_LEAD`]보다 늦으면(엄격한 초과) 먼 앞날
/// 기억이라 거부한다(6차 규칙 1). `last_now`가 없으면 기준이 없으므로 이 검사는 하지 않는다(보수적).
fn check_marks(ledger: &Ledger) -> Result<(), SnapshotReason> {
    let lead = chrono::Duration::from_std(MAX_EXCHANGE_LEAD).unwrap_or(chrono::Duration::MAX);
    let latest = ledger.last_now.map(|exported| {
        exported
            .checked_add_signed(lead)
            .unwrap_or(DateTime::<Utc>::MAX_UTC)
    });
    let sorted = ledger
        .trade_marks
        .windows(2)
        .all(|pair| (&pair[0].symbol, pair[0].currency) < (&pair[1].symbol, pair[1].currency));
    let bad = ledger.trade_marks.iter().any(|mark| {
        mark.prices.len() > MAX_SAME_TIME_PRICES
            || latest.is_some_and(|limit| mark.traded_at > limit)
            || ledger
                .last_quote(&mark.symbol, mark.currency)
                .and_then(|quote| quote.exchange_time)
                .is_some_and(|traded_at| mark.traded_at < traded_at)
    });
    if !sorted || bad {
        return Err(SnapshotReason::BadTradeMark);
    }
    Ok(())
}

/// 이 계좌가 낸 식별자의 번호. 다른 모양의 글자면 `None`.
fn id_number(order_id: &OrderId) -> Option<u64> {
    order_id.as_str().strip_prefix("PAPER-")?.parse().ok()
}

/// 다음 식별자 번호가 이미 낸 식별자(주문 · 체결 기록 · 멱등성 키가 가리키는 것)보다 큰가.
fn check_next_id(ledger: &Ledger) -> Result<(), SnapshotReason> {
    let issued = ledger
        .orders
        .iter()
        .map(|order| &order.order_id)
        .chain(ledger.fills.iter().map(|fill| &fill.order_id))
        .chain(ledger.idempotency.values().map(|key| &key.order_id))
        .filter_map(id_number)
        .max();
    match issued {
        Some(last) if ledger.next_id <= last => Err(SnapshotReason::NextIdNotAhead),
        _ => Ok(()),
    }
}

/// 되살릴 장부가 말이 되는가. 처음 걸린 이유를 돌려준다.
pub(crate) fn check_ledger(ledger: &Ledger, config: &PaperConfig) -> Result<(), BrokerError> {
    let check = || {
        check_assets(ledger)?;
        for (index, order) in ledger.orders.iter().enumerate() {
            check_order(order, config)?;
            let duplicated = ledger
                .orders
                .iter()
                .skip(index + 1)
                .any(|other| other.order_id == order.order_id);
            if duplicated {
                return Err(SnapshotReason::DuplicateOrderId);
            }
        }
        check_sells(ledger)?;
        check_quotes(ledger)?;
        check_marks(ledger)?;
        check_next_id(ledger)
    };
    check().map_err(BrokerError::InvalidSnapshot)
}

#[cfg(test)]
mod tests {
    use super::check_marks;
    use crate::ledger::{CashCode, Ledger, QuoteRecord};
    use crate::{MAX_SAME_TIME_PRICES, SnapshotReason};
    use chrono::{DateTime, TimeZone, Utc};
    use rust_decimal::Decimal;

    fn at(seconds: i64) -> DateTime<Utc> {
        Utc.timestamp_opt(1_800_000_000 + seconds, 0).unwrap()
    }

    /// 삼성 5초 시세 하나를 받은 장부(기억 목록 포함).
    fn ledger() -> Ledger {
        let mut ledger = Ledger::new(Decimal::ZERO, Decimal::ZERO);
        ledger.remember_quote(QuoteRecord {
            symbol: "005930".to_owned(),
            currency: CashCode::Krw,
            price: Decimal::from(70_000),
            exchange_time: Some(at(5)),
            received_at: at(5),
        });
        ledger.check_trade("005930", CashCode::Krw, at(5), Decimal::from(70_000));
        ledger
    }

    #[test]
    fn 거래소_시각이_있는_마지막_시세에_기억이_없어도_받아들인다() {
        let mut ledger = ledger();
        assert_eq!(check_marks(&ledger), Ok(()));
        ledger.trade_marks.clear();
        assert_eq!(check_marks(&ledger), Ok(()));
    }

    #[test]
    fn 같은_시각_값이_상한을_넘는_기억은_거부한다() {
        let mut ledger = ledger();
        let prices = (0..MAX_SAME_TIME_PRICES).map(|i| Decimal::from(60_000 + i));
        ledger.trade_marks[0].prices = prices.collect();
        assert_eq!(check_marks(&ledger), Ok(()));
        ledger.trade_marks[0].prices.push(Decimal::from(1));
        assert_eq!(check_marks(&ledger), Err(SnapshotReason::BadTradeMark));
    }
}
