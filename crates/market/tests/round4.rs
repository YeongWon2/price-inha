//! 4차 작업(docs/dev/modules/market.md "4차 작업").
//! 로컬 가짜 서버(wiremock)만 쓴다. 시간은 테스트가 정한다.

use std::collections::HashMap;
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

const CLIENT_ID: &str = "시험용-클라이언트-아이디-0005";
const CLIENT_SECRET: &str = "시험용-클라이언트-비밀-0005";
const TOKEN_1: &str = "test-access-token-1";
const TOKEN_PATH: &str = "/oauth2/token";
const PRICES_PATH: &str = "/api/v1/prices";
const START: &str = "2026-03-25T10:00:00+09:00";

fn symbols(list: &[&str]) -> Vec<Symbol> {
    list.iter().map(|s| Symbol::new(*s)).collect()
}

/// 테스트가 정하는 시계. `sleep`은 기다리지 않고 시각만 옮긴다.
struct TestClock {
    now: Mutex<DateTime<Utc>>,
}

impl TestClock {
    fn at(text: &str) -> Arc<Self> {
        Arc::new(Self {
            now: Mutex::new(text.parse::<DateTime<Utc>>().unwrap()),
        })
    }
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

fn toss(server: &MockServer, cooldown: Duration) -> TossClient {
    let config = TossConfig {
        base_url: server.uri(),
        timeout: Duration::from_secs(5),
        token_refresh_margin: Duration::from_secs(60),
        rate_limit_retries: 2,
        token_failure_cooldown: cooldown,
        ..TossConfig::default()
    };
    TossClient::new(config, Arc::new(TestCredentials), TestClock::at(START))
}

fn token_ok() -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(json!({
        "access_token": TOKEN_1,
        "token_type": "Bearer",
        "expires_in": 86400,
    }))
}

fn oauth_error(status: u16, error: &str, description: &str) -> ResponseTemplate {
    ResponseTemplate::new(status)
        .set_body_json(json!({ "error": error, "error_description": description }))
}

async fn mount_token(server: &MockServer, response: ResponseTemplate, times: Option<u64>) {
    let mock = Mock::given(method("POST"))
        .and(path(TOKEN_PATH))
        .respond_with(response);
    let mock = match times {
        Some(n) => mock.up_to_n_times(n),
        None => mock,
    };
    mock.mount(server).await;
}

fn price_item(symbol: &str, price: &str, timestamp: &str) -> Value {
    json!({
        "symbol": symbol,
        "timestamp": timestamp,
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

/// 달라는 종목마다 100원짜리 시세를 돌려주고, 끝에 `extra`를 덧붙인다.
async fn mount_echo_prices(server: &MockServer, extra: fn(&[String]) -> Vec<Value>) {
    Mock::given(method("GET"))
        .and(path(PRICES_PATH))
        .respond_with(move |req: &Request| {
            let wanted = query_symbols(req);
            let mut items: Vec<Value> = wanted
                .iter()
                .map(|s| price_item(s, "100", "2026-03-25T09:30:00+09:00"))
                .collect();
            items.extend(extra(&wanted));
            ResponseTemplate::new(200).set_body_json(json!({ "result": items }))
        })
        .mount(server)
        .await;
}

async fn count(server: &MockServer, wanted: &str) -> usize {
    server
        .received_requests()
        .await
        .unwrap()
        .into_iter()
        .filter(|r| r.url.path() == wanted)
        .count()
}

fn is_unexpected(err: &MarketError) -> bool {
    matches!(err, MarketError::UnexpectedResponse { .. })
}

// ───────────── 1번: 요청하지 않은 종목의 읽히지 않는 가격 · 시각 ─────────────

// 검증: D5
#[tokio::test]
async fn 요청하지_않은_종목이라도_가격이_읽히지_않으면_응답_모양_오류다() {
    let server = MockServer::start().await;
    mount_token(&server, token_ok(), None).await;
    mount_echo_prices(&server, |_| {
        vec![price_item("NOTASKED", "abc", "2026-03-25T09:30:00+09:00")]
    })
    .await;
    let adapter = toss(&server, Duration::from_secs(60)).adapter(Market::Kr);

    let err = adapter.quotes(&symbols(&["005930"])).await.unwrap_err();
    assert!(is_unexpected(&err), "{err:?}");
}

// 검증: D5
#[tokio::test]
async fn 요청하지_않은_종목의_가격이_빈_글자여도_응답_모양_오류다() {
    let server = MockServer::start().await;
    mount_token(&server, token_ok(), None).await;
    mount_echo_prices(&server, |_| {
        vec![price_item("NOTASKED", "", "2026-03-25T09:30:00+09:00")]
    })
    .await;
    let adapter = toss(&server, Duration::from_secs(60)).adapter(Market::Kr);

    let err = adapter.quotes(&symbols(&["005930"])).await.unwrap_err();
    assert!(is_unexpected(&err), "{err:?}");
}

// 검증: D5
#[tokio::test]
async fn 요청하지_않은_종목의_시각이_읽히지_않으면_버리고_오류가_아니다() {
    // 시각 칸은 버린 뒤에 보므로, 가격과 달리 요청하지 않은 종목의 것은 오류가 아니다.
    let server = MockServer::start().await;
    mount_token(&server, token_ok(), None).await;
    mount_echo_prices(&server, |_| {
        vec![price_item("NOTASKED", "100", "어제 아침")]
    })
    .await;
    let adapter = toss(&server, Duration::from_secs(60)).adapter(Market::Kr);

    let quotes = adapter.quotes(&symbols(&["005930"])).await.unwrap();
    assert_eq!(quotes.len(), 1);
    assert_eq!(quotes[0].symbol, Symbol::new("005930"));
}

// 검증: D5
#[tokio::test]
async fn 요청한_종목의_시각이_읽히지_않으면_응답_모양_오류다() {
    let server = MockServer::start().await;
    mount_token(&server, token_ok(), None).await;
    Mock::given(method("GET"))
        .and(path(PRICES_PATH))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "result": [price_item("005930", "72000", "어제 아침")]
        })))
        .mount(&server)
        .await;
    let adapter = toss(&server, Duration::from_secs(60)).adapter(Market::Kr);

    let err = adapter.quotes(&symbols(&["005930"])).await.unwrap_err();
    assert!(is_unexpected(&err), "{err:?}");
}

// ───────────── 2번: 묶음별 요청 종목 ─────────────

fn many(n: usize) -> Vec<Symbol> {
    (0..n).map(|i| Symbol::new(format!("S{i:04}"))).collect()
}

fn per_symbol(quotes: &[Quote]) -> HashMap<String, Vec<Decimal>> {
    let mut seen: HashMap<String, Vec<Decimal>> = HashMap::new();
    for quote in quotes {
        seen.entry(quote.symbol.as_str().to_string())
            .or_default()
            .push(quote.price);
    }
    seen
}

/// 첫 묶음(S0000으로 시작)에는 둘째 묶음의 S0249를, 둘째 묶음에는 첫 묶음의 S0000을
/// 999원으로 섞어 보낸다.
fn other_chunk_symbol(wanted: &[String]) -> Vec<Value> {
    let stray = if wanted.iter().any(|s| s == "S0000") {
        "S0249"
    } else {
        "S0000"
    };
    vec![price_item(stray, "999", "2026-03-25T09:30:00+09:00")]
}

// 검증: D5
#[tokio::test]
async fn 다른_묶음의_종목이_섞여_와도_한_종목은_결과에_한_번만_들어간다() {
    let server = MockServer::start().await;
    mount_token(&server, token_ok(), None).await;
    mount_echo_prices(&server, other_chunk_symbol).await;
    let adapter = toss(&server, Duration::from_secs(60)).adapter(Market::Kr);
    let wanted = many(250);

    let quotes = adapter.quotes(&wanted).await.unwrap();
    assert_eq!(count(&server, PRICES_PATH).await, 2);
    assert_eq!(quotes.len(), 250, "한 종목이 두 번 들어가면 안 된다");
    let seen = per_symbol(&quotes);
    assert_eq!(seen.len(), 250);
    assert!(seen.values().all(|prices| prices.len() == 1));
}

// 검증: D5
#[tokio::test]
async fn 묶음마다_그_묶음에서_요청한_종목의_값만_쓰고_순서는_요청_순서다() {
    let server = MockServer::start().await;
    mount_token(&server, token_ok(), None).await;
    mount_echo_prices(&server, other_chunk_symbol).await;
    let adapter = toss(&server, Duration::from_secs(60)).adapter(Market::Kr);
    let wanted = many(250);

    let quotes = adapter.quotes(&wanted).await.unwrap();
    // 섞여 온 999원은 그 종목을 요청하지 않은 묶음의 것이므로 버린다.
    assert!(
        quotes.iter().all(|q| q.price == Decimal::from(100)),
        "다른 묶음에서 온 값을 쓰면 안 된다"
    );
    let order: Vec<&Symbol> = quotes.iter().map(|q| &q.symbol).collect();
    let expected: Vec<&Symbol> = wanted.iter().collect();
    assert_eq!(order, expected);
}

// 검증: D5
#[tokio::test]
async fn 묶음_하나로_끝나는_호출은_요청한_종목만_돌려준다() {
    let server = MockServer::start().await;
    mount_token(&server, token_ok(), None).await;
    mount_echo_prices(&server, |_| {
        vec![price_item("S9999", "999", "2026-03-25T09:30:00+09:00")]
    })
    .await;
    let adapter = toss(&server, Duration::from_secs(60)).adapter(Market::Kr);
    let wanted = many(200);

    let quotes = adapter.quotes(&wanted).await.unwrap();
    assert_eq!(count(&server, PRICES_PATH).await, 1);
    assert_eq!(quotes.len(), 200);
    assert!(quotes.iter().all(|q| q.symbol.as_str() != "S9999"));
}

// ───────────── 3번: 여러 스레드에서 겹치는 동시 호출 ─────────────

/// 국내 · 미국 어댑터로 `each`개씩 `tokio::spawn`해 겹쳐 부르고 결과를 모은다.
async fn spawn_quotes(
    kr: Arc<TossAdapter>,
    us: Arc<TossAdapter>,
    each: usize,
) -> Vec<Result<Vec<Quote>, MarketError>> {
    let mut handles = Vec::new();
    for i in 0..each * 2 {
        let adapter = if i % 2 == 0 {
            Arc::clone(&kr)
        } else {
            Arc::clone(&us)
        };
        handles.push(tokio::spawn(async move {
            adapter.quotes(&symbols(&["005930"])).await
        }));
    }
    let mut results = Vec::new();
    for handle in handles {
        results.push(handle.await.unwrap());
    }
    results
}

// 검증: D5
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn 여러_스레드에서_겹친_호출도_토큰_발급은_한_번이다() {
    let server = MockServer::start().await;
    mount_token(
        &server,
        token_ok().set_delay(Duration::from_millis(150)),
        None,
    )
    .await;
    mount_echo_prices(&server, |_| Vec::new()).await;
    let toss = toss(&server, Duration::from_secs(60));
    let kr = Arc::new(toss.adapter(Market::Kr));
    let us = Arc::new(toss.adapter(Market::Us));

    let results = spawn_quotes(kr, us, 8).await;
    assert_eq!(results.len(), 16);
    for result in results {
        assert_eq!(result.unwrap().len(), 1);
    }
    assert_eq!(count(&server, TOKEN_PATH).await, 1);
    assert_eq!(count(&server, PRICES_PATH).await, 16);
}

// 검증: D5
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn 여러_스레드에서_쉬는_동안_겹친_호출은_모두_같은_오류고_발급은_한_번이다() {
    let server = MockServer::start().await;
    mount_token(
        &server,
        oauth_error(403, "access_denied", "IP address not allowed"),
        Some(1),
    )
    .await;
    mount_token(&server, token_ok(), None).await;
    mount_echo_prices(&server, |_| Vec::new()).await;
    let toss = toss(&server, Duration::from_secs(60));
    let kr = Arc::new(toss.adapter(Market::Kr));
    let us = Arc::new(toss.adapter(Market::Us));

    assert_eq!(
        kr.quotes(&symbols(&["005930"])).await.unwrap_err(),
        MarketError::IpNotAllowed
    );
    let results = spawn_quotes(kr, us, 8).await;
    for result in results {
        assert_eq!(result.unwrap_err(), MarketError::IpNotAllowed);
    }
    assert_eq!(count(&server, TOKEN_PATH).await, 1);
    assert_eq!(count(&server, PRICES_PATH).await, 0);
}

// 검증: D5
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn 여러_스레드에서_발급이_실패하는_중에_겹친_호출도_발급은_한_번이고_모두_같은_오류다() {
    let server = MockServer::start().await;
    // 실패 응답이 늦게 와서, 그 사이 다른 스레드의 호출들이 기다리게 한다.
    mount_token(
        &server,
        oauth_error(401, "invalid_client", "Client authentication failed.")
            .set_delay(Duration::from_millis(200)),
        Some(1),
    )
    .await;
    mount_token(&server, token_ok(), None).await;
    mount_echo_prices(&server, |_| Vec::new()).await;
    let toss = toss(&server, Duration::from_secs(60));
    let kr = Arc::new(toss.adapter(Market::Kr));
    let us = Arc::new(toss.adapter(Market::Us));
    let auth_failed = MarketError::AuthFailed {
        code: "invalid_client".to_string(),
    };

    let results = spawn_quotes(kr, us, 6).await;
    assert_eq!(results.len(), 12);
    for result in results {
        assert_eq!(result.unwrap_err(), auth_failed);
    }
    assert_eq!(count(&server, TOKEN_PATH).await, 1);
    assert_eq!(count(&server, PRICES_PATH).await, 0);
}
