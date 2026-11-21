//! 공급자 ID · 자산 종류 · 시세 공급자 등록부 (결정 2026-11-21 2번).
//!
//! 공급자는 이름(공급자 ID)과 자기 키 칸을 선언한다. 같은 `(공급자, 시장)`을 다시 등록하면
//! 앞의 것을 바꾼다(Hermes `ProviderProfile`의 last-writer-wins, `docs/research/hermes-agent.md` 4.1).

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
        todo!()
    }

    /// 이름으로 공급자를 찾는다. 소문자 그대로만 받는다(대문자 · 모르는 글자는 `None`).
    pub fn parse(value: &str) -> Option<ProviderId> {
        todo!()
    }

    /// 공급자가 선언하는 자기 키 칸 이름들. 토스는 `["client_id", "client_secret"]`.
    /// 보관소 칸과 잇는 일은 조립 단계(server)에서 한다.
    pub fn credential_fields(self) -> &'static [&'static str] {
        todo!()
    }
}

impl fmt::Display for ProviderId {
    /// [`ProviderId::as_str`]와 같다.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        todo!()
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
    _private: (),
}

impl Default for MarketRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl MarketRegistry {
    /// 빈 등록부.
    pub fn new() -> Self {
        todo!()
    }

    /// 공급자를 등록한다. 시장은 `adapter.market()`으로 정한다.
    /// 같은 `(공급자, 시장)`을 다시 등록하면 앞의 것을 바꾼다. 등록 순서는 기억한다.
    pub fn register(&mut self, provider: ProviderId, adapter: Arc<dyn MarketAdapter>) {
        todo!()
    }

    /// 그 시장의 기본 공급자를 정한다.
    pub fn set_default(&mut self, market: Market, provider: ProviderId) {
        todo!()
    }

    /// 그 종목만 다른 공급자로 보낸다.
    pub fn route(&mut self, market: Market, symbol: Symbol, provider: ProviderId) {
        todo!()
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
        todo!()
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
        todo!()
    }

    /// 등록된 `(공급자, 시장)`들. 등록 순서.
    pub fn providers(&self) -> Vec<(ProviderId, Market)> {
        todo!()
    }
}
