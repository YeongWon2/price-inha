//! 잠긴 테스트가 덮지 않는 판단을 확인한다. 로컬 가짜 서버만 쓴다.

use std::sync::Mutex as StdMutex;

use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::*;

struct TestClock {
    now: StdMutex<DateTime<Utc>>,
    slept: StdMutex<Vec<Duration>>,
}

impl TestClock {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            now: StdMutex::new("2026-03-25T01:00:00Z".parse().unwrap()),
            slept: StdMutex::new(Vec::new()),
        })
    }

    fn slept(&self) -> Vec<Duration> {
        self.slept.lock().unwrap().clone()
    }
}

#[async_trait]
impl Clock for TestClock {
    fn now(&self) -> DateTime<Utc> {
        *self.now.lock().unwrap()
    }

    async fn sleep(&self, duration: Duration) {
        self.slept.lock().unwrap().push(duration);
        *self.now.lock().unwrap() += chrono::Duration::from_std(duration).unwrap();
    }
}

struct Source(StdMutex<Option<Credentials>>);

impl Source {
    fn with(id: &str, secret: &str) -> Arc<Self> {
        Arc::new(Self(StdMutex::new(Some(Credentials::new(id, secret)))))
    }

    fn set(&self, credentials: Option<Credentials>) {
        *self.0.lock().unwrap() = credentials;
    }
}

impl CredentialSource for Source {
    fn credentials(&self) -> Option<Credentials> {
        self.0.lock().unwrap().clone()
    }
}

fn config(server: &MockServer) -> TossConfig {
    TossConfig {
        base_url: server.uri(),
        timeout: Duration::from_secs(5),
        ..TossConfig::default()
    }
}

async fn mount_token(server: &MockServer) {
    Mock::given(method("POST"))
        .and(path("/oauth2/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            json!({ "access_token": "unit-token", "token_type": "Bearer", "expires_in": 86400 }),
        ))
        .mount(server)
        .await;
}

async fn mount_prices_ok(server: &MockServer) {
    Mock::given(method("GET"))
        .and(path(PRICES_PATH))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "result": [
            { "symbol": "005930", "lastPrice": "72000", "currency": "KRW" }
        ]})))
        .mount(server)
        .await;
}

async fn count(server: &MockServer, wanted: &str) -> usize {
    let requests = server.received_requests().await.unwrap();
    requests.iter().filter(|r| r.url.path() == wanted).count()
}

fn wanted() -> Vec<Symbol> {
    vec![Symbol::new("005930")]
}

#[tokio::test]
async fn 한도_초과에_retry_after가_없으면_1초_기다렸다가_다시_시도한다() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("GET"))
        .and(path(PRICES_PATH))
        .respond_with(ResponseTemplate::new(429))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    mount_prices_ok(&server).await;
    let clock = TestClock::new();
    let adapter = TossClient::new(config(&server), Source::with("id", "pw"), clock.clone())
        .adapter(Market::Kr);

    assert_eq!(adapter.quotes(&wanted()).await.unwrap().len(), 1);
    assert_eq!(clock.slept(), vec![FALLBACK_RETRY_WAIT]);
}

#[tokio::test]
async fn retry_after가_너무_길면_기다리지_않고_한도_초과로_끝낸다() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("GET"))
        .and(path(PRICES_PATH))
        .respond_with(ResponseTemplate::new(429).insert_header("Retry-After", "3600"))
        .mount(&server)
        .await;
    let clock = TestClock::new();
    let adapter = TossClient::new(config(&server), Source::with("id", "pw"), clock.clone())
        .adapter(Market::Kr);

    let err = adapter.quotes(&wanted()).await.unwrap_err();
    assert_eq!(
        err,
        MarketError::RateLimited {
            retry_after: Some(Duration::from_secs(3600))
        }
    );
    assert!(clock.slept().is_empty());
    assert_eq!(count(&server, PRICES_PATH).await, 1);
}

#[tokio::test]
async fn 토큰_발급이_한도_초과면_다시_보내지_않는다() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/oauth2/token"))
        .respond_with(ResponseTemplate::new(429).insert_header("Retry-After", "1"))
        .mount(&server)
        .await;
    let clock = TestClock::new();
    let adapter = TossClient::new(config(&server), Source::with("id", "pw"), clock.clone())
        .adapter(Market::Kr);

    let err = adapter.quotes(&wanted()).await.unwrap_err();
    assert_eq!(
        err,
        MarketError::RateLimited {
            retry_after: Some(Duration::from_secs(1))
        }
    );
    assert!(clock.slept().is_empty());
    assert_eq!(count(&server, "/oauth2/token").await, 1);
    assert_eq!(count(&server, PRICES_PATH).await, 0);

    let err = verify(&config(&server), &Credentials::new("id", "pw"))
        .await
        .unwrap_err();
    assert!(matches!(err, MarketError::RateLimited { .. }));
    assert_eq!(count(&server, "/oauth2/token").await, 2);
}

#[tokio::test]
async fn 장애_5xx는_다시_시도하지_않는다() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("GET"))
        .and(path(PRICES_PATH))
        .respond_with(ResponseTemplate::new(503))
        .mount(&server)
        .await;
    let clock = TestClock::new();
    let adapter = TossClient::new(config(&server), Source::with("id", "pw"), clock.clone())
        .adapter(Market::Kr);

    let err = adapter.quotes(&wanted()).await.unwrap_err();
    assert!(matches!(err, MarketError::Unavailable { status: 503, .. }));
    assert_eq!(count(&server, PRICES_PATH).await, 1);
    assert!(clock.slept().is_empty());
}

#[tokio::test]
async fn 자격이_바뀌거나_지워지면_쥐고_있던_토큰을_버린다() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    mount_prices_ok(&server).await;
    let source = Source::with("id", "pw");
    let adapter =
        TossClient::new(config(&server), source.clone(), TestClock::new()).adapter(Market::Kr);

    adapter.quotes(&wanted()).await.unwrap();
    adapter.quotes(&wanted()).await.unwrap();
    assert_eq!(count(&server, "/oauth2/token").await, 1);

    source.set(Some(Credentials::new("id", "new-pw")));
    adapter.quotes(&wanted()).await.unwrap();
    assert_eq!(count(&server, "/oauth2/token").await, 2);

    source.set(None);
    assert_eq!(
        adapter.quotes(&wanted()).await.unwrap_err(),
        MarketError::NoCredentials
    );
    source.set(Some(Credentials::new("id", "new-pw")));
    adapter.quotes(&wanted()).await.unwrap();
    assert_eq!(count(&server, "/oauth2/token").await, 3);
}

#[tokio::test]
async fn 다른_주소로_넘기는_응답은_따라가지_않는다() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("GET"))
        .and(path(PRICES_PATH))
        .respond_with(ResponseTemplate::new(302).insert_header("Location", "/elsewhere"))
        .mount(&server)
        .await;
    let adapter = TossClient::new(config(&server), Source::with("id", "pw"), TestClock::new())
        .adapter(Market::Kr);

    let err = adapter.quotes(&wanted()).await.unwrap_err();
    assert!(matches!(err, MarketError::UnexpectedResponse { .. }));
    assert_eq!(count(&server, "/elsewhere").await, 0);
}

fn quote_at(symbol: &str, price: i64, exchange_time: Option<&str>) -> Quote {
    Quote {
        symbol: Symbol::new(symbol),
        price: rust_decimal::Decimal::from(price),
        currency: crate::Currency::Krw,
        exchange_time: exchange_time.map(|t| t.parse().unwrap()),
        received_at: "2026-03-25T01:00:00Z".parse().unwrap(),
    }
}

#[test]
fn 같은_종목이_같은_가격이라도_거래소_시각이_다르면_응답_모양_오류다() {
    let parsed = vec![
        quote_at("005930", 100, Some("2026-03-25T00:30:00Z")),
        quote_at("005930", 100, Some("2026-03-25T00:31:00Z")),
    ];
    let err = collapse_duplicates(parsed).unwrap_err();
    assert!(matches!(err, MarketError::UnexpectedResponse { .. }));
}

#[test]
fn 같은_종목이_가격과_시각이_모두_같으면_하나로_줄인다() {
    let parsed = vec![
        quote_at("005930", 100, None),
        quote_at("000660", 200, None),
        quote_at("005930", 100, None),
    ];
    let by_symbol = collapse_duplicates(parsed).unwrap();
    assert_eq!(by_symbol.len(), 2);
}

#[test]
fn 겹친_종목은_처음_나온_순서로_한_번씩_남는다() {
    let wanted = vec![
        Symbol::new("B"),
        Symbol::new("A"),
        Symbol::new("B"),
        Symbol::new("C"),
        Symbol::new("A"),
    ];
    let unique: Vec<&str> = first_occurrences(&wanted)
        .into_iter()
        .map(|s| s.as_str())
        .collect();
    assert_eq!(unique, ["B", "A", "C"]);
}
