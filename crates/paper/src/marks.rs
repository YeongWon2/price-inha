//! 종목마다의 가장 늦은 거래소 시각과 그 시각에 쓴 거래 (4차 규칙 1 · 2), 그리고 옛 모양의
//! 장부를 올릴 때 쓰는 셈(4차 규칙 4).

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::ledger::{CashCode, Ledger};

/// 종목에서 가장 늦게 기억한 거래소 시각과, 그 시각에 이미 쓴 거래의 값들 (4차 2 · 3번).
/// 거래소 시각이 없는 시세는 이것을 바꾸지 않는다. 그래서 시각 없는 시세가 끼어도 옛 시세를
/// 가리고, 같은 시각의 같은 거래를 두 번 쓰지 않는다.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct TradeMark {
    pub symbol: String,
    pub currency: CashCode,
    /// 그 종목에서 가장 늦은 거래소 시각.
    pub traded_at: DateTime<Utc>,
    /// 그 시각에 이미 쓴 거래의 값. 받은 순서.
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
    /// 옛 시세나 이미 쓴 거래면 아무것도 바꾸지 않는다.
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
