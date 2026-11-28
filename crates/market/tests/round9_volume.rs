//! 9차 작업(V32 거래량). 그날 누적 거래량을 일봉(`GET /api/v1/candles?interval=1d`)의 맨 앞 봉에서 읽는다.
//!
//! 공식 명세(REST OpenAPI v1.2.31, 2026-11-28 확인): 캔들은 최신순이고 `volume`은 십진수 글자다. 일봉의
//! `timestamp`는 그 거래일의 현지 자정이다. 한도 그룹은 `MARKET_DATA_CHART`(초당 20회).
//! 로컬 가짜 서버(wiremock)만 쓴다. 시간은 테스트가 정한다.
//! (9차) 2026-11-28 V32 거래량을 합쳐 잠금.

use std::str::FromStr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use pi_market::{
    Clock, CredentialSource, Credentials, FakeAdapter, Market, MarketAdapter, MarketError, Symbol,
    TossAdapter, TossClient, TossConfig,
};
use rust_decimal::Decimal;
use serde_json::{Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

const CLIENT_ID: &str = "시험용-클라이언트-아이디-0009";
const CLIENT_SECRET: &str = "시험용-클라이언트-비밀-0009";
const TOKEN_PATH: &str = "/oauth2/token";
const CANDLES_PATH: &str = "/api/v1/candles";
const START: &str = "2026-03-25T10:00:00+09:00";

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

fn date(text: &str) -> NaiveDate {
    NaiveDate::from_str(text).unwrap()
}

fn dec(text: &str) -> Decimal {
    Decimal::from_str(text).unwrap()
}

fn candle(stamp: &str, volume: Value) -> Value {
    json!({
        "timestamp": stamp,
        "openPrice": "71600",
        "highPrice": "72300",
        "lowPrice": "71500",
        "closePrice": "72000",
        "volume": volume,
        "currency": "KRW",
    })
}

/// 토큰은 언제나 내주고, 캔들은 `candles`로 답하는 가짜 서버의 어댑터.
async fn adapter_with(server: &MockServer, market: Market, candles: Vec<Value>) -> TossAdapter {
    Mock::given(method("POST"))
        .and(path(TOKEN_PATH))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "access_token": "test-access-token-9",
            "token_type": "Bearer",
            "expires_in": 86400,
        })))
        .mount(server)
        .await;
    Mock::given(method("GET"))
        .and(path(CANDLES_PATH))
        .respond_with(move |_: &Request| {
            ResponseTemplate::new(200).set_body_json(json!({
                "result": { "candles": candles.clone(), "nextBefore": null }
            }))
        })
        .mount(server)
        .await;
    let config = TossConfig {
        base_url: server.uri(),
        timeout: Duration::from_secs(5),
        ..TossConfig::default()
    };
    let clock = Arc::new(TestClock {
        now: Mutex::new(START.parse::<DateTime<Utc>>().unwrap()),
    });
    TossClient::new(config, Arc::new(TestCredentials), clock).adapter(market)
}

async fn candle_requests(server: &MockServer) -> Vec<Request> {
    server
        .received_requests()
        .await
        .unwrap_or_default()
        .into_iter()
        .filter(|r| r.url.path() == CANDLES_PATH)
        .collect()
}

fn query(req: &Request, key: &str) -> Option<String> {
    req.url
        .query_pairs()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.into_owned())
}

// 검증: V32
#[tokio::test]
async fn 오늘_일봉의_거래량을_그날_누적_거래량으로_읽는다() {
    let server = MockServer::start().await;
    let adapter = adapter_with(
        &server,
        Market::Kr,
        vec![
            candle("2026-03-25T00:00:00+09:00", json!("3521000")),
            candle("2026-03-24T00:00:00+09:00", json!("2984000")),
        ],
    )
    .await;
    let volume = adapter
        .daily_volume(&Symbol::new("005930"), date("2026-03-25"))
        .await
        .unwrap();
    assert_eq!(volume, Some(dec("3521000")));

    let asked = candle_requests(&server).await;
    assert_eq!(asked.len(), 1, "종목 하나에 요청 하나");
    assert_eq!(query(&asked[0], "symbol").as_deref(), Some("005930"));
    assert_eq!(query(&asked[0], "interval").as_deref(), Some("1d"));
    assert_eq!(
        query(&asked[0], "count").as_deref(),
        Some("1"),
        "맨 앞(가장 최근) 봉 하나만 묻는다"
    );
}

// 검증: V32
#[tokio::test]
async fn 마지막_일봉이_어제면_그날_거래량은_0부터다() {
    let server = MockServer::start().await;
    let adapter = adapter_with(
        &server,
        Market::Kr,
        vec![candle("2026-03-24T00:00:00+09:00", json!("2984000"))],
    )
    .await;
    let volume = adapter
        .daily_volume(&Symbol::new("005930"), date("2026-03-25"))
        .await
        .unwrap();
    assert_eq!(
        volume,
        Some(Decimal::ZERO),
        "거래일이 바뀌면 어제 거래량을 이어 쓰지 않는다"
    );
}

// 검증: V32
#[tokio::test]
async fn 일봉이_없거나_물은_날보다_뒤면_모른다() {
    let server = MockServer::start().await;
    let adapter = adapter_with(&server, Market::Kr, Vec::new()).await;
    let none = adapter
        .daily_volume(&Symbol::new("005930"), date("2026-03-25"))
        .await
        .unwrap();
    assert_eq!(
        none, None,
        "일봉이 하나도 없으면 모른다(0이라고 하지 않는다)"
    );

    let later = MockServer::start().await;
    let adapter = adapter_with(
        &later,
        Market::Kr,
        vec![candle("2026-03-26T00:00:00+09:00", json!("100"))],
    )
    .await;
    let skewed = adapter
        .daily_volume(&Symbol::new("005930"), date("2026-03-25"))
        .await
        .unwrap();
    assert_eq!(skewed, None, "물은 날보다 뒤 봉은 그날 것이 아니다");
}

// 검증: V32
#[tokio::test]
async fn 미국_일봉은_그_봉의_현지_날짜로_본다() {
    let server = MockServer::start().await;
    let adapter = adapter_with(
        &server,
        Market::Us,
        vec![candle("2026-03-25T00:00:00-04:00", json!("1234567.5"))],
    )
    .await;
    let volume = adapter
        .daily_volume(&Symbol::new("AAPL"), date("2026-03-25"))
        .await
        .unwrap();
    assert_eq!(volume, Some(dec("1234567.5")), "소수 수량도 그대로 읽는다");
}

// 검증: V32
#[tokio::test]
async fn 거래량이_음수이거나_숫자_타입이면_응답_모양_오류다() {
    for bad in [json!("-1"), json!(3521000), json!("많음")] {
        let server = MockServer::start().await;
        let adapter = adapter_with(
            &server,
            Market::Kr,
            vec![candle("2026-03-25T00:00:00+09:00", bad.clone())],
        )
        .await;
        let result = adapter
            .daily_volume(&Symbol::new("005930"), date("2026-03-25"))
            .await;
        assert!(
            matches!(result, Err(MarketError::UnexpectedResponse { .. })),
            "{bad}: 읽을 수 없는 거래량은 고르지 않는다: {result:?}"
        );
    }
}

// 검증: V32
#[tokio::test]
async fn 틀린_종목이면_요청을_보내지_않는다() {
    let server = MockServer::start().await;
    let adapter = adapter_with(&server, Market::Kr, Vec::new()).await;
    let result = adapter
        .daily_volume(&Symbol::new("00 5930"), date("2026-03-25"))
        .await;
    assert!(matches!(result, Err(MarketError::InvalidSymbol)));
    assert!(
        server
            .received_requests()
            .await
            .unwrap_or_default()
            .is_empty(),
        "토큰 발급도 하지 않는다"
    );
}

// 검증: V32
#[tokio::test]
async fn 가짜_어댑터는_정한_날의_거래량만_준다() {
    let fake = FakeAdapter::new(Market::Kr);
    let symbol = Symbol::new("005930");
    assert_eq!(
        fake.daily_volume(&symbol, date("2026-03-25"))
            .await
            .unwrap(),
        None,
        "정하지 않았으면 모른다"
    );
    fake.set_volume(symbol.clone(), date("2026-03-25"), dec("1500"));
    assert_eq!(
        fake.daily_volume(&symbol, date("2026-03-25"))
            .await
            .unwrap(),
        Some(dec("1500"))
    );
    assert_eq!(
        fake.daily_volume(&symbol, date("2026-03-26"))
            .await
            .unwrap(),
        Some(Decimal::ZERO),
        "다음 날은 0부터"
    );
    assert_eq!(
        fake.daily_volume(&symbol, date("2026-03-24"))
            .await
            .unwrap(),
        None
    );
}
