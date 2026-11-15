//! 토스증권 어댑터. 로컬 가짜 서버(wiremock)만 쓴다.
//! 가짜 응답의 필드 이름 · 타입 · 예시는 공식 명세(OpenAPI v1.2.28)를 따른다.

use std::collections::HashSet;
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use pi_market::{
    Clock, CredentialSource, Credentials, Currency, DEFAULT_BASE_URL, DEFAULT_MAX_RESPONSE_BYTES,
    DEFAULT_RATE_LIMIT_RETRIES, DEFAULT_TIMEOUT, DEFAULT_TOKEN_FAILURE_COOLDOWN,
    DEFAULT_TOKEN_REFRESH_MARGIN, MAX_SYMBOLS_PER_REQUEST, Market, MarketAdapter, MarketError,
    SessionKind, Symbol, TossClient, TossConfig, verify,
};
use rust_decimal::Decimal;
use serde_json::{Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

const CLIENT_ID: &str = "시험용-클라이언트-아이디-0001";
const CLIENT_SECRET: &str = "시험용-클라이언트-비밀-0001";
const TOKEN_1: &str = "test-access-token-1";
const TOKEN_2: &str = "test-access-token-2";
const TOKEN_3: &str = "test-access-token-3";
const TOKEN_PATH: &str = "/oauth2/token";
const PRICES_PATH: &str = "/api/v1/prices";
const KR_PATH: &str = "/api/v1/market-calendar/KR";
const US_PATH: &str = "/api/v1/market-calendar/US";
const START: &str = "2026-03-25T10:00:00+09:00";

fn ts(text: &str) -> DateTime<Utc> {
    text.parse::<DateTime<Utc>>().unwrap()
}

fn day(text: &str) -> NaiveDate {
    text.parse::<NaiveDate>().unwrap()
}

fn dec(text: &str) -> Decimal {
    Decimal::from_str(text).unwrap()
}

fn symbols(list: &[&str]) -> Vec<Symbol> {
    list.iter().map(|s| Symbol::new(*s)).collect()
}

/// 테스트가 정하는 시계. `sleep`은 기다리지 않고 적어 두기만 하며 시각을 그만큼 옮긴다.
struct TestClock {
    now: Mutex<DateTime<Utc>>,
    slept: Mutex<Vec<Duration>>,
}

impl TestClock {
    fn at(text: &str) -> Arc<Self> {
        Arc::new(Self {
            now: Mutex::new(ts(text)),
            slept: Mutex::new(Vec::new()),
        })
    }

    fn advance(&self, duration: Duration) {
        let mut now = self.now.lock().unwrap();
        *now += chrono::Duration::from_std(duration).unwrap();
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
        self.advance(duration);
    }
}

/// 테스트가 정하는 자격. 처음에는 없을 수 있고 나중에 넣을 수 있다.
struct TestCredentials(Mutex<Option<Credentials>>);

impl TestCredentials {
    fn present() -> Arc<Self> {
        Arc::new(Self(Mutex::new(Some(Credentials::new(
            CLIENT_ID,
            CLIENT_SECRET,
        )))))
    }

    fn absent() -> Arc<Self> {
        Arc::new(Self(Mutex::new(None)))
    }

    fn put(&self, credentials: Credentials) {
        *self.0.lock().unwrap() = Some(credentials);
    }
}

impl CredentialSource for TestCredentials {
    fn credentials(&self) -> Option<Credentials> {
        self.0.lock().unwrap().clone()
    }
}

fn config_for(base_url: &str) -> TossConfig {
    TossConfig {
        base_url: base_url.to_string(),
        timeout: Duration::from_secs(5),
        token_refresh_margin: Duration::from_secs(60),
        rate_limit_retries: 2,
        // 2차에 더해진 설정(발급 실패 쉼 · 응답 크기 한도)은 기본값을 쓴다.
        ..TossConfig::default()
    }
}

fn client(server: &MockServer, clock: &Arc<TestClock>) -> TossClient {
    TossClient::new(
        config_for(&server.uri()),
        TestCredentials::present(),
        clock.clone(),
    )
}

/// 아무도 듣지 않는 로컬 주소.
fn dead_base_url() -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    format!("http://127.0.0.1:{port}")
}

fn token_body(token: &str, expires_in: u64) -> Value {
    json!({ "access_token": token, "token_type": "Bearer", "expires_in": expires_in })
}

/// 토큰 발급을 순서대로 돌려준다. 마지막 토큰은 몇 번이든 돌려준다.
async fn mount_tokens(server: &MockServer, tokens: &[&str], expires_in: u64) {
    for (i, token) in tokens.iter().enumerate() {
        let mock = Mock::given(method("POST"))
            .and(path(TOKEN_PATH))
            .respond_with(ResponseTemplate::new(200).set_body_json(token_body(token, expires_in)));
        if i + 1 < tokens.len() {
            mock.up_to_n_times(1).mount(server).await;
        } else {
            mock.mount(server).await;
        }
    }
}

fn price_item(symbol: &str, price: &str, currency: &str) -> Value {
    json!({
        "symbol": symbol,
        "timestamp": "2026-03-25T09:30:00.123+09:00",
        "lastPrice": price,
        "currency": currency,
    })
}

/// 달라는 종목마다 100원짜리 시세를 돌려주는 현재가 응답.
async fn mount_echo_prices(server: &MockServer) {
    Mock::given(method("GET"))
        .and(path(PRICES_PATH))
        .respond_with(|req: &Request| {
            let wanted = query_value(req, "symbols").unwrap_or_default();
            let items: Vec<Value> = wanted
                .split(',')
                .filter(|s| !s.is_empty())
                .map(|s| price_item(s, "100", "KRW"))
                .collect();
            ResponseTemplate::new(200).set_body_json(json!({ "result": items }))
        })
        .mount(server)
        .await;
}

async fn mount_prices(server: &MockServer, body: Value) {
    Mock::given(method("GET"))
        .and(path(PRICES_PATH))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .mount(server)
        .await;
}

/// 공통 오류 모양: `{"error":{"requestId","code","message"}}`.
fn api_error(status: u16, code: &str) -> ResponseTemplate {
    ResponseTemplate::new(status).set_body_json(json!({
        "error": {
            "requestId": "01HXYZABCDEFG123456789",
            "code": code,
            "message": "요청을 처리하지 못했습니다.",
        }
    }))
}

/// 토큰 발급의 오류 모양(OAuth2 표준): `{"error","error_description"}`.
fn oauth_error(status: u16, error: &str, description: &str) -> ResponseTemplate {
    ResponseTemplate::new(status)
        .set_body_json(json!({ "error": error, "error_description": description }))
}

fn query_value(req: &Request, key: &str) -> Option<String> {
    req.url
        .query_pairs()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.into_owned())
}

fn bearer(req: &Request) -> String {
    req.headers
        .get("authorization")
        .expect("Authorization 헤더가 있어야 한다")
        .to_str()
        .unwrap()
        .to_string()
}

async fn requests_to(server: &MockServer, wanted: &str) -> Vec<Request> {
    server
        .received_requests()
        .await
        .unwrap()
        .into_iter()
        .filter(|r| r.url.path() == wanted)
        .collect()
}

async fn count(server: &MockServer, wanted: &str) -> usize {
    requests_to(server, wanted).await.len()
}

fn kr_open_day(date: &str) -> Value {
    json!({
        "date": date,
        "integrated": {
            "preMarket": {
                "startTime": format!("{date}T08:00:00+09:00"),
                "singlePriceAuctionStartTime": format!("{date}T08:50:00+09:00"),
                "endTime": format!("{date}T09:00:00+09:00"),
            },
            "regularMarket": {
                "startTime": format!("{date}T09:00:00+09:00"),
                "singlePriceAuctionStartTime": format!("{date}T15:20:00+09:00"),
                "endTime": format!("{date}T15:30:00+09:00"),
            },
            "afterMarket": {
                "startTime": format!("{date}T15:30:00+09:00"),
                "singlePriceAuctionEndTime": format!("{date}T15:40:00+09:00"),
                "endTime": format!("{date}T20:00:00+09:00"),
            },
        }
    })
}

fn us_open_day(date: &str, next: &str) -> Value {
    json!({
        "date": date,
        "dayMarket": {
            "startTime": format!("{date}T09:00:00+09:00"),
            "endTime": format!("{date}T16:50:00+09:00"),
        },
        "preMarket": {
            "startTime": format!("{date}T17:00:00+09:00"),
            "endTime": format!("{date}T22:30:00+09:00"),
        },
        "regularMarket": {
            "startTime": format!("{date}T22:30:00+09:00"),
            "endTime": format!("{next}T05:00:00+09:00"),
        },
        "afterMarket": {
            "startTime": format!("{next}T05:00:00+09:00"),
            "endTime": format!("{next}T07:00:00+09:00"),
        },
    })
}

async fn mount_calendar(server: &MockServer, calendar_path: &str, body: Value) {
    Mock::given(method("GET"))
        .and(path(calendar_path))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .mount(server)
        .await;
}

/// 공식 명세 예시 `businessDay` (국내).
fn kr_business_day_body() -> Value {
    json!({ "result": {
        "today": kr_open_day("2026-03-25"),
        "previousBusinessDay": kr_open_day("2026-03-24"),
        "nextBusinessDay": kr_open_day("2026-03-26"),
    }})
}

/// 공식 명세 예시 `businessDay` (미국).
fn us_business_day_body() -> Value {
    json!({ "result": {
        "today": us_open_day("2026-03-25", "2026-03-26"),
        "previousBusinessDay": us_open_day("2026-03-24", "2026-03-25"),
        "nextBusinessDay": us_open_day("2026-03-26", "2026-03-27"),
    }})
}

fn assert_no_secret(text: &str) {
    let encoded_secret: String =
        url::form_urlencoded::byte_serialize(CLIENT_SECRET.as_bytes()).collect();
    let encoded_id: String = url::form_urlencoded::byte_serialize(CLIENT_ID.as_bytes()).collect();
    for needle in [
        CLIENT_SECRET,
        CLIENT_ID,
        encoded_secret.as_str(),
        encoded_id.as_str(),
        "클라이언트-비밀",
        TOKEN_1,
    ] {
        assert!(
            !text.contains(needle),
            "자격이나 토큰 글자가 새어 나왔다: {text}"
        );
    }
}

fn assert_error_has_no_secret(error: &MarketError) {
    assert_no_secret(&error.to_string());
    assert_no_secret(&format!("{error:?}"));
    assert_no_secret(&format!("{error:#?}"));
}

// ───────────── 설정 ─────────────

// 검증: D5
#[test]
fn 설정의_기본값은_지시서와_같다() {
    assert_eq!(DEFAULT_TOKEN_REFRESH_MARGIN, Duration::from_secs(60));
    assert_eq!(DEFAULT_RATE_LIMIT_RETRIES, 2);
    assert_eq!(MAX_SYMBOLS_PER_REQUEST, 200);
    assert!(DEFAULT_BASE_URL.starts_with("https://"));
    assert!(!DEFAULT_BASE_URL.ends_with('/'));

    let config = TossConfig::default();
    assert_eq!(config.base_url, DEFAULT_BASE_URL);
    assert_eq!(config.timeout, DEFAULT_TIMEOUT);
    assert_eq!(config.token_refresh_margin, DEFAULT_TOKEN_REFRESH_MARGIN);
    assert_eq!(config.rate_limit_retries, DEFAULT_RATE_LIMIT_RETRIES);
}

// ───────────── 토큰 ─────────────

// 검증: D5
#[tokio::test]
async fn 토큰_발급은_정해진_경로에_form으로_보낸다() {
    let server = MockServer::start().await;
    mount_tokens(&server, &[TOKEN_1], 86400).await;
    mount_echo_prices(&server).await;
    let clock = TestClock::at(START);
    let adapter = client(&server, &clock).adapter(Market::Kr);

    adapter.quotes(&symbols(&["005930"])).await.unwrap();

    let token_requests = requests_to(&server, TOKEN_PATH).await;
    assert_eq!(token_requests.len(), 1);
    let req = &token_requests[0];
    assert_eq!(req.method.as_str(), "POST");
    let content_type = req.headers.get("content-type").unwrap().to_str().unwrap();
    assert!(
        content_type.starts_with("application/x-www-form-urlencoded"),
        "Content-Type: {content_type}"
    );
    // 자격은 주소가 아니라 본문에만 담는다.
    assert_eq!(req.url.query(), None);
    let mut form: Vec<(String, String)> = url::form_urlencoded::parse(&req.body)
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    form.sort();
    assert_eq!(
        form,
        vec![
            ("client_id".to_string(), CLIENT_ID.to_string()),
            ("client_secret".to_string(), CLIENT_SECRET.to_string()),
            ("grant_type".to_string(), "client_credentials".to_string()),
        ]
    );
}

// 검증: D5
#[tokio::test]
async fn 연결을_만들기만_해서는_요청이_나가지_않는다() {
    let server = MockServer::start().await;
    mount_tokens(&server, &[TOKEN_1], 86400).await;
    let clock = TestClock::at(START);
    let toss = client(&server, &clock);
    let _kr = toss.adapter(Market::Kr);
    let _us = toss.adapter(Market::Us);
    assert!(server.received_requests().await.unwrap().is_empty());
}

// 검증: D5
#[tokio::test]
async fn 토큰은_만료_전에는_다시_발급하지_않는다() {
    let server = MockServer::start().await;
    mount_tokens(&server, &[TOKEN_1, TOKEN_2], 86400).await;
    mount_echo_prices(&server).await;
    mount_calendar(&server, KR_PATH, kr_business_day_body()).await;
    let clock = TestClock::at(START);
    let adapter = client(&server, &clock).adapter(Market::Kr);

    adapter.quotes(&symbols(&["005930"])).await.unwrap();
    clock.advance(Duration::from_secs(3600));
    adapter.quotes(&symbols(&["000660"])).await.unwrap();
    clock.advance(Duration::from_secs(3600 * 12));
    adapter.calendar(day("2026-03-25")).await.unwrap();

    assert_eq!(count(&server, TOKEN_PATH).await, 1);
    let prices = requests_to(&server, PRICES_PATH).await;
    assert_eq!(prices.len(), 2);
    for req in &prices {
        assert_eq!(bearer(req), format!("Bearer {TOKEN_1}"));
    }
}

// 검증: D5
#[tokio::test]
async fn 국내와_미국_어댑터는_토큰_하나를_함께_쓴다() {
    let server = MockServer::start().await;
    mount_tokens(&server, &[TOKEN_1, TOKEN_2], 86400).await;
    mount_echo_prices(&server).await;
    let clock = TestClock::at(START);
    let toss = client(&server, &clock);
    let kr = toss.adapter(Market::Kr);
    let us = toss.clone().adapter(Market::Us);
    assert_eq!(kr.market(), Market::Kr);
    assert_eq!(us.market(), Market::Us);

    kr.quotes(&symbols(&["005930"])).await.unwrap();
    us.quotes(&symbols(&["AAPL"])).await.unwrap();

    assert_eq!(count(&server, TOKEN_PATH).await, 1);
    let prices = requests_to(&server, PRICES_PATH).await;
    assert_eq!(prices.len(), 2);
    assert_eq!(bearer(&prices[1]), format!("Bearer {TOKEN_1}"));
}

// 검증: D5
#[tokio::test]
async fn 토큰_만료가_가까우면_미리_다시_발급한다() {
    let server = MockServer::start().await;
    mount_tokens(&server, &[TOKEN_1, TOKEN_2, TOKEN_3], 3600).await;
    mount_echo_prices(&server).await;
    let clock = TestClock::at(START);
    let adapter = client(&server, &clock).adapter(Market::Kr);

    adapter.quotes(&symbols(&["005930"])).await.unwrap();
    assert_eq!(count(&server, TOKEN_PATH).await, 1);

    // 남은 시간 61초: 기준(60초)보다 많으니 그대로 쓴다.
    clock.advance(Duration::from_secs(3600 - 61));
    adapter.quotes(&symbols(&["005930"])).await.unwrap();
    assert_eq!(count(&server, TOKEN_PATH).await, 1);

    // 남은 시간 59초: 아직 만료 전이지만 미리 다시 발급한다.
    clock.advance(Duration::from_secs(2));
    adapter.quotes(&symbols(&["005930"])).await.unwrap();
    assert_eq!(count(&server, TOKEN_PATH).await, 2);

    let prices = requests_to(&server, PRICES_PATH).await;
    assert_eq!(prices.len(), 3);
    assert_eq!(bearer(&prices[1]), format!("Bearer {TOKEN_1}"));
    assert_eq!(bearer(&prices[2]), format!("Bearer {TOKEN_2}"));

    // 새 토큰도 만료 전까지 다시 쓴다.
    clock.advance(Duration::from_secs(600));
    adapter.quotes(&symbols(&["005930"])).await.unwrap();
    assert_eq!(count(&server, TOKEN_PATH).await, 2);
}

// 검증: D5
#[tokio::test]
async fn 미리_다시_발급하는_기준은_설정값이다() {
    let server = MockServer::start().await;
    mount_tokens(&server, &[TOKEN_1, TOKEN_2], 3600).await;
    mount_echo_prices(&server).await;
    let clock = TestClock::at(START);
    let mut config = config_for(&server.uri());
    config.token_refresh_margin = Duration::from_secs(600);
    let adapter =
        TossClient::new(config, TestCredentials::present(), clock.clone()).adapter(Market::Kr);

    adapter.quotes(&symbols(&["005930"])).await.unwrap();
    // 남은 시간 601초 → 그대로.
    clock.advance(Duration::from_secs(3600 - 601));
    adapter.quotes(&symbols(&["005930"])).await.unwrap();
    assert_eq!(count(&server, TOKEN_PATH).await, 1);
    // 남은 시간 599초 → 다시 발급.
    clock.advance(Duration::from_secs(2));
    adapter.quotes(&symbols(&["005930"])).await.unwrap();
    assert_eq!(count(&server, TOKEN_PATH).await, 2);
}

async fn 무효가_된_토큰은_한_번만_다시_발급해_다시_시도한다(code: &str) {
    let server = MockServer::start().await;
    mount_tokens(&server, &[TOKEN_1, TOKEN_2, TOKEN_3], 86400).await;
    Mock::given(method("GET"))
        .and(path(PRICES_PATH))
        .respond_with(api_error(401, code))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    mount_echo_prices(&server).await;
    let clock = TestClock::at(START);
    let adapter = client(&server, &clock).adapter(Market::Kr);

    let quotes = adapter.quotes(&symbols(&["005930"])).await.unwrap();
    assert_eq!(quotes.len(), 1);
    assert_eq!(quotes[0].symbol, Symbol::new("005930"));

    assert_eq!(count(&server, TOKEN_PATH).await, 2);
    let prices = requests_to(&server, PRICES_PATH).await;
    assert_eq!(prices.len(), 2);
    assert_eq!(bearer(&prices[0]), format!("Bearer {TOKEN_1}"));
    assert_eq!(bearer(&prices[1]), format!("Bearer {TOKEN_2}"));

    // 그 뒤로는 새 토큰을 다시 쓴다.
    adapter.quotes(&symbols(&["005930"])).await.unwrap();
    assert_eq!(count(&server, TOKEN_PATH).await, 2);
}

// 검증: D5
#[tokio::test]
async fn expired_token을_받으면_한_번_다시_발급해_다시_시도한다() {
    무효가_된_토큰은_한_번만_다시_발급해_다시_시도한다("expired-token").await;
}

// 검증: D5
#[tokio::test]
async fn token_revoked를_받으면_한_번_다시_발급해_다시_시도한다() {
    무효가_된_토큰은_한_번만_다시_발급해_다시_시도한다("token-revoked").await;
}

// 검증: D5
#[tokio::test]
async fn 다시_발급한_뒤에도_무효면_더_발급하지_않고_인증_실패다() {
    let server = MockServer::start().await;
    mount_tokens(&server, &[TOKEN_1, TOKEN_2, TOKEN_3], 86400).await;
    Mock::given(method("GET"))
        .and(path(PRICES_PATH))
        .respond_with(api_error(401, "token-revoked"))
        .mount(&server)
        .await;
    let clock = TestClock::at(START);
    let adapter = client(&server, &clock).adapter(Market::Kr);

    let err = adapter.quotes(&symbols(&["005930"])).await.unwrap_err();
    assert_eq!(
        err,
        MarketError::AuthFailed {
            code: "token-revoked".to_string()
        }
    );
    assert_eq!(count(&server, TOKEN_PATH).await, 2);
    assert_eq!(count(&server, PRICES_PATH).await, 2);
}

// 검증: D5
#[tokio::test]
async fn 장_시간_요청도_무효가_된_토큰이면_한_번_다시_발급한다() {
    let server = MockServer::start().await;
    mount_tokens(&server, &[TOKEN_1, TOKEN_2], 86400).await;
    Mock::given(method("GET"))
        .and(path(KR_PATH))
        .respond_with(api_error(401, "expired-token"))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    mount_calendar(&server, KR_PATH, kr_business_day_body()).await;
    let clock = TestClock::at(START);
    let adapter = client(&server, &clock).adapter(Market::Kr);

    let calendar = adapter.calendar(day("2026-03-25")).await.unwrap();
    assert_eq!(calendar.today.date, day("2026-03-25"));
    assert_eq!(count(&server, TOKEN_PATH).await, 2);
    let calls = requests_to(&server, KR_PATH).await;
    assert_eq!(calls.len(), 2);
    assert_eq!(bearer(&calls[1]), format!("Bearer {TOKEN_2}"));
}

// 검증: D5
#[tokio::test]
async fn 동시에_여러_요청이_와도_토큰_발급은_한_번이다() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(TOKEN_PATH))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(token_body(TOKEN_1, 86400))
                .set_delay(Duration::from_millis(150)),
        )
        .mount(&server)
        .await;
    mount_echo_prices(&server).await;
    mount_calendar(&server, US_PATH, us_business_day_body()).await;
    let clock = TestClock::at(START);
    let toss = client(&server, &clock);
    let kr = toss.adapter(Market::Kr);
    let us = toss.adapter(Market::Us);

    let wanted = symbols(&["005930"]);
    let quote_calls = futures::future::join_all((0..8).map(|_| kr.quotes(&wanted)));
    let (quote_results, calendar_result) =
        futures::future::join(quote_calls, us.calendar(day("2026-03-25"))).await;

    for result in quote_results {
        assert_eq!(result.unwrap().len(), 1);
    }
    calendar_result.unwrap();
    assert_eq!(count(&server, TOKEN_PATH).await, 1);
    assert_eq!(count(&server, PRICES_PATH).await, 8);
}

// ───────────── 현재가 ─────────────

// 검증: D5
#[tokio::test]
async fn 현재가는_정해진_경로에_symbols와_bearer_토큰으로_묻는다() {
    let server = MockServer::start().await;
    mount_tokens(&server, &[TOKEN_1], 86400).await;
    mount_echo_prices(&server).await;
    let clock = TestClock::at(START);
    let adapter = client(&server, &clock).adapter(Market::Kr);

    adapter
        .quotes(&symbols(&["005930", "000660"]))
        .await
        .unwrap();

    let all = server.received_requests().await.unwrap();
    assert_eq!(all.len(), 2, "토큰 발급 한 번과 현재가 한 번만 나간다");
    let prices = requests_to(&server, PRICES_PATH).await;
    assert_eq!(prices.len(), 1);
    let req = &prices[0];
    assert_eq!(req.method.as_str(), "GET");
    assert_eq!(query_value(req, "symbols").unwrap(), "005930,000660");
    assert_eq!(req.url.query_pairs().count(), 1);
    assert_eq!(bearer(req), format!("Bearer {TOKEN_1}"));
}

// 검증: D5
#[tokio::test]
async fn 현재가는_명세_예시를_그대로_읽는다() {
    let server = MockServer::start().await;
    mount_tokens(&server, &[TOKEN_1], 86400).await;
    mount_prices(
        &server,
        json!({ "result": [
            { "symbol": "005930", "timestamp": "2026-03-25T09:30:00.123+09:00", "lastPrice": "72000", "currency": "KRW" },
            { "symbol": "AAPL", "timestamp": "2026-03-25T22:30:00.456+09:00", "lastPrice": "185.70", "currency": "USD" },
        ]}),
    )
    .await;
    let clock = TestClock::at(START);
    let adapter = client(&server, &clock).adapter(Market::Kr);

    let quotes = adapter.quotes(&symbols(&["005930", "AAPL"])).await.unwrap();

    assert_eq!(quotes.len(), 2);
    assert_eq!(quotes[0].symbol, Symbol::new("005930"));
    assert_eq!(quotes[0].price, dec("72000"));
    assert_eq!(quotes[0].currency, Currency::Krw);
    assert_eq!(
        quotes[0].exchange_time,
        Some(ts("2026-03-25T09:30:00.123+09:00"))
    );
    assert_eq!(quotes[1].symbol, Symbol::new("AAPL"));
    assert_eq!(quotes[1].price, dec("185.70"));
    assert_eq!(quotes[1].currency, Currency::Usd);
    assert_eq!(
        quotes[1].exchange_time,
        Some(ts("2026-03-25T22:30:00.456+09:00"))
    );
}

// 검증: D5
#[tokio::test]
async fn 가격_문자열은_부동소수점을_거치지_않고_십진수로_읽는다() {
    let server = MockServer::start().await;
    mount_tokens(&server, &[TOKEN_1], 86400).await;
    mount_prices(
        &server,
        json!({ "result": [
            price_item("AAPL", "185.70", "USD"),
            price_item("TINY", "0.0001", "USD"),
            price_item("LONG", "123456789012.123456789", "USD"),
            price_item("SUM1", "0.1", "USD"),
            price_item("SUM2", "0.2", "USD"),
        ]}),
    )
    .await;
    let clock = TestClock::at(START);
    let adapter = client(&server, &clock).adapter(Market::Us);

    let quotes = adapter
        .quotes(&symbols(&["AAPL", "TINY", "LONG", "SUM1", "SUM2"]))
        .await
        .unwrap();

    assert_eq!(quotes[0].price, Decimal::new(18570, 2));
    assert_eq!(quotes[0].price.to_string(), "185.70");
    assert_eq!(quotes[1].price, Decimal::new(1, 4));
    assert_eq!(quotes[2].price.to_string(), "123456789012.123456789");
    // f64였다면 0.30000000000000004가 된다.
    assert_eq!(quotes[3].price + quotes[4].price, dec("0.3"));
}

// 검증: D5
#[tokio::test]
async fn 거래소_시각이_null이거나_없어도_시세를_읽는다() {
    let server = MockServer::start().await;
    mount_tokens(&server, &[TOKEN_1], 86400).await;
    mount_prices(
        &server,
        json!({ "result": [
            { "symbol": "005930", "timestamp": null, "lastPrice": "72000", "currency": "KRW" },
            { "symbol": "000660", "lastPrice": "185500", "currency": "KRW" },
        ]}),
    )
    .await;
    let clock = TestClock::at(START);
    let adapter = client(&server, &clock).adapter(Market::Kr);

    let quotes = adapter
        .quotes(&symbols(&["005930", "000660"]))
        .await
        .unwrap();

    assert_eq!(quotes.len(), 2);
    assert_eq!(quotes[0].exchange_time, None);
    assert_eq!(quotes[0].price, dec("72000"));
    assert_eq!(quotes[1].exchange_time, None);
    assert_eq!(quotes[1].price, dec("185500"));
}

// 검증: D5, N6
#[tokio::test]
async fn 받은_시각은_거래소_시각이_아니라_시계의_지금이다() {
    let server = MockServer::start().await;
    mount_tokens(&server, &[TOKEN_1], 86400).await;
    mount_echo_prices(&server).await;
    let clock = TestClock::at("2026-03-25T10:00:00+09:00");
    let adapter = client(&server, &clock).adapter(Market::Kr);

    let first = adapter.quotes(&symbols(&["005930"])).await.unwrap();
    assert_eq!(first[0].received_at, ts("2026-03-25T10:00:00+09:00"));

    clock.advance(Duration::from_secs(7));
    let second = adapter.quotes(&symbols(&["005930"])).await.unwrap();
    assert_eq!(second[0].received_at, ts("2026-03-25T10:00:07+09:00"));
    assert_eq!(
        second[0].exchange_time,
        Some(ts("2026-03-25T09:30:00.123+09:00"))
    );
}

// 검증: D5
#[tokio::test]
async fn 모르는_통화_코드도_받아들인다() {
    let server = MockServer::start().await;
    mount_tokens(&server, &[TOKEN_1], 86400).await;
    mount_prices(
        &server,
        json!({ "result": [ price_item("7203", "2850", "JPY") ] }),
    )
    .await;
    let clock = TestClock::at(START);
    let adapter = client(&server, &clock).adapter(Market::Kr);

    let quotes = adapter.quotes(&symbols(&["7203"])).await.unwrap();
    assert_eq!(quotes[0].currency, Currency::Other("JPY".to_string()));
    assert_eq!(quotes[0].price, dec("2850"));
}

// 검증: D5
#[tokio::test]
async fn 종목이_200개를_넘으면_200개씩_나눠_부른다() {
    let server = MockServer::start().await;
    mount_tokens(&server, &[TOKEN_1], 86400).await;
    mount_echo_prices(&server).await;
    let clock = TestClock::at(START);
    let adapter = client(&server, &clock).adapter(Market::Kr);
    let wanted: Vec<Symbol> = (0..450).map(|i| Symbol::new(format!("S{i:04}"))).collect();

    let quotes = adapter.quotes(&wanted).await.unwrap();

    // 결과는 달라는 순서대로 모두 온다.
    let got: Vec<Symbol> = quotes.iter().map(|q| q.symbol.clone()).collect();
    assert_eq!(got, wanted);

    let prices = requests_to(&server, PRICES_PATH).await;
    let mut sizes = Vec::new();
    let mut seen = HashSet::new();
    for req in &prices {
        let value = query_value(req, "symbols").unwrap();
        let chunk: Vec<&str> = value.split(',').collect();
        sizes.push(chunk.len());
        for symbol in chunk {
            assert!(seen.insert(symbol.to_string()), "{symbol}을 두 번 물었다");
        }
    }
    sizes.sort_unstable();
    assert_eq!(sizes, vec![50, 200, 200]);
    assert_eq!(seen.len(), 450);
    assert_eq!(count(&server, TOKEN_PATH).await, 1);
}

// 검증: D5
#[tokio::test]
async fn 종목이_딱_200개면_한_번만_부른다() {
    let server = MockServer::start().await;
    mount_tokens(&server, &[TOKEN_1], 86400).await;
    mount_echo_prices(&server).await;
    let clock = TestClock::at(START);
    let adapter = client(&server, &clock).adapter(Market::Kr);
    let wanted: Vec<Symbol> = (0..200).map(|i| Symbol::new(format!("S{i:04}"))).collect();

    let quotes = adapter.quotes(&wanted).await.unwrap();
    assert_eq!(quotes.len(), 200);
    assert_eq!(count(&server, PRICES_PATH).await, 1);

    let one_more: Vec<Symbol> = (0..201).map(|i| Symbol::new(format!("S{i:04}"))).collect();
    let quotes = adapter.quotes(&one_more).await.unwrap();
    assert_eq!(quotes.len(), 201);
    assert_eq!(count(&server, PRICES_PATH).await, 3);
}

// 검증: D5
#[tokio::test]
async fn 종목이_없으면_요청을_보내지_않는다() {
    let server = MockServer::start().await;
    mount_tokens(&server, &[TOKEN_1], 86400).await;
    mount_echo_prices(&server).await;
    let clock = TestClock::at(START);
    let adapter = client(&server, &clock).adapter(Market::Kr);

    let quotes = adapter.quotes(&[]).await.unwrap();
    assert!(quotes.is_empty());
    assert!(server.received_requests().await.unwrap().is_empty());
}

// ───────────── 장 시간 ─────────────

// 검증: D5
#[tokio::test]
async fn 국내_장_시간은_kr_경로에_date로_묻고_세션을_읽는다() {
    let server = MockServer::start().await;
    mount_tokens(&server, &[TOKEN_1], 86400).await;
    mount_calendar(&server, KR_PATH, kr_business_day_body()).await;
    let clock = TestClock::at(START);
    let adapter = client(&server, &clock).adapter(Market::Kr);

    let calendar = adapter.calendar(day("2026-03-25")).await.unwrap();

    let calls = requests_to(&server, KR_PATH).await;
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].method.as_str(), "GET");
    assert_eq!(query_value(&calls[0], "date").unwrap(), "2026-03-25");
    assert_eq!(bearer(&calls[0]), format!("Bearer {TOKEN_1}"));
    assert_eq!(count(&server, US_PATH).await, 0);

    assert_eq!(calendar.market, Market::Kr);
    assert_eq!(calendar.today.date, day("2026-03-25"));
    assert_eq!(calendar.previous_business_day.date, day("2026-03-24"));
    assert_eq!(calendar.next_business_day.date, day("2026-03-26"));

    let kinds: Vec<SessionKind> = calendar.today.sessions.iter().map(|s| s.kind).collect();
    assert_eq!(
        kinds,
        vec![
            SessionKind::PreMarket,
            SessionKind::Regular,
            SessionKind::AfterMarket
        ]
    );
    let pre = calendar.today.session(SessionKind::PreMarket).unwrap();
    assert_eq!(pre.start, ts("2026-03-25T08:00:00+09:00"));
    assert_eq!(pre.end, ts("2026-03-25T09:00:00+09:00"));
    let regular = calendar.today.session(SessionKind::Regular).unwrap();
    assert_eq!(regular.start, ts("2026-03-25T09:00:00+09:00"));
    assert_eq!(regular.end, ts("2026-03-25T15:30:00+09:00"));
    let after = calendar.today.session(SessionKind::AfterMarket).unwrap();
    assert_eq!(after.start, ts("2026-03-25T15:30:00+09:00"));
    assert_eq!(after.end, ts("2026-03-25T20:00:00+09:00"));

    let prev = calendar
        .previous_business_day
        .session(SessionKind::Regular)
        .unwrap();
    assert_eq!(prev.start, ts("2026-03-24T09:00:00+09:00"));
    let next = calendar
        .next_business_day
        .session(SessionKind::Regular)
        .unwrap();
    assert_eq!(next.end, ts("2026-03-26T15:30:00+09:00"));

    // 받은 달력으로 지금 세션을 안다.
    let now = adapter
        .session_at(&calendar, ts("2026-03-25T10:00:00+09:00"))
        .unwrap();
    assert_eq!(now.kind, SessionKind::Regular);
    assert_eq!(
        adapter.session_at(&calendar, ts("2026-03-25T20:00:00+09:00")),
        None
    );
}

// 검증: D5
#[tokio::test]
async fn 국내_휴장일은_integrated가_null이고_세션이_없다() {
    let server = MockServer::start().await;
    mount_tokens(&server, &[TOKEN_1], 86400).await;
    // 공식 명세 예시 `holidayToday`.
    mount_calendar(
        &server,
        KR_PATH,
        json!({ "result": {
            "today": { "date": "2026-05-05", "integrated": null },
            "previousBusinessDay": kr_open_day("2026-05-04"),
            "nextBusinessDay": kr_open_day("2026-05-06"),
        }}),
    )
    .await;
    let clock = TestClock::at("2026-05-05T10:00:00+09:00");
    let adapter = client(&server, &clock).adapter(Market::Kr);

    let calendar = adapter.calendar(day("2026-05-05")).await.unwrap();

    assert_eq!(
        query_value(&requests_to(&server, KR_PATH).await[0], "date").unwrap(),
        "2026-05-05"
    );
    assert_eq!(calendar.today.date, day("2026-05-05"));
    assert!(calendar.today.is_holiday());
    assert!(calendar.today.sessions.is_empty());
    assert!(!calendar.previous_business_day.is_holiday());
    assert_eq!(calendar.previous_business_day.date, day("2026-05-04"));
    assert_eq!(calendar.next_business_day.sessions.len(), 3);
    assert_eq!(
        adapter.session_at(&calendar, ts("2026-05-05T10:00:00+09:00")),
        None
    );
}

// 검증: D5
#[tokio::test]
async fn 국내_부분_휴장은_null인_세션만_뺀다() {
    let server = MockServer::start().await;
    mount_tokens(&server, &[TOKEN_1], 86400).await;
    // 공식 명세 예시 `nxtPreMarketHoliday`.
    let mut today = kr_open_day("2026-03-25");
    today["integrated"]["preMarket"] = Value::Null;
    mount_calendar(
        &server,
        KR_PATH,
        json!({ "result": {
            "today": today,
            "previousBusinessDay": kr_open_day("2026-03-24"),
            "nextBusinessDay": kr_open_day("2026-03-26"),
        }}),
    )
    .await;
    let clock = TestClock::at(START);
    let adapter = client(&server, &clock).adapter(Market::Kr);

    let calendar = adapter.calendar(day("2026-03-25")).await.unwrap();

    assert!(!calendar.today.is_holiday());
    assert!(calendar.today.session(SessionKind::PreMarket).is_none());
    let kinds: Vec<SessionKind> = calendar.today.sessions.iter().map(|s| s.kind).collect();
    assert_eq!(kinds, vec![SessionKind::Regular, SessionKind::AfterMarket]);
    assert_eq!(
        adapter.session_at(&calendar, ts("2026-03-25T08:30:00+09:00")),
        None
    );
}

// 검증: D5
#[tokio::test]
async fn 미국_장_시간은_us_경로로_묻고_네_세션을_읽는다() {
    let server = MockServer::start().await;
    mount_tokens(&server, &[TOKEN_1], 86400).await;
    mount_calendar(&server, US_PATH, us_business_day_body()).await;
    let clock = TestClock::at(START);
    let adapter = client(&server, &clock).adapter(Market::Us);

    let calendar = adapter.calendar(day("2026-03-25")).await.unwrap();

    let calls = requests_to(&server, US_PATH).await;
    assert_eq!(calls.len(), 1);
    assert_eq!(query_value(&calls[0], "date").unwrap(), "2026-03-25");
    assert_eq!(bearer(&calls[0]), format!("Bearer {TOKEN_1}"));
    assert_eq!(count(&server, KR_PATH).await, 0);

    assert_eq!(calendar.market, Market::Us);
    assert_eq!(calendar.today.date, day("2026-03-25"));
    assert_eq!(calendar.previous_business_day.date, day("2026-03-24"));
    assert_eq!(calendar.next_business_day.date, day("2026-03-26"));
    let kinds: Vec<SessionKind> = calendar.today.sessions.iter().map(|s| s.kind).collect();
    assert_eq!(
        kinds,
        vec![
            SessionKind::DayMarket,
            SessionKind::PreMarket,
            SessionKind::Regular,
            SessionKind::AfterMarket
        ]
    );
    let day_market = calendar.today.session(SessionKind::DayMarket).unwrap();
    assert_eq!(day_market.start, ts("2026-03-25T09:00:00+09:00"));
    assert_eq!(day_market.end, ts("2026-03-25T16:50:00+09:00"));
    let pre = calendar.today.session(SessionKind::PreMarket).unwrap();
    assert_eq!(pre.start, ts("2026-03-25T17:00:00+09:00"));
    assert_eq!(pre.end, ts("2026-03-25T22:30:00+09:00"));
    let after = calendar.today.session(SessionKind::AfterMarket).unwrap();
    assert_eq!(after.start, ts("2026-03-26T05:00:00+09:00"));
    assert_eq!(after.end, ts("2026-03-26T07:00:00+09:00"));
}

// 검증: D5
#[tokio::test]
async fn 미국_정규장은_날짜를_넘는_세션으로_읽는다() {
    let server = MockServer::start().await;
    mount_tokens(&server, &[TOKEN_1], 86400).await;
    mount_calendar(&server, US_PATH, us_business_day_body()).await;
    let clock = TestClock::at(START);
    let adapter = client(&server, &clock).adapter(Market::Us);

    let calendar = adapter.calendar(day("2026-03-25")).await.unwrap();

    let regular = calendar.today.session(SessionKind::Regular).unwrap();
    assert_eq!(regular.start, ts("2026-03-25T22:30:00+09:00"));
    assert_eq!(regular.end, ts("2026-03-26T05:00:00+09:00"));
    // KST 새벽 2시(다음 날짜)는 여전히 03-25 정규장이다.
    let got = adapter
        .session_at(&calendar, ts("2026-03-26T02:00:00+09:00"))
        .unwrap();
    assert_eq!(got.kind, SessionKind::Regular);
    assert_eq!(got.start, ts("2026-03-25T22:30:00+09:00"));
    // 03-25 KST 새벽 2시는 이전 영업일(03-24) 정규장이다.
    let got = adapter
        .session_at(&calendar, ts("2026-03-25T02:00:00+09:00"))
        .unwrap();
    assert_eq!(got.kind, SessionKind::Regular);
    assert_eq!(got.start, ts("2026-03-24T22:30:00+09:00"));
}

// 검증: D5
#[tokio::test]
async fn 미국_휴장일은_네_세션이_모두_null이고_세션이_없다() {
    let server = MockServer::start().await;
    mount_tokens(&server, &[TOKEN_1], 86400).await;
    // 공식 명세 예시 `holidayToday`.
    mount_calendar(
        &server,
        US_PATH,
        json!({ "result": {
            "today": { "date": "2026-07-03", "dayMarket": null, "preMarket": null, "regularMarket": null, "afterMarket": null },
            "previousBusinessDay": us_open_day("2026-07-02", "2026-07-03"),
            "nextBusinessDay": us_open_day("2026-07-06", "2026-07-07"),
        }}),
    )
    .await;
    let clock = TestClock::at("2026-07-03T23:00:00+09:00");
    let adapter = client(&server, &clock).adapter(Market::Us);

    let calendar = adapter.calendar(day("2026-07-03")).await.unwrap();

    assert!(calendar.today.is_holiday());
    assert_eq!(calendar.previous_business_day.sessions.len(), 4);
    assert_eq!(calendar.next_business_day.date, day("2026-07-06"));
    assert_eq!(
        adapter.session_at(&calendar, ts("2026-07-03T23:00:00+09:00")),
        None
    );
    // 이전 영업일의 애프터마켓은 휴장일 아침(KST)까지 이어진다.
    let tail = adapter
        .session_at(&calendar, ts("2026-07-03T06:00:00+09:00"))
        .unwrap();
    assert_eq!(tail.kind, SessionKind::AfterMarket);
}

// ───────────── 오류 구분 ─────────────

// 검증: V31
#[tokio::test]
async fn 키가_없으면_요청을_보내지_않고_키를_넣은_뒤에는_된다() {
    let server = MockServer::start().await;
    mount_tokens(&server, &[TOKEN_1], 86400).await;
    mount_echo_prices(&server).await;
    mount_calendar(&server, KR_PATH, kr_business_day_body()).await;
    let clock = TestClock::at(START);
    let source = TestCredentials::absent();
    let adapter = TossClient::new(config_for(&server.uri()), source.clone(), clock.clone())
        .adapter(Market::Kr);

    let err = adapter.quotes(&symbols(&["005930"])).await.unwrap_err();
    assert_eq!(err, MarketError::NoCredentials);
    let err = adapter.calendar(day("2026-03-25")).await.unwrap_err();
    assert_eq!(err, MarketError::NoCredentials);
    assert!(
        server.received_requests().await.unwrap().is_empty(),
        "키가 없으면 바깥으로 아무 요청도 나가지 않는다"
    );

    source.put(Credentials::new(CLIENT_ID, CLIENT_SECRET));
    let quotes = adapter.quotes(&symbols(&["005930"])).await.unwrap();
    assert_eq!(quotes.len(), 1);
    adapter.calendar(day("2026-03-25")).await.unwrap();
    assert_eq!(count(&server, TOKEN_PATH).await, 1);
}

// 검증: D5
#[tokio::test]
async fn 토큰_발급이_401_invalid_client면_인증_실패다() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(TOKEN_PATH))
        .respond_with(
            oauth_error(401, "invalid_client", "Client authentication failed.")
                .insert_header("WWW-Authenticate", "Basic realm=\"openapi\""),
        )
        .mount(&server)
        .await;
    mount_echo_prices(&server).await;
    let clock = TestClock::at(START);
    let adapter = client(&server, &clock).adapter(Market::Kr);

    let err = adapter.quotes(&symbols(&["005930"])).await.unwrap_err();
    assert_eq!(
        err,
        MarketError::AuthFailed {
            code: "invalid_client".to_string()
        }
    );
    // 토큰이 없으면 현재가를 묻지 않는다.
    assert_eq!(count(&server, PRICES_PATH).await, 0);
    assert_error_has_no_secret(&err);
}

// 검증: D5
#[tokio::test]
async fn 토큰_발급이_403_access_denied면_허용_ip가_아니다() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(TOKEN_PATH))
        .respond_with(oauth_error(403, "access_denied", "IP address not allowed"))
        .mount(&server)
        .await;
    let clock = TestClock::at(START);
    let adapter = client(&server, &clock).adapter(Market::Kr);

    let err = adapter.calendar(day("2026-03-25")).await.unwrap_err();
    assert_eq!(err, MarketError::IpNotAllowed);
    assert_eq!(count(&server, KR_PATH).await, 0);
    assert_error_has_no_secret(&err);
}

// 검증: D5
#[tokio::test]
async fn 요청이_401_invalid_token이면_다시_발급하지_않고_인증_실패다() {
    let server = MockServer::start().await;
    mount_tokens(&server, &[TOKEN_1, TOKEN_2], 86400).await;
    Mock::given(method("GET"))
        .and(path(PRICES_PATH))
        .respond_with(api_error(401, "invalid-token"))
        .mount(&server)
        .await;
    let clock = TestClock::at(START);
    let adapter = client(&server, &clock).adapter(Market::Kr);

    let err = adapter.quotes(&symbols(&["005930"])).await.unwrap_err();
    assert_eq!(
        err,
        MarketError::AuthFailed {
            code: "invalid-token".to_string()
        }
    );
    assert_eq!(count(&server, TOKEN_PATH).await, 1);
    assert_eq!(count(&server, PRICES_PATH).await, 1);
    assert_error_has_no_secret(&err);
}

// 검증: D5
#[tokio::test]
async fn 요청이_403_ip_not_allowed면_허용_ip가_아니다() {
    let server = MockServer::start().await;
    mount_tokens(&server, &[TOKEN_1, TOKEN_2], 86400).await;
    Mock::given(method("GET"))
        .and(path(PRICES_PATH))
        .respond_with(api_error(403, "ip-not-allowed"))
        .mount(&server)
        .await;
    let clock = TestClock::at(START);
    let adapter = client(&server, &clock).adapter(Market::Kr);

    let err = adapter.quotes(&symbols(&["005930"])).await.unwrap_err();
    assert_eq!(err, MarketError::IpNotAllowed);
    assert_eq!(count(&server, TOKEN_PATH).await, 1);
    assert_eq!(count(&server, PRICES_PATH).await, 1);
    assert!(clock.slept().is_empty());
}

fn rate_limited(retry_after: &str) -> ResponseTemplate {
    api_error(429, "rate-limit-exceeded")
        .insert_header("Retry-After", retry_after)
        .insert_header("X-RateLimit-Limit", "10")
        .insert_header("X-RateLimit-Remaining", "0")
        .insert_header("X-RateLimit-Reset", retry_after)
}

// 검증: D5
#[tokio::test]
async fn 한도_초과는_retry_after만큼_기다렸다가_다시_시도한다() {
    let server = MockServer::start().await;
    mount_tokens(&server, &[TOKEN_1, TOKEN_2], 86400).await;
    Mock::given(method("GET"))
        .and(path(PRICES_PATH))
        .respond_with(rate_limited("1"))
        .up_to_n_times(2)
        .mount(&server)
        .await;
    mount_echo_prices(&server).await;
    let clock = TestClock::at(START);
    let adapter = client(&server, &clock).adapter(Market::Kr);

    let quotes = adapter.quotes(&symbols(&["005930"])).await.unwrap();

    assert_eq!(quotes.len(), 1);
    assert_eq!(count(&server, PRICES_PATH).await, 3);
    assert_eq!(
        clock.slept(),
        vec![Duration::from_secs(1), Duration::from_secs(1)]
    );
    // 한도 초과로는 토큰을 다시 발급하지 않는다.
    assert_eq!(count(&server, TOKEN_PATH).await, 1);
    // 기다린 뒤에 받았으니 받은 시각도 그만큼 뒤다.
    assert_eq!(quotes[0].received_at, ts("2026-03-25T10:00:02+09:00"));
}

// 검증: D5
#[tokio::test]
async fn 한도_초과가_이어지면_두_번만_다시_시도하고_한도_초과로_끝낸다() {
    let server = MockServer::start().await;
    mount_tokens(&server, &[TOKEN_1, TOKEN_2], 86400).await;
    Mock::given(method("GET"))
        .and(path(PRICES_PATH))
        .respond_with(rate_limited("3"))
        .mount(&server)
        .await;
    let clock = TestClock::at(START);
    let adapter = client(&server, &clock).adapter(Market::Kr);

    let err = adapter.quotes(&symbols(&["005930"])).await.unwrap_err();

    assert_eq!(
        err,
        MarketError::RateLimited {
            retry_after: Some(Duration::from_secs(3))
        }
    );
    assert_eq!(count(&server, PRICES_PATH).await, 3, "처음 1번 + 다시 2번");
    assert_eq!(
        clock.slept(),
        vec![Duration::from_secs(3), Duration::from_secs(3)]
    );
    assert_eq!(count(&server, TOKEN_PATH).await, 1);
}

// 검증: D5
#[tokio::test]
async fn 다시_시도하는_횟수는_설정값이다() {
    let server = MockServer::start().await;
    mount_tokens(&server, &[TOKEN_1], 86400).await;
    Mock::given(method("GET"))
        .and(path(KR_PATH))
        .respond_with(rate_limited("1"))
        .mount(&server)
        .await;
    let clock = TestClock::at(START);
    let mut config = config_for(&server.uri());
    config.rate_limit_retries = 0;
    let adapter =
        TossClient::new(config, TestCredentials::present(), clock.clone()).adapter(Market::Kr);

    let err = adapter.calendar(day("2026-03-25")).await.unwrap_err();
    assert!(matches!(err, MarketError::RateLimited { .. }));
    assert_eq!(count(&server, KR_PATH).await, 1);
    assert!(clock.slept().is_empty());

    let mut config = config_for(&server.uri());
    config.rate_limit_retries = 4;
    let clock = TestClock::at(START);
    let adapter =
        TossClient::new(config, TestCredentials::present(), clock.clone()).adapter(Market::Kr);
    let err = adapter.calendar(day("2026-03-25")).await.unwrap_err();
    assert!(matches!(err, MarketError::RateLimited { .. }));
    assert_eq!(count(&server, KR_PATH).await, 1 + 5);
    assert_eq!(clock.slept().len(), 4);
}

// 검증: D5
#[tokio::test]
async fn 점검_중이면_점검_장애_오류다() {
    let server = MockServer::start().await;
    mount_tokens(&server, &[TOKEN_1], 86400).await;
    Mock::given(method("GET"))
        .and(path(PRICES_PATH))
        .respond_with(api_error(500, "maintenance"))
        .mount(&server)
        .await;
    let clock = TestClock::at(START);
    let adapter = client(&server, &clock).adapter(Market::Kr);

    let err = adapter.quotes(&symbols(&["005930"])).await.unwrap_err();
    assert_eq!(
        err,
        MarketError::Unavailable {
            status: 500,
            code: Some("maintenance".to_string())
        }
    );
    assert_eq!(count(&server, TOKEN_PATH).await, 1);
}

// 검증: D5
#[tokio::test]
async fn 서버_장애_5xx는_점검_장애_오류다() {
    let server = MockServer::start().await;
    mount_tokens(&server, &[TOKEN_1], 86400).await;
    Mock::given(method("GET"))
        .and(path(PRICES_PATH))
        .respond_with(api_error(500, "internal-error"))
        .mount(&server)
        .await;
    // 본문이 JSON이 아닌 5xx(앞단 장비가 낸 응답)도 같은 구분이다.
    Mock::given(method("GET"))
        .and(path(KR_PATH))
        .respond_with(
            ResponseTemplate::new(503).set_body_string("<html>Service Unavailable</html>"),
        )
        .mount(&server)
        .await;
    let clock = TestClock::at(START);
    let adapter = client(&server, &clock).adapter(Market::Kr);

    let err = adapter.quotes(&symbols(&["005930"])).await.unwrap_err();
    assert_eq!(
        err,
        MarketError::Unavailable {
            status: 500,
            code: Some("internal-error".to_string())
        }
    );
    let err = adapter.calendar(day("2026-03-25")).await.unwrap_err();
    assert_eq!(
        err,
        MarketError::Unavailable {
            status: 503,
            code: None
        }
    );
}

// 검증: D5
#[tokio::test]
async fn 토큰_발급이_5xx면_점검_장애_오류다() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(TOKEN_PATH))
        .respond_with(ResponseTemplate::new(502).set_body_string("Bad Gateway"))
        .mount(&server)
        .await;
    let clock = TestClock::at(START);
    let adapter = client(&server, &clock).adapter(Market::Kr);

    let err = adapter.quotes(&symbols(&["005930"])).await.unwrap_err();
    assert!(matches!(err, MarketError::Unavailable { status: 502, .. }));
    assert_eq!(count(&server, PRICES_PATH).await, 0);
}

// 검증: D5
#[tokio::test]
async fn 없는_종목_404는_거절_오류다() {
    let server = MockServer::start().await;
    mount_tokens(&server, &[TOKEN_1], 86400).await;
    Mock::given(method("GET"))
        .and(path(PRICES_PATH))
        .respond_with(api_error(404, "stock-not-found"))
        .mount(&server)
        .await;
    let clock = TestClock::at(START);
    let adapter = client(&server, &clock).adapter(Market::Kr);

    let err = adapter.quotes(&symbols(&["999999"])).await.unwrap_err();
    assert_eq!(
        err,
        MarketError::Rejected {
            status: 404,
            code: "stock-not-found".to_string()
        }
    );
    assert_eq!(count(&server, PRICES_PATH).await, 1);
    assert_eq!(count(&server, TOKEN_PATH).await, 1);
}

// 검증: D5
#[tokio::test]
async fn 현재가_응답_모양이_다르면_응답_모양_오류다() {
    let bodies = [
        // result가 없다.
        json!({ "data": [] }),
        // result가 배열이 아니다.
        json!({ "result": { "symbol": "005930" } }),
        // 필수 필드 lastPrice가 없다.
        json!({ "result": [ { "symbol": "005930", "currency": "KRW" } ] }),
        // lastPrice가 십진수가 아니다.
        json!({ "result": [ { "symbol": "005930", "lastPrice": "칠만이천", "currency": "KRW" } ] }),
        // timestamp가 시각이 아니다.
        json!({ "result": [ { "symbol": "005930", "timestamp": "어제", "lastPrice": "72000", "currency": "KRW" } ] }),
    ];
    for body in bodies {
        let server = MockServer::start().await;
        mount_tokens(&server, &[TOKEN_1], 86400).await;
        mount_prices(&server, body.clone()).await;
        let clock = TestClock::at(START);
        let adapter = client(&server, &clock).adapter(Market::Kr);

        let err = adapter.quotes(&symbols(&["005930"])).await.unwrap_err();
        assert!(
            matches!(err, MarketError::UnexpectedResponse { .. }),
            "{body} → {err:?}"
        );
    }
}

// 검증: D5
#[tokio::test]
async fn 본문이_json이_아니면_응답_모양_오류다() {
    let server = MockServer::start().await;
    mount_tokens(&server, &[TOKEN_1], 86400).await;
    Mock::given(method("GET"))
        .and(path(PRICES_PATH))
        .respond_with(ResponseTemplate::new(200).set_body_string("<html>ok</html>"))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(US_PATH))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({ "result": { "today": { "date": "2026-03-25" } } })),
        )
        .mount(&server)
        .await;
    let clock = TestClock::at(START);
    let toss = client(&server, &clock);

    let err = toss
        .adapter(Market::Kr)
        .quotes(&symbols(&["005930"]))
        .await
        .unwrap_err();
    assert!(matches!(err, MarketError::UnexpectedResponse { .. }));

    // 장 시간: 필수인 previousBusinessDay · nextBusinessDay가 없다.
    let err = toss
        .adapter(Market::Us)
        .calendar(day("2026-03-25"))
        .await
        .unwrap_err();
    assert!(matches!(err, MarketError::UnexpectedResponse { .. }));
}

// 검증: D5
#[tokio::test]
async fn 장_시간의_시각이_시각이_아니면_응답_모양_오류다() {
    let server = MockServer::start().await;
    mount_tokens(&server, &[TOKEN_1], 86400).await;
    let mut today = kr_open_day("2026-03-25");
    today["integrated"]["regularMarket"]["startTime"] = json!("아홉 시");
    mount_calendar(
        &server,
        KR_PATH,
        json!({ "result": {
            "today": today,
            "previousBusinessDay": kr_open_day("2026-03-24"),
            "nextBusinessDay": kr_open_day("2026-03-26"),
        }}),
    )
    .await;
    let clock = TestClock::at(START);
    let adapter = client(&server, &clock).adapter(Market::Kr);

    let err = adapter.calendar(day("2026-03-25")).await.unwrap_err();
    assert!(matches!(err, MarketError::UnexpectedResponse { .. }));
}

// 검증: D5
#[tokio::test]
async fn 토큰_응답_모양이_다르면_응답_모양_오류고_본문을_오류에_담지_않는다() {
    let server = MockServer::start().await;
    // access_token이 없고, 보낸 자격을 그대로 되돌려 주는 이상한 응답.
    Mock::given(method("POST"))
        .and(path(TOKEN_PATH))
        .respond_with(|req: &Request| {
            ResponseTemplate::new(200).set_body_json(json!({
                "token_type": "Bearer",
                "echo": String::from_utf8_lossy(&req.body),
                "client_secret": CLIENT_SECRET,
            }))
        })
        .mount(&server)
        .await;
    let clock = TestClock::at(START);
    let adapter = client(&server, &clock).adapter(Market::Kr);

    let err = adapter.quotes(&symbols(&["005930"])).await.unwrap_err();
    assert!(matches!(err, MarketError::UnexpectedResponse { .. }));
    assert_error_has_no_secret(&err);
    assert_eq!(count(&server, PRICES_PATH).await, 0);
}

// 검증: D5
#[tokio::test]
async fn 연결하지_못하면_연결_실패다() {
    let clock = TestClock::at(START);
    let adapter = TossClient::new(
        config_for(&dead_base_url()),
        TestCredentials::present(),
        clock.clone(),
    )
    .adapter(Market::Kr);

    let err = adapter.quotes(&symbols(&["005930"])).await.unwrap_err();
    assert!(matches!(err, MarketError::Connection { .. }), "{err:?}");
    assert_error_has_no_secret(&err);

    let err = adapter.calendar(day("2026-03-25")).await.unwrap_err();
    assert!(matches!(err, MarketError::Connection { .. }), "{err:?}");
}

// 검증: D5
#[tokio::test]
async fn 시간_제한을_넘으면_연결_실패고_시간_제한은_설정값이다() {
    let server = MockServer::start().await;
    mount_tokens(&server, &[TOKEN_1], 86400).await;
    Mock::given(method("GET"))
        .and(path(PRICES_PATH))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({ "result": [ price_item("005930", "72000", "KRW") ] }))
                .set_delay(Duration::from_secs(3)),
        )
        .mount(&server)
        .await;
    let clock = TestClock::at(START);
    let mut config = config_for(&server.uri());
    config.timeout = Duration::from_millis(200);
    let adapter =
        TossClient::new(config, TestCredentials::present(), clock.clone()).adapter(Market::Kr);

    let began = std::time::Instant::now();
    let err = adapter.quotes(&symbols(&["005930"])).await.unwrap_err();
    assert!(matches!(err, MarketError::Connection { .. }), "{err:?}");
    assert!(began.elapsed() < Duration::from_secs(2));
}

// 검증: D5, V31
#[tokio::test]
async fn 어떤_오류에도_자격_글자가_없다() {
    // 자격 자체의 Debug도 값을 가린다.
    let credentials = Credentials::new(CLIENT_ID, CLIENT_SECRET);
    assert_eq!(credentials.client_id(), CLIENT_ID);
    assert_eq!(credentials.client_secret(), CLIENT_SECRET);
    assert_no_secret(&format!("{credentials:?}"));
    assert_no_secret(&format!("{credentials:#?}"));
    assert_no_secret(&format!("{:?}", Some(credentials.clone())));

    let mut errors: Vec<MarketError> = vec![
        MarketError::NoCredentials,
        MarketError::IpNotAllowed,
        MarketError::AuthFailed {
            code: "invalid_client".to_string(),
        },
    ];

    // 연결 실패 (토큰 발급 단계).
    let dead = config_for(&dead_base_url());
    errors.push(verify(&dead, &credentials).await.unwrap_err());

    // 토큰 발급 거절 · 장애 · 한도 초과 · 이상한 본문.
    let token_failures = [
        oauth_error(400, "invalid_request", "OAuth 2.0 Parameter: client_secret"),
        oauth_error(401, "invalid_client", "Client authentication failed."),
        oauth_error(403, "access_denied", "IP address not allowed"),
        api_error(500, "internal-error"),
        ResponseTemplate::new(200).set_body_string(format!("client_secret={CLIENT_SECRET}")),
    ];
    for failure in token_failures {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path(TOKEN_PATH))
            .respond_with(failure)
            .mount(&server)
            .await;
        let clock = TestClock::at(START);
        let adapter = client(&server, &clock).adapter(Market::Kr);
        errors.push(adapter.quotes(&symbols(&["005930"])).await.unwrap_err());
        errors.push(adapter.calendar(day("2026-03-25")).await.unwrap_err());
        errors.push(
            verify(&config_for(&server.uri()), &credentials)
                .await
                .unwrap_err(),
        );
    }

    // 토큰을 받은 뒤의 실패.
    let api_failures = [
        api_error(401, "invalid-token"),
        api_error(401, "token-revoked"),
        api_error(403, "ip-not-allowed"),
        api_error(404, "stock-not-found"),
        api_error(500, "maintenance"),
        rate_limited("1"),
        ResponseTemplate::new(200).set_body_string(format!("토큰 {TOKEN_1} 은 이상한 응답")),
    ];
    for failure in api_failures {
        let server = MockServer::start().await;
        mount_tokens(&server, &[TOKEN_1], 86400).await;
        Mock::given(method("GET"))
            .and(path(PRICES_PATH))
            .respond_with(failure)
            .mount(&server)
            .await;
        let clock = TestClock::at(START);
        let adapter = client(&server, &clock).adapter(Market::Kr);
        errors.push(adapter.quotes(&symbols(&["005930"])).await.unwrap_err());
    }

    assert_eq!(errors.len(), 3 + 1 + 5 * 3 + 7);
    for error in &errors {
        assert_error_has_no_secret(error);
    }
}

// ───────────── 확인 함수 ─────────────

// 검증: V31
#[tokio::test]
async fn verify는_토큰_발급_한_번으로_키가_닿는지_확인한다() {
    let server = MockServer::start().await;
    mount_tokens(&server, &[TOKEN_1], 86400).await;
    let credentials = Credentials::new(CLIENT_ID, CLIENT_SECRET);

    verify(&config_for(&server.uri()), &credentials)
        .await
        .unwrap();

    let all = server.received_requests().await.unwrap();
    assert_eq!(all.len(), 1, "토큰 발급 한 번만 나간다");
    let req = &all[0];
    assert_eq!(req.method.as_str(), "POST");
    assert_eq!(req.url.path(), TOKEN_PATH);
    let form: Vec<(String, String)> = url::form_urlencoded::parse(&req.body)
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    assert!(form.contains(&("grant_type".to_string(), "client_credentials".to_string())));
    assert!(form.contains(&("client_id".to_string(), CLIENT_ID.to_string())));
    assert!(form.contains(&("client_secret".to_string(), CLIENT_SECRET.to_string())));
}

// 검증: V31
#[tokio::test]
async fn verify는_틀린_키와_허용되지_않은_ip와_연결_실패를_구분한다() {
    let credentials = Credentials::new(CLIENT_ID, CLIENT_SECRET);

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(TOKEN_PATH))
        .respond_with(oauth_error(
            401,
            "invalid_client",
            "Client authentication failed.",
        ))
        .mount(&server)
        .await;
    let err = verify(&config_for(&server.uri()), &credentials)
        .await
        .unwrap_err();
    assert_eq!(
        err,
        MarketError::AuthFailed {
            code: "invalid_client".to_string()
        }
    );
    assert_eq!(server.received_requests().await.unwrap().len(), 1);

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(TOKEN_PATH))
        .respond_with(oauth_error(403, "access_denied", "IP address not allowed"))
        .mount(&server)
        .await;
    let err = verify(&config_for(&server.uri()), &credentials)
        .await
        .unwrap_err();
    assert_eq!(err, MarketError::IpNotAllowed);

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(TOKEN_PATH))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "token_type": "Bearer" })))
        .mount(&server)
        .await;
    let err = verify(&config_for(&server.uri()), &credentials)
        .await
        .unwrap_err();
    assert!(matches!(err, MarketError::UnexpectedResponse { .. }));

    let err = verify(&config_for(&dead_base_url()), &credentials)
        .await
        .unwrap_err();
    assert!(matches!(err, MarketError::Connection { .. }));
}

// ───────────── 2차: 설정 ─────────────

// 검증: D5
#[test]
fn 발급_실패_쉼과_응답_크기_한도는_설정값이고_기본값이_있다() {
    let config = TossConfig::default();
    assert_eq!(
        config.token_failure_cooldown,
        DEFAULT_TOKEN_FAILURE_COOLDOWN
    );
    assert!(config.token_failure_cooldown > Duration::ZERO);
    assert_eq!(config.max_response_bytes, DEFAULT_MAX_RESPONSE_BYTES);
    // 현재가 200종목 응답이 들어가는 크기다.
    assert!(config.max_response_bytes >= 64 * 1024);
}

// ───────────── 2차: 토큰 유효 시간 ─────────────

// 검증: D5
#[tokio::test]
async fn 토큰_유효_시간이_다시_발급_여유_이하이면_응답_모양_오류다() {
    for (expires_in, margin) in [(60, 60), (59, 60), (1, 60), (0, 60), (300, 300)] {
        let server = MockServer::start().await;
        mount_tokens(&server, &[TOKEN_1], expires_in).await;
        mount_echo_prices(&server).await;
        let clock = TestClock::at(START);
        let mut config = config_for(&server.uri());
        config.token_refresh_margin = Duration::from_secs(margin);
        let adapter =
            TossClient::new(config, TestCredentials::present(), clock.clone()).adapter(Market::Kr);

        let err = adapter.quotes(&symbols(&["005930"])).await.unwrap_err();
        assert!(
            matches!(err, MarketError::UnexpectedResponse { .. }),
            "expires_in {expires_in} · 여유 {margin} → {err:?}"
        );
        assert_eq!(count(&server, PRICES_PATH).await, 0);
        assert_error_has_no_secret(&err);
    }
}

// 검증: D5
#[tokio::test]
async fn 토큰_유효_시간이_여유보다_길면_받아서_다시_쓴다() {
    let server = MockServer::start().await;
    mount_tokens(&server, &[TOKEN_1, TOKEN_2], 61).await;
    mount_echo_prices(&server).await;
    let clock = TestClock::at(START);
    let adapter = client(&server, &clock).adapter(Market::Kr);

    adapter.quotes(&symbols(&["005930"])).await.unwrap();
    adapter.quotes(&symbols(&["005930"])).await.unwrap();
    assert_eq!(count(&server, TOKEN_PATH).await, 1);
    let prices = requests_to(&server, PRICES_PATH).await;
    assert!(
        prices
            .iter()
            .all(|r| bearer(r) == format!("Bearer {TOKEN_1}"))
    );
}

// ───────────── 2차: 발급 실패 뒤 쉬기 ─────────────

/// 토큰 발급이 처음 `times`번은 `failure`로, 그 뒤로는 `TOKEN_1`로 답한다.
async fn mount_token_failure_then_success(
    server: &MockServer,
    failure: ResponseTemplate,
    times: u64,
) {
    Mock::given(method("POST"))
        .and(path(TOKEN_PATH))
        .respond_with(failure)
        .up_to_n_times(times)
        .mount(server)
        .await;
    mount_tokens(server, &[TOKEN_1], 86400).await;
}

// 검증: D5
#[tokio::test]
async fn 발급이_인증_실패면_쉬는_동안_다시_보내지_않고_같은_오류다() {
    let server = MockServer::start().await;
    mount_token_failure_then_success(
        &server,
        oauth_error(401, "invalid_client", "Client authentication failed."),
        1,
    )
    .await;
    mount_echo_prices(&server).await;
    mount_calendar(&server, US_PATH, us_business_day_body()).await;
    let clock = TestClock::at(START);
    let mut config = config_for(&server.uri());
    config.token_failure_cooldown = Duration::from_secs(30);
    let toss = TossClient::new(config, TestCredentials::present(), clock.clone());
    let kr = toss.adapter(Market::Kr);
    let us = toss.adapter(Market::Us);
    let auth_failed = MarketError::AuthFailed {
        code: "invalid_client".to_string(),
    };

    assert_eq!(
        kr.quotes(&symbols(&["005930"])).await.unwrap_err(),
        auth_failed
    );
    assert_eq!(count(&server, TOKEN_PATH).await, 1);

    // 쉬는 동안: 발급을 다시 보내지 않고 같은 오류. 다른 시장 어댑터도 함께 쉰다.
    assert_eq!(
        kr.quotes(&symbols(&["005930"])).await.unwrap_err(),
        auth_failed
    );
    assert_eq!(
        us.calendar(day("2026-03-25")).await.unwrap_err(),
        auth_failed
    );
    clock.advance(Duration::from_secs(29));
    assert_eq!(
        kr.quotes(&symbols(&["005930"])).await.unwrap_err(),
        auth_failed
    );
    assert_eq!(count(&server, TOKEN_PATH).await, 1);
    assert_eq!(count(&server, PRICES_PATH).await, 0);
    assert_eq!(count(&server, US_PATH).await, 0);
    // 쉬는 것은 기다리는 것이 아니다.
    assert!(clock.slept().is_empty());

    // 쉬는 시간이 지나면 다시 발급한다.
    clock.advance(Duration::from_secs(2));
    let quotes = kr.quotes(&symbols(&["005930"])).await.unwrap();
    assert_eq!(quotes.len(), 1);
    assert_eq!(count(&server, TOKEN_PATH).await, 2);
}

// 검증: D5
#[tokio::test]
async fn 발급이_허용_ip_아님이면_쉬고_쉬는_시간은_설정값이다() {
    let server = MockServer::start().await;
    mount_token_failure_then_success(
        &server,
        oauth_error(403, "access_denied", "IP address not allowed"),
        1,
    )
    .await;
    mount_calendar(&server, KR_PATH, kr_business_day_body()).await;
    let clock = TestClock::at(START);
    let mut config = config_for(&server.uri());
    config.token_failure_cooldown = Duration::from_secs(5);
    let adapter =
        TossClient::new(config, TestCredentials::present(), clock.clone()).adapter(Market::Kr);

    assert_eq!(
        adapter.calendar(day("2026-03-25")).await.unwrap_err(),
        MarketError::IpNotAllowed
    );
    clock.advance(Duration::from_secs(4));
    assert_eq!(
        adapter.calendar(day("2026-03-25")).await.unwrap_err(),
        MarketError::IpNotAllowed
    );
    assert_eq!(count(&server, TOKEN_PATH).await, 1);
    assert_eq!(count(&server, KR_PATH).await, 0);

    clock.advance(Duration::from_secs(2));
    adapter.calendar(day("2026-03-25")).await.unwrap();
    assert_eq!(count(&server, TOKEN_PATH).await, 2);
}

// 검증: D5
#[tokio::test]
async fn 장애나_이상한_응답으로_발급이_실패하면_쉬지_않는다() {
    let failures = [
        api_error(500, "internal-error"),
        api_error(503, "maintenance"),
        ResponseTemplate::new(200).set_body_json(json!({ "token_type": "Bearer" })),
    ];
    for failure in failures {
        let server = MockServer::start().await;
        mount_token_failure_then_success(&server, failure, 1).await;
        mount_echo_prices(&server).await;
        let clock = TestClock::at(START);
        let adapter = client(&server, &clock).adapter(Market::Kr);

        adapter.quotes(&symbols(&["005930"])).await.unwrap_err();
        // 시각을 옮기지 않아도 바로 다시 발급한다.
        let quotes = adapter.quotes(&symbols(&["005930"])).await.unwrap();
        assert_eq!(quotes.len(), 1);
        assert_eq!(count(&server, TOKEN_PATH).await, 2);
    }
}

// ───────────── 2차: 종목 글자 확인 ─────────────

// 검증: D5
#[tokio::test]
async fn 틀린_종목이_하나라도_있으면_아무_요청도_보내지_않고_오류다() {
    let mut many: Vec<Symbol> = (0..250).map(|i| Symbol::new(format!("S{i:04}"))).collect();
    // 두 번째 묶음(201번째 뒤)에 틀린 종목이 있어도 첫 묶음조차 보내지 않는다.
    many[230] = Symbol::new("S0230?X");
    let too_long = "A".repeat(33);
    let cases: Vec<(Vec<Symbol>, String)> = vec![
        (symbols(&["005930", "AAPL,MSFT"]), "AAPL,MSFT".to_string()),
        (symbols(&["005930&debug=1"]), "005930&debug=1".to_string()),
        (symbols(&["", "005930"]), String::new()),
        (symbols(&["삼성전자"]), "삼성전자".to_string()),
        (symbols(&[too_long.as_str()]), too_long.clone()),
        (many, "S0230?X".to_string()),
    ];
    for (list, bad) in cases {
        let server = MockServer::start().await;
        mount_tokens(&server, &[TOKEN_1], 86400).await;
        mount_echo_prices(&server).await;
        let clock = TestClock::at(START);
        let adapter = client(&server, &clock).adapter(Market::Kr);

        let err = adapter.quotes(&list).await.unwrap_err();
        assert_eq!(err, MarketError::InvalidSymbol, "{bad:?}");
        assert!(
            server.received_requests().await.unwrap().is_empty(),
            "틀린 종목이 있으면 토큰 발급도 현재가도 보내지 않는다: {bad:?}"
        );
        if !bad.is_empty() {
            assert!(!err.to_string().contains(&bad));
            assert!(!format!("{err:?}").contains(&bad));
        }
    }
}

// 검증: D5
#[tokio::test]
async fn 점과_빼기가_든_종목은_그대로_묻는다() {
    let server = MockServer::start().await;
    mount_tokens(&server, &[TOKEN_1], 86400).await;
    mount_echo_prices(&server).await;
    let clock = TestClock::at(START);
    let adapter = client(&server, &clock).adapter(Market::Us);

    let quotes = adapter.quotes(&symbols(&["BRK.B", "BF-B"])).await.unwrap();
    assert_eq!(quotes.len(), 2);
    let req = &requests_to(&server, PRICES_PATH).await[0];
    assert_eq!(query_value(req, "symbols").as_deref(), Some("BRK.B,BF-B"));
}

// ───────────── 2차: 가격 · 요청하지 않은 종목 ─────────────

// 검증: D5
#[tokio::test]
async fn 영_이하의_가격은_응답_모양_오류다() {
    for price in ["0", "0.00", "-0", "-1", "-0.0001", "-72000"] {
        let server = MockServer::start().await;
        mount_tokens(&server, &[TOKEN_1], 86400).await;
        mount_prices(
            &server,
            json!({ "result": [
                price_item("005930", "72000", "KRW"),
                price_item("000660", price, "KRW"),
            ]}),
        )
        .await;
        let clock = TestClock::at(START);
        let adapter = client(&server, &clock).adapter(Market::Kr);

        let err = adapter
            .quotes(&symbols(&["005930", "000660"]))
            .await
            .unwrap_err();
        assert!(
            matches!(err, MarketError::UnexpectedResponse { .. }),
            "{price} → {err:?}"
        );
    }
}

// 검증: D5
#[tokio::test]
async fn 아주_작은_양의_가격은_받는다() {
    let server = MockServer::start().await;
    mount_tokens(&server, &[TOKEN_1], 86400).await;
    mount_prices(
        &server,
        json!({ "result": [ price_item("TINY", "0.0001", "USD") ] }),
    )
    .await;
    let clock = TestClock::at(START);
    let adapter = client(&server, &clock).adapter(Market::Us);

    let quotes = adapter.quotes(&symbols(&["TINY"])).await.unwrap();
    assert_eq!(quotes[0].price, dec("0.0001"));
}

// 검증: D5
#[tokio::test]
async fn 요청하지_않은_종목이_응답에_있으면_버린다() {
    let server = MockServer::start().await;
    mount_tokens(&server, &[TOKEN_1], 86400).await;
    mount_prices(
        &server,
        json!({ "result": [
            price_item("000660", "185500", "KRW"),
            price_item("005930", "72000", "KRW"),
            price_item("AAPL", "185.70", "USD"),
        ]}),
    )
    .await;
    let clock = TestClock::at(START);
    let adapter = client(&server, &clock).adapter(Market::Kr);

    let quotes = adapter.quotes(&symbols(&["005930"])).await.unwrap();
    assert_eq!(quotes.len(), 1);
    assert_eq!(quotes[0].symbol, Symbol::new("005930"));
    assert_eq!(quotes[0].price, dec("72000"));

    let quotes = adapter
        .quotes(&symbols(&["005930", "000660"]))
        .await
        .unwrap();
    let got: HashSet<String> = quotes
        .iter()
        .map(|q| q.symbol.as_str().to_string())
        .collect();
    let want: HashSet<String> = ["005930", "000660"].into_iter().map(String::from).collect();
    assert_eq!(got, want);

    // 요청한 종목이 응답에 하나도 없으면 빈 목록이다(오류가 아니다).
    let quotes = adapter.quotes(&symbols(&["035720"])).await.unwrap();
    assert!(quotes.is_empty());
}

// ───────────── 2차: 평문 주소 ─────────────

// 검증: D5, V31
#[tokio::test]
async fn 기본_주소가_https가_아니면_요청을_보내지_않고_거부한다() {
    let credentials = Credentials::new(CLIENT_ID, CLIENT_SECRET);
    for base in [
        "http://192.0.2.1",
        "http://192.0.2.1:8080",
        "http://10.0.0.1",
        "ftp://192.0.2.1",
    ] {
        let clock = TestClock::at(START);
        let mut config = config_for(base);
        config.timeout = Duration::from_millis(200);
        let adapter = TossClient::new(config.clone(), TestCredentials::present(), clock.clone())
            .adapter(Market::Kr);

        let err = adapter.quotes(&symbols(&["005930"])).await.unwrap_err();
        assert_eq!(err, MarketError::InsecureBaseUrl, "{base}");
        let err = adapter.calendar(day("2026-03-25")).await.unwrap_err();
        assert_eq!(err, MarketError::InsecureBaseUrl, "{base}");
        let err = verify(&config, &credentials).await.unwrap_err();
        assert_eq!(err, MarketError::InsecureBaseUrl, "{base}");
        assert_error_has_no_secret(&err);
    }
}

// 검증: D5
#[tokio::test]
async fn https면_루프백이_아니어도_주소_때문에_거부하지_않는다() {
    let clock = TestClock::at(START);
    let mut config = config_for("https://192.0.2.1");
    config.timeout = Duration::from_millis(200);
    let adapter =
        TossClient::new(config, TestCredentials::present(), clock.clone()).adapter(Market::Kr);

    // 닿지 않는 주소라 연결 실패다(주소 거부가 아니다).
    let err = adapter.quotes(&symbols(&["005930"])).await.unwrap_err();
    assert!(matches!(err, MarketError::Connection { .. }), "{err:?}");
}

// 검증: D5
#[tokio::test]
async fn 루프백_주소의_가짜_서버는_http여도_쓴다() {
    let server = MockServer::start().await;
    assert!(server.uri().starts_with("http://127.0.0.1"));
    mount_tokens(&server, &[TOKEN_1], 86400).await;
    mount_echo_prices(&server).await;
    let clock = TestClock::at(START);
    let adapter = client(&server, &clock).adapter(Market::Kr);

    assert_eq!(
        adapter.quotes(&symbols(&["005930"])).await.unwrap().len(),
        1
    );
    verify(
        &config_for(&server.uri()),
        &Credentials::new(CLIENT_ID, CLIENT_SECRET),
    )
    .await
    .unwrap();
}

// ───────────── 2차: 응답 크기 한도 ─────────────

/// JSON 글자 뒤를 빈칸으로 채워 정확히 `size` 바이트로 만든다(빈칸은 JSON으로 읽힌다).
fn padded(value: &Value, size: usize) -> String {
    let mut text = value.to_string();
    assert!(text.len() <= size);
    text.push_str(&" ".repeat(size - text.len()));
    text
}

fn json_body(text: String) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_raw(text, "application/json")
}

// 검증: D5
#[tokio::test]
async fn 현재가_응답이_정한_크기를_넘으면_오류고_딱_그_크기면_읽는다() {
    let limit = 2048;
    let body = json!({ "result": [ price_item("005930", "72000", "KRW") ] });
    for (size, ok) in [(limit, true), (limit + 1, false), (limit * 50, false)] {
        let server = MockServer::start().await;
        mount_tokens(&server, &[TOKEN_1], 86400).await;
        Mock::given(method("GET"))
            .and(path(PRICES_PATH))
            .respond_with(json_body(padded(&body, size)))
            .mount(&server)
            .await;
        let clock = TestClock::at(START);
        let mut config = config_for(&server.uri());
        config.max_response_bytes = limit;
        let adapter =
            TossClient::new(config, TestCredentials::present(), clock.clone()).adapter(Market::Kr);

        let result = adapter.quotes(&symbols(&["005930"])).await;
        if ok {
            assert_eq!(result.unwrap()[0].price, dec("72000"), "{size}");
        } else {
            let err = result.unwrap_err();
            assert!(
                matches!(err, MarketError::UnexpectedResponse { .. }),
                "{size} → {err:?}"
            );
        }
    }
}

// 검증: D5
#[tokio::test]
async fn 장_시간과_토큰_응답도_크기_한도를_넘으면_오류다() {
    let limit = 4096;

    // 장 시간.
    let server = MockServer::start().await;
    mount_tokens(&server, &[TOKEN_1], 86400).await;
    Mock::given(method("GET"))
        .and(path(KR_PATH))
        .respond_with(json_body(padded(&kr_business_day_body(), limit + 1)))
        .mount(&server)
        .await;
    let clock = TestClock::at(START);
    let mut config = config_for(&server.uri());
    config.max_response_bytes = limit;
    let adapter = TossClient::new(config.clone(), TestCredentials::present(), clock.clone())
        .adapter(Market::Kr);
    let err = adapter.calendar(day("2026-03-25")).await.unwrap_err();
    assert!(
        matches!(err, MarketError::UnexpectedResponse { .. }),
        "{err:?}"
    );

    // 토큰: 크기를 넘는 본문에 자격이 들어 있어도 오류에 담지 않는다.
    let server = MockServer::start().await;
    let token = json!({
        "access_token": TOKEN_1,
        "token_type": "Bearer",
        "expires_in": 86400,
        "echo": CLIENT_SECRET,
    });
    Mock::given(method("POST"))
        .and(path(TOKEN_PATH))
        .respond_with(json_body(padded(&token, limit + 1)))
        .mount(&server)
        .await;
    mount_echo_prices(&server).await;
    let mut config = config_for(&server.uri());
    config.max_response_bytes = limit;
    let adapter = TossClient::new(config.clone(), TestCredentials::present(), clock.clone())
        .adapter(Market::Kr);
    let err = adapter.quotes(&symbols(&["005930"])).await.unwrap_err();
    assert!(
        matches!(err, MarketError::UnexpectedResponse { .. }),
        "{err:?}"
    );
    assert_error_has_no_secret(&err);
    assert_eq!(count(&server, PRICES_PATH).await, 0);

    let err = verify(&config, &Credentials::new(CLIENT_ID, CLIENT_SECRET))
        .await
        .unwrap_err();
    assert!(
        matches!(err, MarketError::UnexpectedResponse { .. }),
        "{err:?}"
    );
}
