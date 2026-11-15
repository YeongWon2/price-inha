//! 3차 작업(docs/dev/modules/market.md "3차 작업", decisions.md 2026-11-15 조정자 추천).
//! 로컬 가짜 서버(wiremock · 127.0.0.1의 손으로 만든 HTTP 서버)만 쓴다.

use std::collections::HashSet;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use futures::future::join_all;
use pi_market::{
    Clock, CredentialSource, Credentials, Currency, Market, MarketAdapter, MarketCalendar,
    MarketError, Quote, StalePolicy, Symbol, TossClient, TossConfig, TradingDay, is_stale,
};
use rust_decimal::Decimal;
use serde_json::{Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

const CLIENT_ID: &str = "시험용-클라이언트-아이디-0003";
const CLIENT_SECRET: &str = "시험용-클라이언트-비밀-0003";
const OTHER_ID: &str = "시험용-다른-아이디-0004";
const OTHER_SECRET: &str = "시험용-다른-비밀-0004";
const TOKEN_1: &str = "test-access-token-1";
const TOKEN_PATH: &str = "/oauth2/token";
const PRICES_PATH: &str = "/api/v1/prices";
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

/// 테스트가 정하는 자격. 도중에 바꿀 수 있다.
struct TestCredentials(Mutex<Option<Credentials>>);

impl TestCredentials {
    fn present() -> Arc<Self> {
        Arc::new(Self(Mutex::new(Some(Credentials::new(
            CLIENT_ID,
            CLIENT_SECRET,
        )))))
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
        ..TossConfig::default()
    }
}

fn client(base_url: &str, clock: &Arc<TestClock>) -> TossClient {
    TossClient::new(
        config_for(base_url),
        TestCredentials::present(),
        clock.clone(),
    )
}

fn token_body(token: &str, expires_in: u64) -> Value {
    json!({ "access_token": token, "token_type": "Bearer", "expires_in": expires_in })
}

async fn mount_token(server: &MockServer) {
    Mock::given(method("POST"))
        .and(path(TOKEN_PATH))
        .respond_with(ResponseTemplate::new(200).set_body_json(token_body(TOKEN_1, 86400)))
        .mount(server)
        .await;
}

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
    mount_token(server).await;
}

fn price_item(symbol: &str, price: &str, currency: &str) -> Value {
    json!({
        "symbol": symbol,
        "timestamp": "2026-03-25T09:30:00.123+09:00",
        "lastPrice": price,
        "currency": currency,
    })
}

async fn mount_prices(server: &MockServer, body: Value) {
    Mock::given(method("GET"))
        .and(path(PRICES_PATH))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .mount(server)
        .await;
}

/// 달라는 종목마다 100원짜리 시세를 돌려준다. `zero`에 든 종목만 0원이다.
async fn mount_echo_prices_with_zero(server: &MockServer, zero: &'static [&'static str]) {
    Mock::given(method("GET"))
        .and(path(PRICES_PATH))
        .respond_with(move |req: &Request| {
            let wanted = query_value(req, "symbols").unwrap_or_default();
            let items: Vec<Value> = wanted
                .split(',')
                .filter(|s| !s.is_empty())
                .map(|s| {
                    let price = if zero.contains(&s) { "0" } else { "100" };
                    price_item(s, price, "KRW")
                })
                .collect();
            ResponseTemplate::new(200).set_body_json(json!({ "result": items }))
        })
        .mount(server)
        .await;
}

fn api_error(status: u16, code: &str) -> ResponseTemplate {
    ResponseTemplate::new(status).set_body_json(json!({
        "error": {
            "requestId": "01HXYZABCDEFG123456789",
            "code": code,
            "message": "요청을 처리하지 못했습니다.",
        }
    }))
}

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

async fn count(server: &MockServer, wanted: &str) -> usize {
    server
        .received_requests()
        .await
        .unwrap()
        .into_iter()
        .filter(|r| r.url.path() == wanted)
        .count()
}

fn names(quotes: &[Quote]) -> HashSet<String> {
    quotes
        .iter()
        .map(|q| q.symbol.as_str().to_string())
        .collect()
}

/// JSON 글자 뒤를 빈칸으로 채워 정확히 `size` 바이트로 만든다(빈칸은 JSON으로 읽힌다).
fn padded(value: &Value, size: usize) -> String {
    let mut text = value.to_string();
    assert!(text.len() <= size);
    text.push_str(&" ".repeat(size - text.len()));
    text
}

fn assert_has_no(err: &MarketError, needles: &[&str]) {
    let shown = format!("{err} {err:?}");
    for needle in needles {
        assert!(
            !shown.contains(needle),
            "오류에 {needle:?}가 들어 있다: {shown}"
        );
    }
}

// ───────────── 손으로 만든 HTTP 서버(길이 없는 chunked 응답) ─────────────

/// 손으로 만든 응답 하나. `chunked`면 `Content-Length` 없이 `Transfer-Encoding: chunked`로 보낸다.
struct RawResponse {
    status: u16,
    body: Vec<u8>,
    chunked: bool,
}

impl RawResponse {
    fn json(status: u16, body: String) -> Self {
        Self {
            status,
            body: body.into_bytes(),
            chunked: false,
        }
    }

    fn chunked(status: u16, body: String) -> Self {
        Self {
            status,
            body: body.into_bytes(),
            chunked: true,
        }
    }
}

type Handler = dyn Fn(&str, &str) -> RawResponse + Send + Sync;

/// 127.0.0.1의 아무 포트에서 듣는 작은 HTTP/1.1 서버. 받은 요청의 경로를 적어 둔다.
struct RawServer {
    base_url: String,
    paths: Arc<Mutex<Vec<String>>>,
}

impl RawServer {
    fn start(handler: impl Fn(&str, &str) -> RawResponse + Send + Sync + 'static) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let paths = Arc::new(Mutex::new(Vec::new()));
        let handler: Arc<Handler> = Arc::new(handler);
        let seen = paths.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { continue };
                let handler = handler.clone();
                let seen = seen.clone();
                std::thread::spawn(move || serve_one(stream, handler.as_ref(), &seen));
            }
        });
        Self {
            base_url: format!("http://127.0.0.1:{port}"),
            paths,
        }
    }

    fn count(&self, wanted: &str) -> usize {
        self.paths
            .lock()
            .unwrap()
            .iter()
            .filter(|p| p.as_str() == wanted)
            .count()
    }
}

/// 요청 하나를 읽고 답한 뒤 연결을 닫는다. 받는 쪽이 먼저 끊어도(쓰기 실패) 그냥 끝낸다.
fn serve_one(mut stream: TcpStream, handler: &Handler, seen: &Mutex<Vec<String>>) {
    let Some((method, target)) = read_request(&mut stream) else {
        return;
    };
    let route = target.split('?').next().unwrap_or("").to_string();
    seen.lock().unwrap().push(route.clone());
    let response = handler(&method, &route);
    let _ = write_response(&mut stream, &response);
}

fn read_request(stream: &mut TcpStream) -> Option<(String, String)> {
    let mut buf = Vec::new();
    let mut byte = [0u8; 1];
    while !buf.ends_with(b"\r\n\r\n") {
        if stream.read(&mut byte).ok()? == 0 {
            return None;
        }
        buf.push(byte[0]);
    }
    let head = String::from_utf8_lossy(&buf).into_owned();
    let mut lines = head.split("\r\n");
    let mut first = lines.next()?.split(' ');
    let method = first.next()?.to_string();
    let target = first.next()?.to_string();
    let length = lines
        .filter_map(|line| line.split_once(':'))
        .find(|(name, _)| name.trim().eq_ignore_ascii_case("content-length"))
        .and_then(|(_, value)| value.trim().parse::<usize>().ok())
        .unwrap_or(0);
    let mut body = vec![0u8; length];
    stream.read_exact(&mut body).ok()?;
    Some((method, target))
}

fn write_response(stream: &mut TcpStream, response: &RawResponse) -> std::io::Result<()> {
    let mut head = format!(
        "HTTP/1.1 {} X\r\nContent-Type: application/json\r\nConnection: close\r\n",
        response.status
    );
    if response.chunked {
        head.push_str("Transfer-Encoding: chunked\r\n\r\n");
        stream.write_all(head.as_bytes())?;
        for piece in response.body.chunks(1024) {
            stream.write_all(format!("{:x}\r\n", piece.len()).as_bytes())?;
            stream.write_all(piece)?;
            stream.write_all(b"\r\n")?;
        }
        stream.write_all(b"0\r\n\r\n")?;
    } else {
        head.push_str(&format!("Content-Length: {}\r\n\r\n", response.body.len()));
        stream.write_all(head.as_bytes())?;
        stream.write_all(&response.body)?;
    }
    stream.flush()
}

/// 토큰은 늘 주고, 현재가는 `prices`가 정한다.
fn raw_server(prices: impl Fn() -> RawResponse + Send + Sync + 'static) -> RawServer {
    RawServer::start(move |method, route| match (method, route) {
        ("POST", TOKEN_PATH) => RawResponse::json(200, token_body(TOKEN_1, 86400).to_string()),
        ("GET", PRICES_PATH) => prices(),
        _ => RawResponse::json(404, json!({}).to_string()),
    })
}

fn one_price_body() -> Value {
    json!({ "result": [ price_item("005930", "72000", "KRW") ] })
}

// ───────────── 3차: 요청하지 않은 종목의 0원 이하 가격은 버린다 ─────────────

// 검증: D5
#[tokio::test]
async fn 요청하지_않은_종목의_영_이하_가격은_버리고_오류가_아니다() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    mount_prices(
        &server,
        json!({ "result": [
            price_item("AAPL", "185.70", "USD"),
            price_item("ZERO", "0", "USD"),
            price_item("NEG", "-3.5", "USD"),
        ]}),
    )
    .await;
    let clock = TestClock::at(START);
    let adapter = client(&server.uri(), &clock).adapter(Market::Us);

    let quotes = adapter.quotes(&symbols(&["AAPL"])).await.unwrap();
    assert_eq!(quotes.len(), 1);
    assert_eq!(quotes[0].symbol, Symbol::new("AAPL"));
    assert_eq!(quotes[0].price, dec("185.70"));
}

// 검증: D5
#[tokio::test]
async fn 나눠_부를_때도_요청하지_않은_영_이하_종목은_버리고_나머지를_모두_돌려준다() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    // 묶음마다 요청하지 않은 0원 · 음수 종목이 섞여 온다.
    Mock::given(method("GET"))
        .and(path(PRICES_PATH))
        .respond_with(|req: &Request| {
            let wanted = query_value(req, "symbols").unwrap_or_default();
            let mut items: Vec<Value> = wanted
                .split(',')
                .filter(|s| !s.is_empty())
                .map(|s| price_item(s, "100", "KRW"))
                .collect();
            items.push(price_item("ZERO", "0", "KRW"));
            items.push(price_item("NEG", "-1", "KRW"));
            ResponseTemplate::new(200).set_body_json(json!({ "result": items }))
        })
        .mount(&server)
        .await;
    let clock = TestClock::at(START);
    let adapter = client(&server.uri(), &clock).adapter(Market::Kr);
    let many: Vec<Symbol> = (0..250).map(|i| Symbol::new(format!("S{i:04}"))).collect();

    let quotes = adapter.quotes(&many).await.unwrap();
    assert_eq!(count(&server, PRICES_PATH).await, 2);
    assert_eq!(quotes.len(), 250);
    let got = names(&quotes);
    assert!(!got.contains("ZERO"));
    assert!(!got.contains("NEG"));
    assert!(quotes.iter().all(|q| q.price == dec("100")));
}

// 검증: D5
#[tokio::test]
async fn 영_이하가_아닌_가격_모양_오류는_그대로_응답_모양_오류다() {
    // 빼는 것은 0원 이하뿐이다. 숫자로 읽히지 않는 가격은 지금처럼 오류로 끝낸다.
    let server = MockServer::start().await;
    mount_token(&server).await;
    mount_prices(
        &server,
        json!({ "result": [
            price_item("005930", "72000", "KRW"),
            price_item("000660", "abc", "KRW"),
        ]}),
    )
    .await;
    let clock = TestClock::at(START);
    let adapter = client(&server.uri(), &clock).adapter(Market::Kr);

    let err = adapter
        .quotes(&symbols(&["005930", "000660"]))
        .await
        .unwrap_err();
    assert!(
        matches!(err, MarketError::UnexpectedResponse { .. }),
        "{err:?}"
    );
}

// ───────────── 3차: 토큰 유효 시간이 짧은 응답도 쉬기에 넣는다 ─────────────

// 검증: D5
#[tokio::test]
async fn 토큰_유효_시간이_여유_이하인_응답도_쉬는_동안_다시_발급하지_않는다() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(TOKEN_PATH))
        .respond_with(ResponseTemplate::new(200).set_body_json(token_body(TOKEN_1, 30)))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    mount_token(&server).await;
    mount_echo_prices_with_zero(&server, &[]).await;
    Mock::given(method("GET"))
        .and(path(US_PATH))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .mount(&server)
        .await;
    let clock = TestClock::at(START);
    let mut config = config_for(&server.uri());
    config.token_failure_cooldown = Duration::from_secs(30);
    let toss = TossClient::new(config, TestCredentials::present(), clock.clone());
    let kr = toss.adapter(Market::Kr);
    let us = toss.adapter(Market::Us);

    let first = kr.quotes(&symbols(&["005930"])).await.unwrap_err();
    assert!(
        matches!(first, MarketError::UnexpectedResponse { .. }),
        "{first:?}"
    );
    assert_eq!(count(&server, TOKEN_PATH).await, 1);

    // 쉬는 동안: 발급을 다시 보내지 않고 같은 오류. 다른 시장 어댑터도 함께 쉰다.
    assert_eq!(kr.quotes(&symbols(&["005930"])).await.unwrap_err(), first);
    assert_eq!(us.calendar(day("2026-03-25")).await.unwrap_err(), first);
    clock.advance(Duration::from_secs(29));
    assert_eq!(kr.quotes(&symbols(&["005930"])).await.unwrap_err(), first);
    assert_eq!(count(&server, TOKEN_PATH).await, 1);
    assert_eq!(count(&server, PRICES_PATH).await, 0);
    assert_eq!(count(&server, US_PATH).await, 0);
    assert!(clock.slept().is_empty());

    // 쉬는 시간이 지나면 다시 발급한다.
    clock.advance(Duration::from_secs(2));
    let quotes = kr.quotes(&symbols(&["005930"])).await.unwrap();
    assert_eq!(quotes.len(), 1);
    assert_eq!(count(&server, TOKEN_PATH).await, 2);
}

// 검증: D5, V31
#[tokio::test]
async fn 토큰_유효_시간이_짧아_쉬는_중에도_자격이_바뀌면_바로_발급한다() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(TOKEN_PATH))
        .respond_with(ResponseTemplate::new(200).set_body_json(token_body(TOKEN_1, 10)))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    mount_token(&server).await;
    mount_echo_prices_with_zero(&server, &[]).await;
    let clock = TestClock::at(START);
    let credentials = TestCredentials::present();
    let mut config = config_for(&server.uri());
    config.token_failure_cooldown = Duration::from_secs(600);
    let adapter = TossClient::new(config, credentials.clone(), clock.clone()).adapter(Market::Kr);

    adapter.quotes(&symbols(&["005930"])).await.unwrap_err();
    assert_eq!(count(&server, TOKEN_PATH).await, 1);

    credentials.put(Credentials::new(OTHER_ID, OTHER_SECRET));
    let quotes = adapter.quotes(&symbols(&["005930"])).await.unwrap();
    assert_eq!(quotes.len(), 1);
    assert_eq!(count(&server, TOKEN_PATH).await, 2);
}

// ───────────── 3차 테스트 빈칸: 쉬는 중 자격 바뀜 · 쉬는 동안 동시 호출 ─────────────

// 검증: D5, V31
#[tokio::test]
async fn 발급이_인증_실패로_쉬는_중에_자격이_바뀌면_바로_발급한다() {
    let server = MockServer::start().await;
    mount_token_failure_then_success(
        &server,
        oauth_error(401, "invalid_client", "Client authentication failed."),
        1,
    )
    .await;
    mount_echo_prices_with_zero(&server, &[]).await;
    let clock = TestClock::at(START);
    let credentials = TestCredentials::present();
    let mut config = config_for(&server.uri());
    config.token_failure_cooldown = Duration::from_secs(600);
    let adapter = TossClient::new(config, credentials.clone(), clock.clone()).adapter(Market::Kr);

    let err = adapter.quotes(&symbols(&["005930"])).await.unwrap_err();
    assert_eq!(
        err,
        MarketError::AuthFailed {
            code: "invalid_client".to_string()
        }
    );
    // 같은 자격이면 쉰다.
    adapter.quotes(&symbols(&["005930"])).await.unwrap_err();
    assert_eq!(count(&server, TOKEN_PATH).await, 1);

    // 자격이 바뀌면 시각을 옮기지 않아도 바로 발급한다.
    credentials.put(Credentials::new(OTHER_ID, OTHER_SECRET));
    let quotes = adapter.quotes(&symbols(&["005930"])).await.unwrap();
    assert_eq!(quotes.len(), 1);
    assert_eq!(count(&server, TOKEN_PATH).await, 2);
    let token_requests: Vec<Request> = server
        .received_requests()
        .await
        .unwrap()
        .into_iter()
        .filter(|r| r.url.path() == TOKEN_PATH)
        .collect();
    let second = String::from_utf8_lossy(&token_requests[1].body).into_owned();
    let encoded_other: String = url::form_urlencoded::byte_serialize(OTHER_ID.as_bytes()).collect();
    assert!(second.contains(&encoded_other), "새 자격으로 발급한다");
    assert!(clock.slept().is_empty());
}

// 검증: D5
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn 쉬는_동안_동시에_온_호출은_모두_같은_오류고_발급은_한_번이다() {
    let server = MockServer::start().await;
    mount_token_failure_then_success(
        &server,
        oauth_error(403, "access_denied", "IP address not allowed"),
        1,
    )
    .await;
    mount_echo_prices_with_zero(&server, &[]).await;
    let clock = TestClock::at(START);
    let mut config = config_for(&server.uri());
    config.token_failure_cooldown = Duration::from_secs(60);
    let toss = TossClient::new(config, TestCredentials::present(), clock.clone());
    let kr = toss.adapter(Market::Kr);
    let us = toss.adapter(Market::Us);

    assert_eq!(
        kr.quotes(&symbols(&["005930"])).await.unwrap_err(),
        MarketError::IpNotAllowed
    );

    let wanted = symbols(&["005930"]);
    let kr_calls = (0..8).map(|_| kr.quotes(&wanted));
    let us_calls = (0..8).map(|_| us.quotes(&wanted));
    let results = join_all(kr_calls.chain(us_calls)).await;
    for result in results {
        assert_eq!(result.unwrap_err(), MarketError::IpNotAllowed);
    }
    assert_eq!(count(&server, TOKEN_PATH).await, 1);
    assert_eq!(count(&server, PRICES_PATH).await, 0);
}

// 검증: D5
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn 발급이_실패하는_중에_동시에_온_호출도_발급은_한_번이고_모두_같은_오류다() {
    let server = MockServer::start().await;
    // 실패 응답이 늦게 와서, 그 사이 다른 호출들이 기다리게 한다.
    mount_token_failure_then_success(
        &server,
        oauth_error(401, "invalid_client", "Client authentication failed.")
            .set_delay(Duration::from_millis(200)),
        1,
    )
    .await;
    mount_echo_prices_with_zero(&server, &[]).await;
    let clock = TestClock::at(START);
    let mut config = config_for(&server.uri());
    config.token_failure_cooldown = Duration::from_secs(60);
    let toss = TossClient::new(config, TestCredentials::present(), clock.clone());
    let kr = toss.adapter(Market::Kr);
    let us = toss.adapter(Market::Us);
    let auth_failed = MarketError::AuthFailed {
        code: "invalid_client".to_string(),
    };

    let wanted = symbols(&["005930"]);
    let kr_calls = (0..6).map(|_| kr.quotes(&wanted));
    let us_calls = (0..6).map(|_| us.quotes(&wanted));
    let results = join_all(kr_calls.chain(us_calls)).await;
    for result in results {
        assert_eq!(result.unwrap_err(), auth_failed);
    }
    assert_eq!(count(&server, TOKEN_PATH).await, 1);
    assert_eq!(count(&server, PRICES_PATH).await, 0);
}

// ───────────── 3차 테스트 빈칸: 길이 없는(chunked) 응답 · 2xx 아닌 큰 응답 ─────────────

// 검증: D5
#[tokio::test]
async fn 길이_없는_chunked_응답도_정한_크기를_넘으면_오류고_딱_그_크기면_읽는다() {
    let limit = 2048;
    for (size, ok) in [(limit, true), (limit + 1, false), (limit * 2048, false)] {
        let server = raw_server(move || RawResponse::chunked(200, padded(&one_price_body(), size)));
        let clock = TestClock::at(START);
        let mut config = config_for(&server.base_url);
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
        assert_eq!(server.count(PRICES_PATH), 1, "{size}");
    }
}

// 검증: D5
#[tokio::test]
async fn 길이_없는_chunked_토큰_응답도_크기_한도를_넘으면_오류고_자격을_담지_않는다() {
    let limit = 1024;
    let token = json!({
        "access_token": TOKEN_1,
        "token_type": "Bearer",
        "expires_in": 86400,
        "echo": CLIENT_SECRET,
    });
    let body = padded(&token, limit * 8);
    let server = RawServer::start(move |method, route| match (method, route) {
        ("POST", TOKEN_PATH) => RawResponse::chunked(200, body.clone()),
        _ => RawResponse::json(200, one_price_body().to_string()),
    });
    let clock = TestClock::at(START);
    let mut config = config_for(&server.base_url);
    config.max_response_bytes = limit;
    let adapter =
        TossClient::new(config, TestCredentials::present(), clock.clone()).adapter(Market::Kr);

    let err = adapter.quotes(&symbols(&["005930"])).await.unwrap_err();
    assert!(
        matches!(err, MarketError::UnexpectedResponse { .. }),
        "{err:?}"
    );
    assert_has_no(&err, &[CLIENT_SECRET, CLIENT_ID, TOKEN_1]);
    assert_eq!(server.count(PRICES_PATH), 0);
}

// 검증: D5
#[tokio::test]
async fn 크기를_넘는_2xx_아닌_응답은_본문을_읽지_않고_상태로만_구분한다() {
    let limit = 1024;
    // 본문의 코드(`maintenance`)를 읽으면 안 된다: 크기를 넘었기 때문이다.
    let big_error = json!({ "error": {
        "requestId": "01HXYZABCDEFG123456789",
        "code": "maintenance",
        "message": "요청을 처리하지 못했습니다.",
    }});

    for chunked in [false, true] {
        let body = padded(&big_error, limit * 4);
        let server = raw_server(move || {
            if chunked {
                RawResponse::chunked(503, body.clone())
            } else {
                RawResponse::json(503, body.clone())
            }
        });
        let clock = TestClock::at(START);
        let mut config = config_for(&server.base_url);
        config.max_response_bytes = limit;
        let adapter =
            TossClient::new(config, TestCredentials::present(), clock.clone()).adapter(Market::Kr);

        let err = adapter.quotes(&symbols(&["005930"])).await.unwrap_err();
        assert_eq!(
            err,
            MarketError::Unavailable {
                status: 503,
                code: None
            },
            "chunked {chunked}"
        );
        assert_has_no(&err, &["maintenance", "01HXYZABCDEFG123456789"]);
    }

    // 견주어: 크기 안의 같은 본문은 코드를 읽는다.
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("GET"))
        .and(path(PRICES_PATH))
        .respond_with(api_error(503, "maintenance"))
        .mount(&server)
        .await;
    let clock = TestClock::at(START);
    let mut config = config_for(&server.uri());
    config.max_response_bytes = limit;
    let adapter =
        TossClient::new(config, TestCredentials::present(), clock.clone()).adapter(Market::Kr);
    assert_eq!(
        adapter.quotes(&symbols(&["005930"])).await.unwrap_err(),
        MarketError::Unavailable {
            status: 503,
            code: Some("maintenance".to_string())
        }
    );
}

// 검증: D5
#[tokio::test]
async fn 크기를_넘는_401은_코드를_모르므로_다시_발급하지_않는다() {
    let limit = 1024;
    let big = padded(
        &json!({ "error": { "requestId": "r", "code": "expired-token", "message": "m" } }),
        limit * 4,
    );
    let server = raw_server(move || RawResponse::chunked(401, big.clone()));
    let clock = TestClock::at(START);
    let mut config = config_for(&server.base_url);
    config.max_response_bytes = limit;
    let adapter =
        TossClient::new(config, TestCredentials::present(), clock.clone()).adapter(Market::Kr);

    let err = adapter.quotes(&symbols(&["005930"])).await.unwrap_err();
    match &err {
        MarketError::AuthFailed { code } => assert_ne!(code, "expired-token"),
        other => panic!("인증 실패여야 한다: {other:?}"),
    }
    assert_eq!(server.count(TOKEN_PATH), 1);
    assert_eq!(server.count(PRICES_PATH), 1);
}

// 검증: D5
#[tokio::test]
async fn 크기를_넘는_429도_retry_after만큼_기다렸다가_다시_시도한다() {
    let limit = 1024;
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("GET"))
        .and(path(PRICES_PATH))
        .respond_with(
            ResponseTemplate::new(429)
                .insert_header("Retry-After", "2")
                .set_body_raw(padded(&json!({}), limit * 4), "application/json"),
        )
        .up_to_n_times(1)
        .mount(&server)
        .await;
    mount_prices(&server, one_price_body()).await;
    let clock = TestClock::at(START);
    let mut config = config_for(&server.uri());
    config.max_response_bytes = limit;
    let adapter =
        TossClient::new(config, TestCredentials::present(), clock.clone()).adapter(Market::Kr);

    let quotes = adapter.quotes(&symbols(&["005930"])).await.unwrap();
    assert_eq!(quotes[0].price, dec("72000"));
    assert_eq!(clock.slept(), vec![Duration::from_secs(2)]);
    assert_eq!(count(&server, PRICES_PATH).await, 2);
}

// ───────────── 3차: 미 동부 서머타임(손으로 센다) ─────────────

/// 세션 없는 날들로 만든 미국 달력. 덮는 범위는 `previous` 자정부터 `next` 다음 날 자정까지다.
fn us_calendar(previous: &str, next: &str) -> MarketCalendar {
    let empty = |date: &str| TradingDay {
        date: day(date),
        sessions: Vec::new(),
    };
    MarketCalendar {
        market: Market::Us,
        today: empty(previous),
        previous_business_day: empty(previous),
        next_business_day: empty(next),
    }
}

fn one_second() -> chrono::Duration {
    chrono::Duration::try_seconds(1).unwrap()
}

/// `previous`의 미 동부 자정(UTC로 `midnight_utc`)이 덮는 범위의 시작이다.
fn assert_starts_at(previous: &str, midnight_utc: &str) {
    let calendar = us_calendar(previous, previous);
    let start = ts(midnight_utc);
    assert!(
        !calendar.covers(start - one_second()),
        "{previous}: {midnight_utc} 1초 전은 덮지 않는다"
    );
    assert!(
        calendar.covers(start),
        "{previous}: {midnight_utc}부터 덮는다"
    );
}

/// `next` 다음 날의 미 동부 자정(UTC로 `midnight_utc`)이 덮는 범위의 끝(제외)이다.
fn assert_ends_at(next: &str, midnight_utc: &str) {
    let calendar = us_calendar(next, next);
    let end = ts(midnight_utc);
    assert!(
        calendar.covers(end - one_second()),
        "{next}: {midnight_utc} 1초 전까지 덮는다"
    );
    assert!(
        !calendar.covers(end),
        "{next}: {midnight_utc}부터는 덮지 않는다"
    );
}

// 검증: N6
#[test]
fn 서머타임이_시작하는_날의_자정은_아직_표준시다() {
    // 2026년: 3월 둘째 일요일은 3월 8일. 그날 02:00(EST)에 바뀐다.
    assert_starts_at("2026-03-07", "2026-03-07T05:00:00Z");
    assert_starts_at("2026-03-08", "2026-03-08T05:00:00Z");
    assert_starts_at("2026-03-09", "2026-03-09T04:00:00Z");
    assert_ends_at("2026-03-07", "2026-03-08T05:00:00Z");
    assert_ends_at("2026-03-08", "2026-03-09T04:00:00Z");
    // 2027년: 3월 둘째 일요일은 3월 14일(날짜를 외워 두지 않고 센다).
    assert_starts_at("2027-03-13", "2027-03-13T05:00:00Z");
    assert_starts_at("2027-03-14", "2027-03-14T05:00:00Z");
    assert_starts_at("2027-03-15", "2027-03-15T04:00:00Z");
    // 3월 1일이 일요일인 해(2026)에도 첫째 일요일(3월 1일)은 아니다.
    assert_starts_at("2026-03-02", "2026-03-02T05:00:00Z");
}

// 검증: N6
#[test]
fn 서머타임이_끝나는_날의_자정은_아직_서머타임이다() {
    // 2026년: 11월 첫째 일요일은 11월 1일. 그날 02:00(EDT)에 바뀐다.
    assert_starts_at("2026-10-31", "2026-10-31T04:00:00Z");
    assert_starts_at("2026-11-01", "2026-11-01T04:00:00Z");
    assert_starts_at("2026-11-02", "2026-11-02T05:00:00Z");
    assert_ends_at("2026-10-31", "2026-11-01T04:00:00Z");
    assert_ends_at("2026-11-01", "2026-11-02T05:00:00Z");
    // 2027년: 11월 첫째 일요일은 11월 7일.
    assert_starts_at("2027-11-06", "2027-11-06T04:00:00Z");
    assert_starts_at("2027-11-07", "2027-11-07T04:00:00Z");
    assert_starts_at("2027-11-08", "2027-11-08T05:00:00Z");
}

// 검증: N6
#[test]
fn 한여름과_한겨울의_미_동부_자정() {
    assert_starts_at("2026-07-01", "2026-07-01T04:00:00Z");
    assert_starts_at("2026-01-15", "2026-01-15T05:00:00Z");
    assert_starts_at("2026-12-31", "2026-12-31T05:00:00Z");
    assert_ends_at("2026-12-31", "2027-01-01T05:00:00Z");
}

// 검증: N6
#[test]
fn 서머타임_경계_앞뒤에서_달력이_덮지_않으면_늦은_시세다() {
    // 2026-03-09(월)은 서머타임이다: 미 동부 자정 = 04:00Z.
    let calendar = us_calendar("2026-03-09", "2026-03-09");
    let policy = StalePolicy::default();
    let quote_at = |at: DateTime<Utc>| Quote {
        symbol: Symbol::new("AAPL"),
        price: dec("185.70"),
        currency: Currency::Usd,
        exchange_time: None,
        received_at: at,
    };

    // 04:00Z 1초 전: 달력 밖 → 늦은 것.
    let before = ts("2026-03-09T03:59:59Z");
    assert!(is_stale(&quote_at(before), &calendar, before, &policy));
    // 04:00Z: 달력 안이고 세션이 없다(장 닫힘) → 늦지 않다.
    let start = ts("2026-03-09T04:00:00Z");
    assert!(!is_stale(&quote_at(start), &calendar, start, &policy));
    // 끝: 다음 날 자정(서머타임) 04:00Z부터는 달력 밖.
    let last = ts("2026-03-10T03:59:59Z");
    assert!(!is_stale(&quote_at(last), &calendar, last, &policy));
    let end = ts("2026-03-10T04:00:00Z");
    assert!(is_stale(&quote_at(end), &calendar, end, &policy));
}
