//! 장부: 계좌 상태 전부(현금 · 보유 · 주문 · 체결 기록 · 멱등성 키 · 마지막 시세 · 다음 번호).
//!
//! [`crate::Snapshot`]이 이 값을 그대로 담는다. 순서가 정해진 자료 구조(`Vec` · `BTreeMap`)만
//! 써서 같은 입력은 같은 값이 된다. `pi-market`의 시장 · 통화 · 종목은 글자로 바꿀 수 있게
//! 이 모듈의 부호([`MarketCode`] · [`CashCode`])와 글자로 적는다.

use std::collections::BTreeMap;

use chrono::{DateTime, NaiveDate, Utc};
use pi_market::{Currency, Market, Symbol};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::{
    CashBalance, CloseReason, Execution, Fill, Holding, Order, OrderId, OrderStatus, OrderType,
    Side, UnfilledTimeout,
};

/// 시장 부호.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub(crate) enum MarketCode {
    Kr,
    Us,
}

impl MarketCode {
    /// 다루는 시장이면 그 부호.
    pub(crate) fn of(market: Market) -> Option<Self> {
        match market {
            Market::Kr => Some(Self::Kr),
            Market::Us => Some(Self::Us),
            _ => None,
        }
    }

    pub(crate) fn market(self) -> Market {
        match self {
            Self::Kr => Market::Kr,
            Self::Us => Market::Us,
        }
    }
}

/// 현금을 두는 통화 부호.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub(crate) enum CashCode {
    Krw,
    Usd,
}

impl CashCode {
    /// 다루는 통화면 그 부호.
    pub(crate) fn of(currency: &Currency) -> Option<Self> {
        match currency {
            Currency::Krw => Some(Self::Krw),
            Currency::Usd => Some(Self::Usd),
            _ => None,
        }
    }

    pub(crate) fn currency(self) -> Currency {
        match self {
            Self::Krw => Currency::Krw,
            Self::Usd => Currency::Usd,
        }
    }
}

/// 결제 전인 돈 한 묶음.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct Lot {
    pub market: MarketCode,
    pub settles_on: NaiveDate,
    pub amount: Decimal,
}

/// 통화 하나의 현금. 묶인 돈은 주문마다([`OrderRecord::held_cash`]) 적는다.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub(crate) struct CashRecord {
    pub available: Decimal,
    pub unsettled: Vec<Lot>,
}

/// 보유 종목 하나.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct HoldingRecord {
    pub symbol: String,
    pub market: MarketCode,
    pub currency: CashCode,
    pub quantity: Decimal,
    pub average_purchase_price: Decimal,
}

/// 주문 하나와 그 주문이 묶은 것 · 마감.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct OrderRecord {
    pub order_id: OrderId,
    pub client_order_id: Option<String>,
    pub market: MarketCode,
    pub symbol: String,
    pub side: Side,
    pub order_type: OrderType,
    pub status: OrderStatus,
    pub price: Option<Decimal>,
    pub quantity: Decimal,
    pub currency: CashCode,
    pub ordered_at: DateTime<Utc>,
    pub canceled_at: Option<DateTime<Utc>>,
    pub filled_quantity: Decimal,
    pub filled_amount: Decimal,
    pub commission: Decimal,
    pub tax: Decimal,
    pub filled_at: Option<DateTime<Utc>>,
    pub settlement_date: Option<NaiveDate>,
    pub close_reason: Option<CloseReason>,
    /// 사는 주문이 묶는 돈을 셈하는 값(지정가, 또는 시장가를 접수할 때 어림한 값).
    pub hold_price: Option<Decimal>,
    /// 이 주문이 지금 묶고 있는 돈. 파는 주문과 끝난 주문은 0이다.
    pub held_cash: Decimal,
    /// 이 시각이 되면 남은 부분을 취소한다.
    pub deadline: DateTime<Utc>,
}

impl OrderRecord {
    pub(crate) fn is_open(&self) -> bool {
        self.status.is_open()
    }

    /// 아직 체결되지 않은 수량.
    pub(crate) fn remaining(&self) -> Decimal {
        self.quantity - self.filled_quantity
    }

    /// 밖에 보여 주는 모습.
    pub(crate) fn view(&self) -> Order {
        let average = if self.filled_quantity.is_zero() {
            None
        } else {
            self.filled_amount.checked_div(self.filled_quantity)
        };
        Order {
            order_id: self.order_id.clone(),
            client_order_id: self.client_order_id.clone(),
            market: self.market.market(),
            symbol: Symbol::new(self.symbol.clone()),
            side: self.side,
            order_type: self.order_type,
            status: self.status,
            price: self.price,
            quantity: self.quantity,
            currency: self.currency.currency(),
            ordered_at: self.ordered_at,
            canceled_at: self.canceled_at,
            execution: Execution {
                filled_quantity: self.filled_quantity,
                average_filled_price: average,
                filled_amount: self.filled_amount,
                commission: self.commission,
                tax: self.tax,
                filled_at: self.filled_at,
                settlement_date: self.settlement_date,
            },
            close_reason: self.close_reason,
        }
    }
}

/// 체결 한 건.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct FillRecord {
    pub order_id: OrderId,
    pub market: MarketCode,
    pub symbol: String,
    pub side: Side,
    pub quantity: Decimal,
    pub price: Decimal,
    pub amount: Decimal,
    pub commission: Decimal,
    pub tax: Decimal,
    pub currency: CashCode,
    pub filled_at: DateTime<Utc>,
    pub settlement_date: NaiveDate,
}

impl FillRecord {
    pub(crate) fn view(&self) -> Fill {
        Fill {
            order_id: self.order_id.clone(),
            market: self.market.market(),
            symbol: Symbol::new(self.symbol.clone()),
            side: self.side,
            quantity: self.quantity,
            price: self.price,
            amount: self.amount,
            commission: self.commission,
            tax: self.tax,
            currency: self.currency.currency(),
            filled_at: self.filled_at,
            settlement_date: self.settlement_date,
        }
    }
}

/// 주문 내용(멱등성 키가 같은 요청이 같은 내용인지 견준다).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct Content {
    pub market: MarketCode,
    pub symbol: String,
    pub side: Side,
    pub order_type: OrderType,
    pub quantity: Decimal,
    pub price: Option<Decimal>,
    pub timeout: UnfilledTimeout,
}

/// 멱등성 키 하나가 가리키는 접수 결과.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct KeyRecord {
    pub order_id: OrderId,
    pub accepted_at: DateTime<Utc>,
    pub content: Content,
}

/// 종목에서 마지막으로 처리한 시세. 같은 시세(거래소 시각 · 값이 같음)를 두 번 쓰지 않는 데와
/// 시장가 사기가 묶는 돈을 셈하는 데 쓴다.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct QuoteRecord {
    pub symbol: String,
    pub currency: CashCode,
    pub price: Decimal,
    /// 거래소가 준 시각. 없는 시세도 있다.
    pub exchange_time: Option<DateTime<Utc>>,
    /// 우리가 받은 시각. 같은 시세를 다시 받으면 더 늦은 쪽으로 고친다.
    pub received_at: DateTime<Utc>,
}

/// 계좌 상태 전부.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct Ledger {
    pub krw: CashRecord,
    pub usd: CashRecord,
    /// 종목 글자 · 시장 순서.
    pub holdings: Vec<HoldingRecord>,
    /// 접수한 순서.
    pub orders: Vec<OrderRecord>,
    /// 일어난 순서.
    pub fills: Vec<FillRecord>,
    pub idempotency: BTreeMap<String, KeyRecord>,
    /// 종목 글자 · 통화 순서.
    pub last_quotes: Vec<QuoteRecord>,
    /// 다음에 낼 식별자 번호.
    pub next_id: u64,
    /// 이 계좌가 받은 가장 늦은 `now`(어느 호출이든). 상태 값의 내보낸 시각이다.
    /// 아직 아무 호출도 받지 않았으면 없다.
    pub last_now: Option<DateTime<Utc>>,
}

impl Ledger {
    /// 처음 돈만 있는 장부.
    pub(crate) fn new(krw: Decimal, usd: Decimal) -> Self {
        let cash = |available| CashRecord {
            available,
            unsettled: Vec::new(),
        };
        Self {
            krw: cash(krw),
            usd: cash(usd),
            holdings: Vec::new(),
            orders: Vec::new(),
            fills: Vec::new(),
            idempotency: BTreeMap::new(),
            last_quotes: Vec::new(),
            next_id: 1,
            last_now: None,
        }
    }

    /// 새 식별자. 주문 · 취소 요청이 같은 번호 줄을 쓴다.
    pub(crate) fn issue_id(&mut self) -> OrderId {
        let id = OrderId::new(format!("PAPER-{:010}", self.next_id));
        self.next_id = self.next_id.saturating_add(1);
        id
    }

    pub(crate) fn cash(&self, code: CashCode) -> &CashRecord {
        match code {
            CashCode::Krw => &self.krw,
            CashCode::Usd => &self.usd,
        }
    }

    pub(crate) fn cash_mut(&mut self, code: CashCode) -> &mut CashRecord {
        match code {
            CashCode::Krw => &mut self.krw,
            CashCode::Usd => &mut self.usd,
        }
    }

    /// 그 통화의 현금 세 칸.
    pub(crate) fn balance(&self, code: CashCode) -> CashBalance {
        let record = self.cash(code);
        CashBalance {
            available: record.available,
            held: self
                .orders
                .iter()
                .filter(|order| order.currency == code)
                .map(|order| order.held_cash)
                .sum(),
            unsettled: record.unsettled.iter().map(|lot| lot.amount).sum(),
        }
    }

    pub(crate) fn order_index(&self, order_id: &OrderId) -> Option<usize> {
        self.orders
            .iter()
            .position(|order| &order.order_id == order_id)
    }

    pub(crate) fn holding(&self, symbol: &str, market: MarketCode) -> Option<&HoldingRecord> {
        self.holdings
            .iter()
            .find(|holding| holding.symbol == symbol && holding.market == market)
    }

    /// 그 종목 · 시장의 열린 파는 주문에 묶인 수량.
    pub(crate) fn locked_quantity(&self, symbol: &str, market: MarketCode) -> Decimal {
        self.orders
            .iter()
            .filter(|order| {
                order.is_open()
                    && order.side == Side::Sell
                    && order.symbol == symbol
                    && order.market == market
            })
            .map(OrderRecord::remaining)
            .sum()
    }

    /// 그 종목 · 시장에서 팔 수 있는 수량.
    pub(crate) fn sellable(&self, symbol: &str, market: MarketCode) -> Decimal {
        let owned = self
            .holding(symbol, market)
            .map_or(Decimal::ZERO, |holding| holding.quantity);
        owned - self.locked_quantity(symbol, market)
    }

    /// 산 만큼 보유를 늘리고 평균 산 값을 다시 셈한다.
    pub(crate) fn add_holding(&mut self, fill: &FillRecord) {
        let found = self
            .holdings
            .iter_mut()
            .find(|holding| holding.symbol == fill.symbol && holding.market == fill.market);
        if let Some(holding) = found {
            let quantity = holding.quantity + fill.quantity;
            let cost = holding
                .average_purchase_price
                .checked_mul(holding.quantity)
                .and_then(|cost| cost.checked_add(fill.amount));
            if let Some(average) = cost.and_then(|cost| cost.checked_div(quantity)) {
                holding.average_purchase_price = average;
            }
            holding.quantity = quantity;
            return;
        }
        self.holdings.push(HoldingRecord {
            symbol: fill.symbol.clone(),
            market: fill.market,
            currency: fill.currency,
            quantity: fill.quantity,
            average_purchase_price: fill.price,
        });
        self.holdings
            .sort_by(|a, b| (&a.symbol, a.market).cmp(&(&b.symbol, b.market)));
    }

    /// 판 만큼 보유를 줄인다. 다 팔면 목록에서 뺀다.
    pub(crate) fn remove_holding(&mut self, symbol: &str, market: MarketCode, quantity: Decimal) {
        if let Some(holding) = self
            .holdings
            .iter_mut()
            .find(|holding| holding.symbol == symbol && holding.market == market)
        {
            holding.quantity -= quantity;
        }
        self.holdings
            .retain(|holding| holding.quantity > Decimal::ZERO);
    }

    /// 마지막으로 처리한 시세를 기억한다(그 종목 · 통화의 앞의 것을 바꾼다).
    pub(crate) fn remember_quote(&mut self, record: QuoteRecord) {
        let found = self
            .last_quotes
            .iter_mut()
            .find(|quote| quote.symbol == record.symbol && quote.currency == record.currency);
        if let Some(quote) = found {
            *quote = record;
            return;
        }
        self.last_quotes.push(record);
        self.last_quotes
            .sort_by(|a, b| (&a.symbol, a.currency).cmp(&(&b.symbol, b.currency)));
    }

    /// 그 종목 · 통화에서 마지막으로 처리한 시세.
    pub(crate) fn last_quote(&self, symbol: &str, currency: CashCode) -> Option<&QuoteRecord> {
        self.last_quotes
            .iter()
            .find(|quote| quote.symbol == symbol && quote.currency == currency)
    }

    /// 보유를 밖에 보여 주는 모습으로.
    pub(crate) fn holding_views(&self, symbol: Option<&Symbol>) -> Vec<Holding> {
        self.holdings
            .iter()
            .filter(|holding| symbol.is_none_or(|symbol| symbol.as_str() == holding.symbol))
            .map(|holding| Holding {
                symbol: Symbol::new(holding.symbol.clone()),
                market: holding.market.market(),
                currency: holding.currency.currency(),
                quantity: holding.quantity,
                average_purchase_price: holding.average_purchase_price,
            })
            .collect()
    }
}
