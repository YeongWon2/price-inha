//! 종목마다의 가장 늦은 거래소 시각과 그 시각에 쓴 거래 (4차 규칙 1 · 2, 5차 규칙 1 · 3), 그리고
//! 옛 모양의 장부를 올릴 때 쓰는 셈(4차 규칙 4).

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::ledger::{CashCode, Ledger};
use crate::paper::SNAPSHOT_VERSION;
use crate::{MAX_EXCHANGE_LEAD, MAX_SAME_TIME_PRICES};

/// 시세의 거래소 시각 가운데 쓸 수 있는 것 (5차 규칙 1). 받은 시각보다 [`MAX_EXCHANGE_LEAD`]
/// **넘게** 앞선 것은 버린다(`None`). 딱 그만큼 앞선 것은 쓴다.
pub(crate) fn trusted_exchange_time(
    exchange_time: Option<DateTime<Utc>>,
    received_at: DateTime<Utc>,
) -> Option<DateTime<Utc>> {
    let lead = chrono::Duration::from_std(MAX_EXCHANGE_LEAD).unwrap_or(chrono::Duration::MAX);
    exchange_time.filter(|traded_at| traded_at.signed_duration_since(received_at) <= lead)
}

/// 종목에서 가장 늦게 기억한 거래소 시각과, 그 시각에 이미 쓴 거래의 값들 (4차 규칙 1 · 2).
/// 거래소 시각이 없는 시세는 이것을 바꾸지 않는다. 그래서 시각 없는 시세가 끼어도 옛 시세를
/// 가리고, 같은 시각의 같은 거래를 두 번 쓰지 않는다.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct TradeMark {
    pub symbol: String,
    pub currency: CashCode,
    /// 그 종목에서 가장 늦은 거래소 시각.
    pub traded_at: DateTime<Utc>,
    /// 그 시각에 이미 쓴 거래의 값. 받은 순서. [`MAX_SAME_TIME_PRICES`]개까지(5차 규칙 3).
    pub prices: Vec<Decimal>,
}

/// 거래소 시각이 있는 시세를 [`TradeMark`]에 견준 결과.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TradeCheck {
    /// 가장 늦은 거래소 시각보다 앞이다(옛 시세).
    Older,
    /// 가장 늦은 거래소 시각에 이미 쓴 거래다.
    Used,
    /// 처음 보는 거래다(기억했다).
    Fresh,
}

impl Ledger {
    /// 거래소 시각 `traded_at`에 `price`로 일어난 거래를 견주고, 처음 보는 거래면 기억한다.
    /// 옛 시세나 이미 쓴 거래면 아무것도 바꾸지 않는다. 같은 시각의 값이
    /// [`MAX_SAME_TIME_PRICES`]개를 넘으면 가장 오래 기억한 것부터 버린다(5차 규칙 3).
    pub(crate) fn check_trade(
        &mut self,
        symbol: &str,
        currency: CashCode,
        traded_at: DateTime<Utc>,
        price: Decimal,
    ) -> TradeCheck {
        let found = self
            .trade_marks
            .iter_mut()
            .find(|mark| mark.symbol == symbol && mark.currency == currency);
        match found {
            Some(mark) if traded_at < mark.traded_at => TradeCheck::Older,
            Some(mark) if traded_at == mark.traded_at => {
                if mark.prices.contains(&price) {
                    TradeCheck::Used
                } else {
                    mark.prices.push(price);
                    let over = mark.prices.len().saturating_sub(MAX_SAME_TIME_PRICES);
                    mark.prices.drain(..over);
                    TradeCheck::Fresh
                }
            }
            Some(mark) => {
                mark.traded_at = traded_at;
                mark.prices = vec![price];
                TradeCheck::Fresh
            }
            None => {
                self.trade_marks.push(TradeMark {
                    symbol: symbol.to_owned(),
                    currency,
                    traded_at,
                    prices: vec![price],
                });
                self.trade_marks
                    .sort_by(|a, b| (&a.symbol, a.currency).cmp(&(&b.symbol, b.currency)));
                TradeCheck::Fresh
            }
        }
    }

    /// 이 장부에 적힌 가장 늦은 시각: 마지막 시세의 받은 시각, 주문의 접수 · 체결 · 취소 시각,
    /// 체결 기록의 시각, 멱등성 키의 접수 시각. 적힌 시각이 없으면 `None`.
    /// 모양 2(내보낸 시각이 없다)를 올릴 때 내보낸 시각의 기본값으로 쓴다(계좌가 받은 `now`는
    /// 언제나 이 시각들 이상이다).
    pub(crate) fn latest_recorded(&self) -> Option<DateTime<Utc>> {
        let quotes = self.last_quotes.iter().map(|quote| quote.received_at);
        let orders = self.orders.iter().flat_map(|order| {
            [Some(order.ordered_at), order.filled_at, order.canceled_at]
                .into_iter()
                .flatten()
        });
        let fills = self.fills.iter().map(|fill| fill.filled_at);
        let keys = self.idempotency.values().map(|key| key.accepted_at);
        quotes.chain(orders).chain(fills).chain(keys).max()
    }
}

/// 옛 모양의 장부를 지금 모양으로 올린다(4차 규칙 4). 모르는 모양 번호면 `None`.
pub(crate) fn upgrade(version: u32, mut ledger: Ledger) -> Option<Ledger> {
    if !(2..=SNAPSHOT_VERSION).contains(&version) {
        return None;
    }
    if version < 3 {
        // 모양 2에는 내보낸 시각이 없다.
        ledger.last_now = ledger.latest_recorded();
    }
    if version < 4 {
        // 모양 3까지는 거래소 시각을 마지막 시세에만 기억했다. 먼 앞날 거래소 시각은 버린다
        // (5차 규칙 1).
        ledger.trade_marks.clear();
        let timed: Vec<(String, CashCode, DateTime<Utc>, Decimal)> = ledger
            .last_quotes
            .iter()
            .filter_map(|quote| {
                let traded_at = trusted_exchange_time(quote.exchange_time, quote.received_at)?;
                Some((quote.symbol.clone(), quote.currency, traded_at, quote.price))
            })
            .collect();
        for (symbol, currency, traded_at, price) in timed {
            ledger.check_trade(&symbol, currency, traded_at, price);
        }
    }
    Some(ledger)
}

#[cfg(test)]
mod tests {
    use super::trusted_exchange_time;
    use crate::MAX_EXCHANGE_LEAD;
    use chrono::{DateTime, TimeZone, Utc};

    fn at(seconds: i64) -> DateTime<Utc> {
        Utc.timestamp_opt(1_800_000_000 + seconds, 0).unwrap()
    }

    #[test]
    fn 받은_시각보다_5분_넘게_앞선_거래소_시각만_버린다() {
        let lead = chrono::Duration::from_std(MAX_EXCHANGE_LEAD).unwrap();
        let received = at(0);
        assert_eq!(trusted_exchange_time(None, received), None);
        assert_eq!(
            trusted_exchange_time(Some(at(-60)), received),
            Some(at(-60))
        );
        assert_eq!(
            trusted_exchange_time(Some(received + lead), received),
            Some(received + lead)
        );
        let too_far = received + lead + chrono::Duration::seconds(1);
        assert_eq!(trusted_exchange_time(Some(too_far), received), None);
    }
}
