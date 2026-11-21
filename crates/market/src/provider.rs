//! 공급자 ID · 자산 종류 · 시세 공급자 등록부 (결정 2026-11-21 2번).
//!
//! 공급자는 이름(공급자 ID)과 자기 키 칸을 선언한다. 같은 `(공급자, 시장)`을 다시 등록하면
//! 앞의 것을 바꾼다(Hermes `ProviderProfile`의 last-writer-wins, `docs/research/hermes-agent.md` 4.1).

use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

use crate::{Market, MarketAdapter, MarketError, Quote, Symbol};

/// 시세 공급자 ID. 새 공급자가 더해질 수 있어 `non_exhaustive`다.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ProviderId {
    /// 토스증권.
    Toss,
}

impl ProviderId {
    /// 공급자 이름. 토스는 `"toss"`.
    pub fn as_str(self) -> &'static str {
        match self {
            ProviderId::Toss => "toss",
        }
    }

    /// 이름으로 공급자를 찾는다. 소문자 그대로만 받는다(대문자 · 모르는 글자는 `None`).
    pub fn parse(value: &str) -> Option<ProviderId> {
        match value {
            "toss" => Some(ProviderId::Toss),
            _ => None,
        }
    }

    /// 공급자가 선언하는 자기 키 칸 이름들. 토스는 `["client_id", "client_secret"]`.
    /// 보관소 칸과 잇는 일은 조립 단계(server)에서 한다.
    pub fn credential_fields(self) -> &'static [&'static str] {
        match self {
            ProviderId::Toss => &["client_id", "client_secret"],
        }
    }
}

impl fmt::Display for ProviderId {
    /// [`ProviderId::as_str`]와 같다.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// 자산 종류. `Crypto`는 값으로만 두고 이번에는 어느 시장도 돌려주지 않는다.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum AssetClass {
    /// 주식.
    Stock,
    /// 코인.
    Crypto,
}

/// 시세 공급자 등록부. 공급자를 `(ProviderId, 시장)`으로 쥔다. 시장은 `adapter.market()`이다.
pub struct MarketRegistry {
    /// 등록된 공급자들. 등록 순서다. `(공급자, 시장)`은 하나씩만 있다.
    entries: Vec<Entry>,
    /// 시장별 기본 공급자.
    defaults: HashMap<Market, ProviderId>,
    /// 종목별 공급자 경로.
    routes: HashMap<(Market, Symbol), ProviderId>,
}

/// 등록된 공급자 하나.
struct Entry {
    provider: ProviderId,
    market: Market,
    adapter: Arc<dyn MarketAdapter>,
}

impl Default for MarketRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl MarketRegistry {
    /// 빈 등록부.
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            defaults: HashMap::new(),
            routes: HashMap::new(),
        }
    }

    /// 공급자를 등록한다. 시장은 `adapter.market()`으로 정한다.
    /// 같은 `(공급자, 시장)`을 다시 등록하면 앞의 것을 바꾼다. 등록 순서는 기억한다.
    pub fn register(&mut self, provider: ProviderId, adapter: Arc<dyn MarketAdapter>) {
        let market = adapter.market();
        match self
            .entries
            .iter_mut()
            .find(|entry| entry.provider == provider && entry.market == market)
        {
            // 앞의 것을 바꾸고 등록 순서의 자리는 그대로 둔다.
            Some(entry) => entry.adapter = adapter,
            None => self.entries.push(Entry {
                provider,
                market,
                adapter,
            }),
        }
    }

    /// 그 시장의 기본 공급자를 정한다.
    pub fn set_default(&mut self, market: Market, provider: ProviderId) {
        self.defaults.insert(market, provider);
    }

    /// 그 종목만 다른 공급자로 보낸다.
    pub fn route(&mut self, market: Market, symbol: Symbol, provider: ProviderId) {
        self.routes.insert((market, symbol), provider);
    }

    /// 종목의 공급자를 고른다. 순서: 종목 경로 → 시장 기본 → 그 시장에 처음 등록된 공급자.
    ///
    /// 하나도 없거나, 경로 · 기본이 등록되지 않은 공급자를 가리키면
    /// [`MarketError::NoProvider`]다. 다른 공급자로 몰래 넘어가지 않는다.
    pub fn select(
        &self,
        market: Market,
        symbol: &Symbol,
    ) -> Result<(ProviderId, Arc<dyn MarketAdapter>), MarketError> {
        let no_provider = MarketError::NoProvider { market };
        let chosen = self
            .routes
            .get(&(market, symbol.clone()))
            .or_else(|| self.defaults.get(&market))
            .copied();
        let entry = match chosen {
            // 경로 · 기본이 가리킨 공급자가 그 시장에 없으면 다른 공급자로 넘어가지 않는다.
            Some(provider) => self.entry(provider, market),
            None => self.entries.iter().find(|entry| entry.market == market),
        }
        .ok_or(no_provider)?;
        Ok((entry.provider, Arc::clone(&entry.adapter)))
    }

    /// 그 시장에 등록된 그 공급자.
    fn entry(&self, provider: ProviderId, market: Market) -> Option<&Entry> {
        self.entries
            .iter()
            .find(|entry| entry.provider == provider && entry.market == market)
    }

    /// 종목마다 공급자를 고르고 공급자별로 한 번씩 부른다.
    ///
    /// 결과는 호출한 쪽이 넘긴 종목 순서이고 같은 종목은 한 번이다. 공급자 하나라도
    /// 실패하면 전체가 그 오류다. 공급자를 고르지 못하면 요청을 보내지 않고 `NoProvider`다.
    pub async fn quotes(
        &self,
        market: Market,
        symbols: &[Symbol],
    ) -> Result<Vec<Quote>, MarketError> {
        // 같은 종목은 한 번만 다룬다(처음 나온 자리).
        let mut unique: Vec<&Symbol> = Vec::new();
        for symbol in symbols {
            if !unique.contains(&symbol) {
                unique.push(symbol);
            }
        }

        // 요청을 보내기 전에 모든 종목의 공급자를 고른다. 하나라도 못 고르면 아무도 부르지 않는다.
        let mut groups: Vec<(ProviderId, Arc<dyn MarketAdapter>, Vec<Symbol>)> = Vec::new();
        for symbol in &unique {
            let (provider, adapter) = self.select(market, symbol)?;
            match groups.iter_mut().find(|(id, _, _)| *id == provider) {
                Some((_, _, list)) => list.push((*symbol).clone()),
                None => groups.push((provider, adapter, vec![(*symbol).clone()])),
            }
        }

        let mut received: HashMap<Symbol, Quote> = HashMap::new();
        for (_, adapter, list) in &groups {
            for quote in adapter.quotes(list).await? {
                // 물은 종목의 것만 받는다. 같은 종목이 여럿이면 처음 것을 쓴다.
                if list.contains(&quote.symbol) {
                    received.entry(quote.symbol.clone()).or_insert(quote);
                }
            }
        }

        Ok(unique
            .into_iter()
            .filter_map(|symbol| received.remove(symbol))
            .collect())
    }

    /// 등록된 `(공급자, 시장)`들. 등록 순서.
    pub fn providers(&self) -> Vec<(ProviderId, Market)> {
        self.entries
            .iter()
            .map(|entry| (entry.provider, entry.market))
            .collect()
    }
}
