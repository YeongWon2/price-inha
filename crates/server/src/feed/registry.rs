//! (10차 10b 11번) 시세는 공급자 등록부(`pi_market::MarketRegistry`, market 7차)를 거친다. 지금은 토스 하나를
//! 그 시장의 기본 공급자로 등록한다(결정 2026-11-21 2번: 새 공급자는 어댑터 하나와 등록 한 줄로 붙는다).
//!
//! 시세 흐름은 어댑터 하나를 받으므로, 등록부를 어댑터 모양으로 감싼다: 시세는 등록부가 종목마다 고른 공급자에게,
//! 달력은 그 시장의 기본 공급자에게 묻는다. 고를 공급자가 없으면 묻지 않고 `NoProvider`다(몰래 넘어가지 않는다).

use std::sync::Arc;

use async_trait::async_trait;
use chrono::NaiveDate;
use pi_market::{
    Market, MarketAdapter, MarketCalendar, MarketError, MarketRegistry, ProviderId, Quote, Symbol,
};

/// 등록부를 감싼 시세 어댑터.
pub(crate) struct Registered {
    market: Market,
    registry: MarketRegistry,
    /// 달력을 묻는 그 시장의 기본 공급자.
    calendar: Arc<dyn MarketAdapter>,
}

impl Registered {
    /// 그 공급자 하나를 그 시장의 기본으로 등록한다.
    pub(crate) fn single(provider: ProviderId, adapter: Arc<dyn MarketAdapter>) -> Registered {
        let market = adapter.market();
        let mut registry = MarketRegistry::new();
        registry.register(provider, Arc::clone(&adapter));
        registry.set_default(market, provider);
        Registered {
            market,
            registry,
            calendar: adapter,
        }
    }
}

#[async_trait]
impl MarketAdapter for Registered {
    fn market(&self) -> Market {
        self.market
    }

    async fn quotes(&self, symbols: &[Symbol]) -> Result<Vec<Quote>, MarketError> {
        if symbols.is_empty() {
            return Ok(Vec::new());
        }
        self.registry.quotes(self.market, symbols).await
    }

    async fn calendar(&self, date: NaiveDate) -> Result<MarketCalendar, MarketError> {
        self.calendar.calendar(date).await
    }
}
