//! 7차 작업(docs/dev/modules/market.md "7차 작업" 1 · 2 · 3번, 결정 2026-11-21 2 · 5번).
//! 공급자 ID · 자산 종류 · 시세 공급자 등록부 · 공급자가 주는 거래 상태.
//! 가짜 어댑터와 로컬 가짜 서버(wiremock)만 쓴다. 시간은 테스트가 정한다.
//!
//! 옛 공개 API가 그대로 컴파일되는지(7차 "테스트로 확인할 것" 10번)도 여기서 본다:
//! `Quote` 구조체 글자, `FakeAdapter::new`, 새 메서드를 구현하지 않은 `MarketAdapter` 구현.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use pi_market::{
    AssetClass, Clock, CredentialSource, Credentials, Currency, FakeAdapter, Market, MarketAdapter,
    MarketCalendar, MarketError, MarketRegistry, ProviderId, Quote, Symbol, TossClient, TossConfig,
    TradingDay, TradingStatus,
};
use rust_decimal::Decimal;
use wiremock::MockServer;

const NOW: &str = "2026-03-25T02:00:00Z";

fn now() -> DateTime<Utc> {
    NOW.parse().unwrap()
}

fn sym(text: &str) -> Symbol {
    Symbol::new(text)
}

fn syms(list: &[&str]) -> Vec<Symbol> {
    list.iter().map(|s| sym(s)).collect()
}

/// 옛 모양 그대로의 `Quote` 구조체 글자.
fn quote(symbol: &str, price: &str) -> Quote {
    Quote {
        symbol: sym(symbol),
        price: price.parse::<Decimal>().unwrap(),
        currency: Currency::Krw,
        exchange_time: None,
        received_at: now(),
    }
}

fn result_symbols(quotes: &[Quote]) -> Vec<String> {
    quotes
        .iter()
        .map(|q| q.symbol.as_str().to_string())
        .collect()
}

/// 국내 가짜 어댑터. 종목마다 시세를 정해 둔다.
fn kr_fake(list: &[(&str, &str)]) -> Arc<FakeAdapter> {
    let fake = Arc::new(FakeAdapter::new(Market::Kr));
    for (symbol, price) in list {
        fake.set_quote(quote(symbol, price));
    }
    fake
}

/// 새 메서드(`trading_statuses`)를 구현하지 않은 옛 모양의 어댑터.
/// 받은 요청을 적어 두고, 시세는 넘긴 순서를 **거꾸로** 돌려준다(등록부가 순서를 맞추는지 본다).
struct LegacyAdapter {
    market: Market,
    calls: AtomicUsize,
    requested: Mutex<Vec<Vec<Symbol>>>,
}

impl LegacyAdapter {
    fn new(market: Market) -> Self {
        Self {
            market,
            calls: AtomicUsize::new(0),
            requested: Mutex::new(Vec::new()),
        }
    }

    fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl MarketAdapter for LegacyAdapter {
    fn market(&self) -> Market {
        self.market
    }

    async fn quotes(&self, symbols: &[Symbol]) -> Result<Vec<Quote>, MarketError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.requested.lock().unwrap().push(symbols.to_vec());
        Ok(symbols
            .iter()
            .rev()
            .map(|s| quote(s.as_str(), "1000"))
            .collect())
    }

    async fn calendar(&self, date: NaiveDate) -> Result<MarketCalendar, MarketError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let closed = || TradingDay {
            date,
            sessions: Vec::new(),
        };
        Ok(MarketCalendar {
            market: self.market,
            today: closed(),
            previous_business_day: closed(),
            next_business_day: closed(),
        })
    }
}

// ---------------------------------------------------------------- 1. 공급자 ID · 자산 종류

// 검증: D5
#[test]
fn 토스_공급자_이름은_toss다() {
    assert_eq!(ProviderId::Toss.as_str(), "toss");
    assert_eq!(ProviderId::Toss.to_string(), "toss");
    assert_eq!(ProviderId::parse("toss"), Some(ProviderId::Toss));
}

// 검증: D5
#[test]
fn 모르는_공급자_이름과_대문자는_받지_않는다() {
    for text in ["Toss", "TOSS", " toss", "toss ", "", "upbit", "kis", "토스"] {
        assert_eq!(ProviderId::parse(text), None, "{text:?}");
    }
}

// 검증: D5, V31
#[test]
fn 토스는_자기_키_칸_둘을_선언한다() {
    assert_eq!(
        ProviderId::Toss.credential_fields(),
        &["client_id", "client_secret"]
    );
}

// 검증: D5
#[test]
fn 국내와_미국_시장은_주식이다() {
    assert_eq!(Market::Kr.asset_class(), AssetClass::Stock);
    assert_eq!(Market::Us.asset_class(), AssetClass::Stock);
    assert_ne!(AssetClass::Stock, AssetClass::Crypto);
}

// ---------------------------------------------------------------- 2. 등록부: 고르기

// 검증: D5
#[tokio::test]
async fn 등록부는_시장_기본_공급자를_고른다() {
    let fake = kr_fake(&[("005930", "70000")]);
    let mut registry = MarketRegistry::new();
    registry.register(ProviderId::Toss, fake.clone());
    registry.set_default(Market::Kr, ProviderId::Toss);

    let (provider, adapter) = registry.select(Market::Kr, &sym("005930")).unwrap();
    assert_eq!(provider, ProviderId::Toss);
    assert_eq!(adapter.market(), Market::Kr);
    let got = adapter.quotes(&syms(&["005930"])).await.unwrap();
    assert_eq!(result_symbols(&got), ["005930"]);
    assert_eq!(fake.quote_calls(), 1, "고른 어댑터가 등록한 그 어댑터다");
}

// 검증: D5
#[tokio::test]
async fn 기본이_없으면_그_시장에_처음_등록된_공급자를_고른다() {
    let fake = kr_fake(&[("005930", "70000")]);
    let mut registry = MarketRegistry::new();
    registry.register(ProviderId::Toss, fake.clone());

    let (provider, adapter) = registry.select(Market::Kr, &sym("005930")).unwrap();
    assert_eq!(provider, ProviderId::Toss);
    adapter.quotes(&syms(&["005930"])).await.unwrap();
    assert_eq!(fake.quote_calls(), 1);
}

// 검증: D5
#[tokio::test]
async fn 종목_경로로_그_종목의_공급자를_고른다() {
    let fake = kr_fake(&[("005930", "70000")]);
    let mut registry = MarketRegistry::new();
    registry.register(ProviderId::Toss, fake.clone());
    registry.route(Market::Kr, sym("005930"), ProviderId::Toss);

    let (provider, adapter) = registry.select(Market::Kr, &sym("005930")).unwrap();
    assert_eq!(provider, ProviderId::Toss);
    adapter.quotes(&syms(&["005930"])).await.unwrap();
    assert_eq!(fake.quote_calls(), 1);
}

// 검증: D5
#[tokio::test]
async fn 같은_공급자와_시장을_다시_등록하면_앞의_것을_바꾼다() {
    let first = kr_fake(&[("005930", "70000")]);
    let second = kr_fake(&[("005930", "71000")]);
    let mut registry = MarketRegistry::new();
    registry.register(ProviderId::Toss, first.clone());
    registry.register(ProviderId::Toss, second.clone());

    assert_eq!(registry.providers(), vec![(ProviderId::Toss, Market::Kr)]);
    let got = registry
        .quotes(Market::Kr, &syms(&["005930"]))
        .await
        .unwrap();
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].price, "71000".parse::<Decimal>().unwrap());
    assert_eq!(first.quote_calls(), 0, "바뀐 앞의 어댑터는 불리지 않는다");
    assert_eq!(second.quote_calls(), 1);
}

// 검증: D5
#[test]
fn 등록부는_등록_순서를_기억한다() {
    let mut registry = MarketRegistry::new();
    assert!(registry.providers().is_empty());
    registry.register(ProviderId::Toss, Arc::new(FakeAdapter::new(Market::Us)));
    registry.register(ProviderId::Toss, Arc::new(FakeAdapter::new(Market::Kr)));
    assert_eq!(
        registry.providers(),
        vec![
            (ProviderId::Toss, Market::Us),
            (ProviderId::Toss, Market::Kr)
        ]
    );
}

// 검증: D5
#[test]
fn 시장은_어댑터가_말하는_시장으로_정한다() {
    let mut registry = MarketRegistry::new();
    registry.register(ProviderId::Toss, Arc::new(FakeAdapter::new(Market::Us)));
    assert!(registry.select(Market::Us, &sym("AAPL")).is_ok());
    assert_eq!(
        registry.select(Market::Kr, &sym("005930")).err(),
        Some(MarketError::NoProvider { market: Market::Kr })
    );
}

// 검증: D5, P12
#[tokio::test]
async fn 공급자가_없는_시장은_고르지_못하고_아무도_부르지_않는다() {
    let us = Arc::new(FakeAdapter::new(Market::Us));
    let mut registry = MarketRegistry::new();
    registry.register(ProviderId::Toss, us.clone());

    assert_eq!(
        registry.select(Market::Kr, &sym("005930")).err(),
        Some(MarketError::NoProvider { market: Market::Kr })
    );
    assert_eq!(
        registry.quotes(Market::Kr, &syms(&["005930"])).await.err(),
        Some(MarketError::NoProvider { market: Market::Kr })
    );
    assert_eq!(us.quote_calls(), 0, "다른 시장의 공급자로 넘어가지 않는다");
}

// 검증: D5, P12
#[tokio::test]
async fn 빈_등록부는_고르지_못한다() {
    let registry = MarketRegistry::new();
    assert_eq!(
        registry.select(Market::Us, &sym("AAPL")).err(),
        Some(MarketError::NoProvider { market: Market::Us })
    );
    assert_eq!(
        registry.quotes(Market::Us, &syms(&["AAPL"])).await.err(),
        Some(MarketError::NoProvider { market: Market::Us })
    );
}

// 검증: D5, P12
#[tokio::test]
async fn 등록되지_않은_공급자를_가리키는_기본은_고르지_못한다() {
    // 토스는 미국에만 등록됐다. 국내 기본을 토스로 정해도 국내에는 토스가 없다.
    let us = Arc::new(FakeAdapter::new(Market::Us));
    let mut registry = MarketRegistry::new();
    registry.register(ProviderId::Toss, us.clone());
    registry.set_default(Market::Kr, ProviderId::Toss);

    assert_eq!(
        registry.select(Market::Kr, &sym("005930")).err(),
        Some(MarketError::NoProvider { market: Market::Kr })
    );
    assert_eq!(
        registry.quotes(Market::Kr, &syms(&["005930"])).await.err(),
        Some(MarketError::NoProvider { market: Market::Kr })
    );
    assert_eq!(us.quote_calls(), 0);
}

// 검증: D5, P12
#[tokio::test]
async fn 등록되지_않은_공급자를_가리키는_종목_경로는_고르지_못한다() {
    let us = Arc::new(FakeAdapter::new(Market::Us));
    let mut registry = MarketRegistry::new();
    registry.register(ProviderId::Toss, us.clone());
    registry.route(Market::Kr, sym("005930"), ProviderId::Toss);

    assert_eq!(
        registry.select(Market::Kr, &sym("005930")).err(),
        Some(MarketError::NoProvider { market: Market::Kr })
    );
    assert_eq!(
        registry.quotes(Market::Kr, &syms(&["005930"])).await.err(),
        Some(MarketError::NoProvider { market: Market::Kr })
    );
    assert_eq!(us.quote_calls(), 0);
}

// ---------------------------------------------------------------- 2. 등록부: 시세

// 검증: D5
#[tokio::test]
async fn 등록부_시세는_요청_순서이고_같은_종목은_한_번이다() {
    let legacy = Arc::new(LegacyAdapter::new(Market::Kr));
    let mut registry = MarketRegistry::new();
    registry.register(ProviderId::Toss, legacy.clone());

    let got = registry
        .quotes(Market::Kr, &syms(&["000660", "005930", "000660", "035420"]))
        .await
        .unwrap();
    assert_eq!(result_symbols(&got), ["000660", "005930", "035420"]);
    assert_eq!(legacy.calls(), 1, "공급자 하나는 한 번만 부른다");
    let requested = legacy.requested.lock().unwrap().clone();
    assert_eq!(requested.len(), 1);
    let mut asked: Vec<String> = requested[0]
        .iter()
        .map(|s| s.as_str().to_string())
        .collect();
    asked.sort();
    assert_eq!(
        asked,
        ["000660", "005930", "035420"],
        "같은 종목은 한 번만 묻는다"
    );
}

// 검증: D5, P12
#[tokio::test]
async fn 공급자가_실패하면_등록부_시세_전체가_그_오류다() {
    let fake = kr_fake(&[("005930", "70000")]);
    fake.fail_quotes(Some(MarketError::Unavailable {
        status: 503,
        code: Some("maintenance".to_string()),
    }));
    let mut registry = MarketRegistry::new();
    registry.register(ProviderId::Toss, fake.clone());

    assert_eq!(
        registry.quotes(Market::Kr, &syms(&["005930"])).await.err(),
        Some(MarketError::Unavailable {
            status: 503,
            code: Some("maintenance".to_string()),
        })
    );
}

// ---------------------------------------------------------------- 3. 공급자가 주는 상태

// 검증: D5, P12
#[tokio::test]
async fn 상태를_구현하지_않은_어댑터는_상태를_주지_않고_요청도_없다() {
    let legacy = LegacyAdapter::new(Market::Kr);
    let got = legacy
        .trading_statuses(&syms(&["005930", "000660"]))
        .await
        .unwrap();
    assert_eq!(got, None);
    assert_eq!(legacy.calls(), 0, "기본 구현은 요청을 보내지 않는다");
}

struct TestClock {
    now: Mutex<DateTime<Utc>>,
}

#[async_trait]
impl Clock for TestClock {
    fn now(&self) -> DateTime<Utc> {
        *self.now.lock().unwrap()
    }

    async fn sleep(&self, duration: Duration) {
        let mut now = self.now.lock().unwrap();
        *now += chrono::Duration::from_std(duration).unwrap();
    }
}

struct TestCredentials;

impl CredentialSource for TestCredentials {
    fn credentials(&self) -> Option<Credentials> {
        Some(Credentials::new(
            "시험용-클라이언트-아이디-0008",
            "시험용-클라이언트-비밀-0008",
        ))
    }
}

// 검증: D5, P12
#[tokio::test]
async fn 토스_어댑터는_상태를_주지_않고_가짜_서버에_요청이_없다() {
    let server = MockServer::start().await;
    let config = TossConfig {
        base_url: server.uri(),
        timeout: Duration::from_secs(5),
        ..TossConfig::default()
    };
    let clock = Arc::new(TestClock {
        now: Mutex::new(now()),
    });
    let client = TossClient::new(config, Arc::new(TestCredentials), clock);
    for market in [Market::Kr, Market::Us] {
        let got = client
            .adapter(market)
            .trading_statuses(&syms(&["005930"]))
            .await
            .unwrap();
        assert_eq!(got, None);
    }
    assert!(server.received_requests().await.unwrap().is_empty());
}

// 검증: D5, P12
#[tokio::test]
async fn 가짜_어댑터는_상태를_정하지_않으면_주지_않는다() {
    let fake = kr_fake(&[("005930", "70000")]);
    let got = fake.trading_statuses(&syms(&["005930"])).await.unwrap();
    assert_eq!(got, None);
}

// 검증: D5, P12
#[tokio::test]
async fn 가짜_어댑터는_정한_종목의_상태만_넘긴_순서로_준다() {
    let fake = FakeAdapter::new(Market::Kr);
    fake.set_status(sym("005930"), TradingStatus::Vi);
    fake.set_status(sym("000660"), TradingStatus::Halted);

    let got = fake
        .trading_statuses(&syms(&["000660", "035420", "005930"]))
        .await
        .unwrap();
    assert_eq!(
        got,
        Some(vec![
            (sym("000660"), TradingStatus::Halted),
            (sym("005930"), TradingStatus::Vi),
        ])
    );
}
