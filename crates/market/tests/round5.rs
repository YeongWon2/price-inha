//! 5차 작업(docs/dev/modules/market.md "5차 작업").
//! 로컬 가짜 서버(wiremock)만 쓴다. 시간은 테스트가 정한다.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use pi_market::{
    Clock, CredentialSource, Credentials, Market, MarketAdapter, MarketError, Quote, Symbol,
    TossAdapter, TossClient, TossConfig,
};
use rust_decimal::Decimal;
use serde_json::{Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

const CLIENT_ID: &str = "시험용-클라이언트-아이디-0006";
const CLIENT_SECRET: &str = "시험용-클라이언트-비밀-0006";
const TOKEN_PATH: &str = "/oauth2/token";
const PRICES_PATH: &str = "/api/v1/prices";
const START: &str = "2026-03-25T10:00:00+09:00";
const STAMP: &str = "2026-03-25T09:30:00+09:00";

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

fn price_item(symbol: &str, price: &str) -> Value {
    json!({
        "symbol": symbol,
        "timestamp": STAMP,
        "lastPrice": price,
        "currency": "KRW",
    })
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

/// 서버가 받은 현재가 요청마다 물은 종목 목록.
async fn asked(server: &MockServer) -> Vec<Vec<String>> {
    server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .filter(|r| r.url.path() == PRICES_PATH)
        .map(query_symbols)
        .collect()
}

/// 종목마다 서로 다른 가격(앞에서부터 101, 102, …)을 매겨 순서를 뒤집어 돌려준다.
fn reversed_with_distinct_prices(wanted: &[String]) -> Vec<Value> {
    wanted
        .iter()
        .enumerate()
        .rev()
        .map(|(i, s)| price_item(s, &(101 + i).to_string()))
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

// ───────────── 1번: 결과는 요청 순서다 ─────────────

// 검증: D5
#[tokio::test]
async fn 서버가_순서를_뒤집어_돌려줘도_결과는_요청_순서다() {
    let server = MockServer::start().await;
    let adapter = adapter_with(&server, reversed_with_distinct_prices).await;

    let quotes = adapter
        .quotes(&symbols(&["005930", "000660", "035420"]))
        .await
        .unwrap();
    assert_eq!(order(&quotes), ["005930", "000660", "035420"]);
    // 순서만 바뀌고 종목과 값의 짝은 그대로다.
    let prices: Vec<Decimal> = quotes.iter().map(|q| q.price).collect();
    assert_eq!(
        prices,
        [Decimal::from(101), Decimal::from(102), Decimal::from(103)]
    );
}

// 검증: D5
#[tokio::test]
async fn 서버가_순서를_섞어_돌려줘도_결과는_요청_순서다() {
    let server = MockServer::start().await;
    // 홀수 자리를 먼저, 짝수 자리를 뒤에 돌려준다: [A, B, C, D, E] → [B, D, A, C, E].
    let adapter = adapter_with(&server, |wanted| {
        let odd = wanted.iter().skip(1).step_by(2);
        let even = wanted.iter().step_by(2);
        odd.chain(even).map(|s| price_item(s, "100")).collect()
    })
    .await;
    let wanted = symbols(&["A", "B", "C", "D", "E"]);

    let quotes = adapter.quotes(&wanted).await.unwrap();
    assert_eq!(order(&quotes), ["A", "B", "C", "D", "E"]);
}

// 검증: D5
#[tokio::test]
async fn 여러_묶음에서_서버가_순서를_뒤집어도_결과는_요청_순서다() {
    let server = MockServer::start().await;
    let adapter = adapter_with(&server, reversed_with_distinct_prices).await;
    let wanted = many(250);

    let quotes = adapter.quotes(&wanted).await.unwrap();
    assert_eq!(asked(&server).await.len(), 2);
    let expected: Vec<String> = wanted.iter().map(|s| s.as_str().to_string()).collect();
    assert_eq!(order(&quotes), expected);
}

// ───────────── 2번: 한 묶음 안의 같은 종목 ─────────────

// 검증: D5
#[tokio::test]
async fn 한_묶음에_같은_종목이_다른_값으로_두_번_오면_응답_모양_오류다() {
    let server = MockServer::start().await;
    let adapter = adapter_with(&server, |wanted| {
        let mut items: Vec<Value> = wanted.iter().map(|s| price_item(s, "100")).collect();
        items.push(price_item("005930", "200"));
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
async fn 한_묶음에_같은_종목이_같은_값으로_두_번_와도_결과에_두_번_들어가지_않는다() {
    // 같은 값의 겹침을 받아 한 번으로 줄일지, 응답 모양 오류로 볼지는 지시서가 정하지 않았다.
    // 어느 쪽이든 결과에 한 종목이 두 번 들어가면 안 된다.
    let server = MockServer::start().await;
    let adapter = adapter_with(&server, |wanted| {
        let mut items: Vec<Value> = wanted.iter().map(|s| price_item(s, "100")).collect();
        items.push(price_item("005930", "100"));
        items
    })
    .await;

    match adapter.quotes(&symbols(&["005930", "000660"])).await {
        Ok(quotes) => assert_eq!(order(&quotes), ["005930", "000660"]),
        Err(err) => assert!(is_unexpected(&err), "{err:?}"),
    }
}

// 검증: D5
#[tokio::test]
async fn 호출한_쪽이_같은_종목을_두_번_넘기면_한_번만_묻고_결과에도_한_번이다() {
    let server = MockServer::start().await;
    let adapter = adapter_with(&server, |wanted| {
        wanted.iter().map(|s| price_item(s, "100")).collect()
    })
    .await;

    let quotes = adapter
        .quotes(&symbols(&["005930", "000660", "005930"]))
        .await
        .unwrap();
    assert_eq!(asked(&server).await, [vec!["005930", "000660"]]);
    assert_eq!(order(&quotes), ["005930", "000660"]);
}

// 검증: D5
#[tokio::test]
async fn 같은_종목을_거듭_넘겨도_서버가_섞어_답하면_처음_나온_순서로_한_번씩이다() {
    let server = MockServer::start().await;
    let adapter = adapter_with(&server, reversed_with_distinct_prices).await;

    let quotes = adapter
        .quotes(&symbols(&["B", "A", "B", "C", "A"]))
        .await
        .unwrap();
    let asked = asked(&server).await;
    assert_eq!(asked.len(), 1);
    assert_eq!(asked[0], ["B", "A", "C"]);
    assert_eq!(order(&quotes), ["B", "A", "C"]);
}

// 검증: D5
#[tokio::test]
async fn 겹친_종목을_빼면_한_묶음에_들어가는_호출은_한_번만_부른다() {
    // 200개 + 앞의 것과 같은 1개 = 겹침을 빼면 200개라 묶음 하나다.
    let server = MockServer::start().await;
    let adapter = adapter_with(&server, |wanted| {
        wanted.iter().map(|s| price_item(s, "100")).collect()
    })
    .await;
    let mut wanted = many(200);
    wanted.push(Symbol::new("S0000"));

    let quotes = adapter.quotes(&wanted).await.unwrap();
    let asked = asked(&server).await;
    assert_eq!(asked.len(), 1);
    assert_eq!(asked[0].len(), 200);
    assert_eq!(quotes.len(), 200);
    assert_eq!(quotes[0].symbol, Symbol::new("S0000"));
}

// 검증: D5
#[tokio::test]
async fn 묶음을_넘어_겹친_종목도_한_번만_묻는다() {
    // 250개 뒤에 첫 묶음의 종목을 다시 넘기면, 겹침을 뺀 250개를 두 묶음으로 나눠 묻는다.
    let server = MockServer::start().await;
    let adapter = adapter_with(&server, |wanted| {
        wanted.iter().map(|s| price_item(s, "100")).collect()
    })
    .await;
    let mut wanted = many(250);
    wanted.push(Symbol::new("S0005"));
    wanted.push(Symbol::new("S0240"));

    let quotes = adapter.quotes(&wanted).await.unwrap();
    let asked = asked(&server).await;
    assert_eq!(asked.len(), 2);
    let mut all: Vec<String> = asked.into_iter().flatten().collect();
    assert_eq!(all.len(), 250, "한 종목을 두 번 물으면 안 된다");
    all.sort();
    all.dedup();
    assert_eq!(all.len(), 250);
    let expected: Vec<String> = many(250).iter().map(|s| s.as_str().to_string()).collect();
    assert_eq!(order(&quotes), expected);
}
