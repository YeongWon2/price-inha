//! 시연용 시세: 가짜 어댑터에 받을 때마다 조금씩 바뀌는 값을 넣는다. 바깥으로 요청을 보내지 않는다.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use async_trait::async_trait;
use chrono::NaiveDate;
use pi_market::{
    Clock, Currency, FakeAdapter, Market, MarketAdapter, MarketCalendar, MarketError, Quote, Symbol,
};
use rust_decimal::Decimal;

/// 시연용 어댑터. 달력은 정하지 않으므로(휴장) 시세 늦음을 알리지 않는다.
pub(crate) struct DemoAdapter {
    fake: FakeAdapter,
    market: Market,
    clock: Arc<dyn Clock>,
    /// 종목마다 지금 값(통화의 가장 작은 단위).
    minor: Mutex<HashMap<Symbol, i64>>,
    ticks: AtomicU64,
}

/// 가격 모양: 한 걸음의 크기 · 처음 값의 범위(걸음 수) · 소수 자리.
struct Shape {
    currency: Currency,
    step: i64,
    floor_steps: i64,
    span_steps: u64,
    scale: u32,
}

impl DemoAdapter {
    pub(crate) fn new(market: Market, clock: Arc<dyn Clock>) -> Self {
        Self {
            fake: FakeAdapter::new(market),
            market,
            clock,
            minor: Mutex::new(HashMap::new()),
            ticks: AtomicU64::new(0),
        }
    }

    fn shape(&self) -> Shape {
        match self.market {
            // 달러: 1센트씩, 50.00 ~ 499.99에서 시작.
            Market::Us => Shape {
                currency: Currency::Usd,
                step: 1,
                floor_steps: 5_000,
                span_steps: 45_000,
                scale: 2,
            },
            // 원: 100원씩, 10,000 ~ 99,900에서 시작.
            _ => Shape {
                currency: Currency::Krw,
                step: 100,
                floor_steps: 100,
                span_steps: 900,
                scale: 0,
            },
        }
    }

    /// 그 종목의 다음 값. 받을 때마다 반드시 바뀐다.
    fn next_price(&self, symbol: &Symbol, tick: u64, shape: &Shape) -> Decimal {
        let seed = fnv1a(symbol.as_str().as_bytes());
        let mut minor = self.minor.lock().unwrap_or_else(PoisonError::into_inner);
        let floor = shape.floor_steps * shape.step;
        let start = floor + i64::try_from(seed % shape.span_steps).unwrap_or(0) * shape.step;
        let value = minor.entry(symbol.clone()).or_insert(start);
        // -3 ~ +3 걸음. 0이면 한 걸음 올린다.
        let roll = i64::try_from(mix(seed ^ tick) % 7).unwrap_or(0) - 3;
        let steps = if roll == 0 { 1 } else { roll };
        let moved = value.saturating_add(steps * shape.step);
        // 바닥 아래로 내려가려 하면 반대로 움직인다(값은 늘 바뀐다).
        *value = if moved < floor {
            value.saturating_add(steps.abs() * shape.step)
        } else {
            moved
        };
        Decimal::new(*value, shape.scale)
    }
}

/// (7차 3번) 시연에 지켜볼 종목이 없을 때 쓰는 시연 종목 하나. 시연(가짜 시세)일 때만 쓰고
/// 저장된 종목 목록은 바꾸지 않는다(등록된 종목도 아니다).
pub(crate) fn demo_symbol(market: Market) -> Symbol {
    match market {
        Market::Us => Symbol::new("AAPL"),
        _ => Symbol::new("005930"),
    }
}

/// FNV-1a. 종목마다 다른 시작 값을 정하는 데만 쓴다.
fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

/// splitmix64의 섞기. 시연용 값의 오르내림을 정하는 데만 쓴다(보안과 무관).
fn mix(value: u64) -> u64 {
    let mut z = value.wrapping_add(0x9e37_79b9_7f4a_7c15);
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

#[async_trait]
impl MarketAdapter for DemoAdapter {
    fn market(&self) -> Market {
        self.market
    }

    async fn quotes(&self, symbols: &[Symbol]) -> Result<Vec<Quote>, MarketError> {
        let tick = self.ticks.fetch_add(1, Ordering::Relaxed);
        let now = self.clock.now();
        let shape = self.shape();
        for symbol in symbols {
            self.fake.set_quote(Quote {
                symbol: symbol.clone(),
                price: self.next_price(symbol, tick, &shape),
                currency: shape.currency.clone(),
                exchange_time: Some(now),
                received_at: now,
            });
        }
        self.fake.quotes(symbols).await
    }

    async fn calendar(&self, date: NaiveDate) -> Result<MarketCalendar, MarketError> {
        self.fake.calendar(date).await
    }
}

#[cfg(test)]
mod tests {
    use pi_market::SystemClock;

    use super::*;

    async fn prices(adapter: &DemoAdapter, symbols: &[Symbol]) -> Vec<Decimal> {
        let quotes = adapter.quotes(symbols).await.unwrap_or_default();
        quotes.into_iter().map(|quote| quote.price).collect()
    }

    #[tokio::test]
    async fn 받을_때마다_값이_바뀌고_바닥_아래로_내려가지_않는다() {
        let adapter = DemoAdapter::new(Market::Kr, Arc::new(SystemClock));
        let symbols = [Symbol::new("005930"), Symbol::new("000660")];
        let mut last = prices(&adapter, &symbols).await;
        assert_eq!(last.len(), 2);
        for _ in 0..2_000 {
            let now = prices(&adapter, &symbols).await;
            for (before, after) in last.iter().zip(&now) {
                assert_ne!(before, after, "받을 때마다 바뀐다");
                assert!(*after >= Decimal::from(10_000), "{after}");
                assert_eq!(after.scale(), 0, "원화는 소수 자리가 없다");
            }
            last = now;
        }
    }

    #[tokio::test]
    async fn 미국_시장은_달러와_센트로_준다() {
        let adapter = DemoAdapter::new(Market::Us, Arc::new(SystemClock));
        let quotes = adapter
            .quotes(&[Symbol::new("AAPL")])
            .await
            .unwrap_or_default();
        assert_eq!(quotes.len(), 1);
        assert_eq!(quotes[0].currency, Currency::Usd);
        assert_eq!(quotes[0].price.scale(), 2);
        assert!(quotes[0].price >= Decimal::new(4_997, 2));
    }
}
