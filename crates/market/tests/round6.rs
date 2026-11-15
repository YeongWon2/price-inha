//! 6차 작업(docs/dev/modules/market.md "6차 작업" 1번).
//! 한 응답 안의 같은 종목: 같은 값(가격 · 통화 · 거래소 시각)이면 한 번으로 줄이고,
//! 하나라도 다르면 응답 모양 오류다.
//! 로컬 가짜 서버(wiremock)만 쓴다. 시간은 테스트가 정한다.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use pi_market::{
    Clock, CredentialSource, Credentials, Currency, Market, MarketAdapter, MarketError, Quote,
    Symbol, TossAdapter, TossClient, TossConfig,
};
use rust_decimal::Decimal;
use serde_json::{Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

const CLIENT_ID: &str = "시험용-클라이언트-아이디-0007";
const CLIENT_SECRET: &str = "시험용-클라이언트-비밀-0007";
const TOKEN_PATH: &str = "/oauth2/token";
const PRICES_PATH: &str = "/api/v1/prices";
const START: &str = "2026-03-25T10:00:00+09:00";
const STAMP: &str = "2026-03-25T09:30:00+09:00";
const OTHER_STAMP: &str = "2026-03-25T09:30:01+09:00";

fn symbols(list: &[&str]) -> Vec<Symbol> {
    list.iter().map(|s| Symbol::new(*s)).collect()
}

fn many(n: usize) -> Vec<Symbol> {
    (0..n).map(|i| Symbol::new(format!("S{i:04}"))).collect()
}

/// 테스트가 정하는 시계. `sleep`은 기다리지 않고 시각만 옮긴다.
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
        Some(Credentials::new(CLIENT_ID, CLIENT_SECRET))
    }
}

/// 토큰은 언제나 내주고, 현재가는 `respond`가 만든 목록으로 답하는 가짜 서버의 국내 어댑터.
async fn adapter_with(
    server: &MockServer,
    respond: impl Fn(&[String]) -> Vec<Value> + Send + Sync + 'static,
) -> TossAdapter {
    Mock::given(method("POST"))
        .and(path(TOKEN_PATH))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "access_token": "test-access-token-1",
            "token_type": "Bearer",
            "expires_in": 86400,
        })))
        .mount(server)
        .await;
    Mock::given(method("GET"))
        .and(path(PRICES_PATH))
        .respond_with(move |req: &Request| {
            let items = respond(&query_symbols(req));
            ResponseTemplate::new(200).set_body_json(json!({ "result": items }))
        })
        .mount(server)
        .await;
    let config = TossConfig {
        base_url: server.uri(),
        timeout: Duration::from_secs(5),
        token_refresh_margin: Duration::from_secs(60),
        rate_limit_retries: 2,
        ..TossConfig::default()
    };
    let clock = Arc::new(TestClock {
        now: Mutex::new(START.parse::<DateTime<Utc>>().unwrap()),
    });
    TossClient::new(config, Arc::new(TestCredentials), clock).adapter(Market::Kr)
}

fn item(symbol: &str, price: &str, currency: &str, stamp: &str) -> Value {
    json!({
        "symbol": symbol,
        "timestamp": stamp,
        "lastPrice": price,
        "currency": currency,
    })
}

fn price_item(symbol: &str, price: &str) -> Value {
    item(symbol, price, "KRW", STAMP)
}

fn query_symbols(req: &Request) -> Vec<String> {
    req.url
        .query_pairs()
        .find(|(k, _)| k == "symbols")
        .map(|(_, v)| v.into_owned())
        .unwrap_or_default()
        .split(',')
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

fn order(quotes: &[Quote]) -> Vec<String> {
    quotes
        .iter()
        .map(|q| q.symbol.as_str().to_string())
        .collect()
}

fn is_unexpected(err: &MarketError) -> bool {
    matches!(err, MarketError::UnexpectedResponse { .. })
}

// ───────────── 같은 값으로 두 번: 한 번으로 줄인다(오류가 아니다) ─────────────

// 검증: D5
#[tokio::test]
async fn 같은_종목이_같은_값으로_두_번_오면_결과에_한_번만_든다() {
    let server = MockServer::start().await;
    let adapter = adapter_with(&server, |wanted| {
        let mut items: Vec<Value> = wanted.iter().map(|s| price_item(s, "71500")).collect();
        items.push(price_item("005930", "71500"));
        items
    })
    .await;

    let quotes = adapter
        .quotes(&symbols(&["005930", "000660"]))
        .await
        .expect("같은 값의 겹침은 오류가 아니다");
    assert_eq!(order(&quotes), ["005930", "000660"]);
    assert_eq!(quotes[0].price, Decimal::from(71500));
    assert_eq!(quotes[0].currency, Currency::Krw);
    let stamp: DateTime<Utc> = STAMP.parse().unwrap();
    assert_eq!(quotes[0].exchange_time, Some(stamp));
}

// 검증: D5
#[tokio::test]
async fn 같은_종목이_같은_값으로_바로_이어_두_번_와도_한_번만_든다() {
    let server = MockServer::start().await;
    let adapter = adapter_with(&server, |wanted| {
        wanted
            .iter()
            .flat_map(|s| [price_item(s, "100"), price_item(s, "100")])
            .collect()
    })
    .await;

    let quotes = adapter
        .quotes(&symbols(&["A", "B", "C"]))
        .await
        .expect("같은 값의 겹침은 오류가 아니다");
    assert_eq!(order(&quotes), ["A", "B", "C"]);
}

// 검증: D5
#[tokio::test]
async fn 같은_값의_겹침은_여러_묶음에서도_한_번씩만_든다() {
    let server = MockServer::start().await;
    // 묶음마다 첫 종목을 같은 값으로 한 번 더 붙인다.
    let adapter = adapter_with(&server, |wanted| {
        let mut items: Vec<Value> = wanted.iter().map(|s| price_item(s, "100")).collect();
        if let Some(first) = wanted.first() {
            items.push(price_item(first, "100"));
        }
        items
    })
    .await;
    let wanted = many(250);

    let quotes = adapter.quotes(&wanted).await.unwrap();
    let expected: Vec<String> = wanted.iter().map(|s| s.as_str().to_string()).collect();
    assert_eq!(order(&quotes), expected);
}

// ───────────── 값이 하나라도 다르면 응답 모양 오류 ─────────────

// 검증: D5
#[tokio::test]
async fn 같은_종목이_시각만_다르게_두_번_오면_응답_모양_오류다() {
    let server = MockServer::start().await;
    let adapter = adapter_with(&server, |wanted| {
        let mut items: Vec<Value> = wanted.iter().map(|s| price_item(s, "100")).collect();
        items.push(item("005930", "100", "KRW", OTHER_STAMP));
        items
    })
    .await;

    let err = adapter
        .quotes(&symbols(&["005930", "000660"]))
        .await
        .unwrap_err();
    assert!(is_unexpected(&err), "{err:?}");
}

// 검증: D5
#[tokio::test]
async fn 같은_종목이_통화만_다르게_두_번_오면_응답_모양_오류다() {
    let server = MockServer::start().await;
    let adapter = adapter_with(&server, |wanted| {
        let mut items: Vec<Value> = wanted.iter().map(|s| price_item(s, "100")).collect();
        items.push(item("005930", "100", "USD", STAMP));
        items
    })
    .await;

    let err = adapter
        .quotes(&symbols(&["005930", "000660"]))
        .await
        .unwrap_err();
    assert!(is_unexpected(&err), "{err:?}");
}

// 검증: D5
#[tokio::test]
async fn 같은_종목이_가격만_다르게_두_번_오면_응답_모양_오류다() {
    let server = MockServer::start().await;
    let adapter = adapter_with(&server, |wanted| {
        let mut items: Vec<Value> = wanted.iter().map(|s| price_item(s, "100")).collect();
        items.push(price_item("005930", "101"));
        items
    })
    .await;

    let err = adapter
        .quotes(&symbols(&["005930", "000660"]))
        .await
        .unwrap_err();
    assert!(is_unexpected(&err), "{err:?}");
}
