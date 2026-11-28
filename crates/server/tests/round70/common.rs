//! 70차 발판: 가짜 토스증권(토큰 · 현재가 · 국내 달력 · 일봉) · 가짜 Anthropic · 서버 띄우기 · 실시간 연결 읽기.
//! `tests/server.rs`의 도우미를 본떴다(바깥 주소는 모두 로컬 가짜 서버다).

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, TimeZone, Utc};
use futures::StreamExt;
use pi_ai::AnthropicConfig;
use pi_market::TossConfig;
use pi_secrets::{Credentials, KeyKind, SecretStore};
use price_inha::{App, KIND_QUOTES, KeyVerifier, Settings, Upstreams, Wiring};
use serde_json::{Value, json};
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

/// 한 걸음을 기다리는 가장 긴 시간.
pub const STEP: Duration = Duration::from_secs(10);
const INTERVAL_MS: &str = "20";
const CLIENT_ID: &str = "test-client-id-0070";
const CLIENT_SECRET: &str = "test-client-secret-not-real-70";
const AI_KEY: &str = "test-ai-key-not-real-0070";
const TOSS_TOKEN: &str = "test-toss-token-not-real-70";
const TOKEN_PATH: &str = "/oauth2/token";
const PRICES_PATH: &str = "/api/v1/prices";
const CALENDAR_KR_PATH: &str = "/api/v1/market-calendar/KR";
/// 캔들 경로(공식 명세 REST v1.2.31).
pub const CANDLES_PATH: &str = "/api/v1/candles";
const MODELS_PATH: &str = "/v1/models";

pub type Ws = WebSocketStream<MaybeTlsStream<TcpStream>>;

/// 테스트가 시각을 정하는 시계.
pub struct TestClock(AtomicI64);

impl TestClock {
    pub fn at(time: DateTime<Utc>) -> Arc<Self> {
        Arc::new(Self(AtomicI64::new(time.timestamp_millis())))
    }

    pub fn set(&self, time: DateTime<Utc>) {
        self.0.store(time.timestamp_millis(), Ordering::SeqCst);
    }
}

#[async_trait]
impl pi_market::Clock for TestClock {
    fn now(&self) -> DateTime<Utc> {
        Utc.timestamp_millis_opt(self.0.load(Ordering::SeqCst))
            .unwrap()
    }

    async fn sleep(&self, _duration: Duration) {
        tokio::task::yield_now().await;
    }
}

/// 한국 시각.
pub fn kst(day: u32, hour: u32, minute: u32) -> DateTime<Utc> {
    let offset = chrono::FixedOffset::east_opt(9 * 3600).unwrap();
    offset
        .with_ymd_and_hms(2026, 3, day, hour, minute, 0)
        .unwrap()
        .with_timezone(&Utc)
}

fn kr_open_day(date: &str) -> Value {
    json!({
        "date": date,
        "integrated": {
            "preMarket": null,
            "regularMarket": {
                "startTime": format!("{date}T08:00:00+09:00"),
                "endTime": format!("{date}T20:00:00+09:00"),
            },
            "afterMarket": null,
        }
    })
}

/// 일봉 하나를 정한다: `(봉 시각, 거래량 글자)`. `None`이면 캔들 경로가 404다(거래량을 모른다).
pub type DailyCandle = Option<(String, String)>;

/// 가짜 토스증권.
pub struct FakeToss {
    pub server: MockServer,
}

impl FakeToss {
    pub async fn start(candle: DailyCandle) -> Self {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path(TOKEN_PATH))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "access_token": TOSS_TOKEN, "token_type": "Bearer", "expires_in": 86_400
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path(PRICES_PATH))
            .respond_with(|req: &Request| {
                let items: Vec<Value> = asked_symbols(req)
                    .iter()
                    .map(|s| {
                        json!({
                            "symbol": s,
                            "timestamp": "2026-03-25T10:00:00.000+09:00",
                            "lastPrice": "72000",
                            "currency": "KRW",
                        })
                    })
                    .collect();
                ResponseTemplate::new(200).set_body_json(json!({ "result": items }))
            })
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path(CALENDAR_KR_PATH))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "result": {
                "today": kr_open_day("2026-03-25"),
                "previousBusinessDay": kr_open_day("2026-03-24"),
                "nextBusinessDay": kr_open_day("2026-03-26"),
            }})))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path(CANDLES_PATH))
            .respond_with(move |_: &Request| match candle.clone() {
                None => ResponseTemplate::new(404).set_body_json(json!({ "error": {
                    "requestId": "r-70", "code": "not-found", "message": "없음"
                }})),
                Some((stamp, volume)) => ResponseTemplate::new(200).set_body_json(json!({
                    "result": { "candles": [{
                        "timestamp": stamp,
                        "openPrice": "71600", "highPrice": "72300", "lowPrice": "71500",
                        "closePrice": "72000", "volume": volume, "currency": "KRW",
                    }], "nextBefore": null }
                })),
            })
            .mount(&server)
            .await;
        Self { server }
    }

    pub async fn requests(&self, wanted: &str) -> Vec<Request> {
        let all = self.server.received_requests().await.unwrap_or_default();
        all.into_iter().filter(|r| r.url.path() == wanted).collect()
    }
}

fn asked_symbols(request: &Request) -> Vec<String> {
    request
        .url
        .query_pairs()
        .find(|(key, _)| key == "symbols")
        .map(|(_, value)| value.split(',').map(str::to_string).collect())
        .unwrap_or_default()
}

/// 요청의 쿼리 값.
pub fn query(request: &Request, key: &str) -> Option<String> {
    request
        .url
        .query_pairs()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.into_owned())
}

async fn fake_ai() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(MODELS_PATH))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": [], "has_more": false, "first_id": null, "last_id": null
        })))
        .mount(&server)
        .await;
    server
}

fn dead_base_url() -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    format!("http://127.0.0.1:{port}")
}

fn upstreams(toss: Option<&str>, anthropic: Option<&str>) -> Upstreams {
    let dead = dead_base_url();
    Upstreams {
        toss: TossConfig {
            base_url: toss.unwrap_or(&dead).to_string(),
            timeout: Duration::from_secs(3),
            token_failure_cooldown: Duration::ZERO,
            ..TossConfig::default()
        },
        anthropic: AnthropicConfig {
            base_url: anthropic.unwrap_or(&dead).to_string(),
            timeout: Duration::from_secs(3),
            max_retries: 0,
            retry_base_delay: Duration::from_millis(1),
        },
    }
}

/// 증권사 키와 AI 키를 가짜 서버로 확인해 보관 폴더에 등록한다(둘 다 있어야 시세를 받기 시작한다).
pub async fn register_keys(home: &Path, toss: &FakeToss) {
    let secrets = SecretStore::open(home).unwrap();
    let verifier = KeyVerifier::new(upstreams(Some(&toss.server.uri()), None));
    secrets
        .register(
            KeyKind::Broker,
            Credentials::new()
                .with("client_id", CLIENT_ID)
                .with("client_secret", CLIENT_SECRET),
            &verifier,
        )
        .await
        .expect("가짜 서버가 토큰을 주면 등록된다");
    let ai = fake_ai().await;
    let verifier = KeyVerifier::new(upstreams(None, Some(&ai.uri())));
    secrets
        .register(
            KeyKind::AiApiKey,
            Credentials::new().with("api_key", AI_KEY),
            &verifier,
        )
        .await
        .expect("가짜 서버가 키를 받으면 등록된다");
}

fn no_env(_name: &str) -> Option<String> {
    None
}

/// 임시 보관 폴더 · 빈 포트 · 짧은 시세 간격 · 국내 종목 둘.
pub fn settings(home: &Path, extra: &[&str]) -> Settings {
    let home = home.to_str().unwrap();
    let mut list = vec![
        "--home",
        home,
        "--port",
        "0",
        "--interval-ms",
        INTERVAL_MS,
        "--symbols",
        "005930,000660",
    ];
    list.extend_from_slice(extra);
    let list: Vec<String> = list.iter().map(|s| (*s).to_string()).collect();
    Settings::parse(&list, &no_env, None).expect("설정을 읽어야 한다")
}

pub fn wiring(toss: Option<&FakeToss>, clock: Arc<TestClock>) -> Wiring {
    let uri = toss.map(|t| t.server.uri());
    Wiring {
        upstreams: upstreams(uri.as_deref(), None),
        clock,
        peer_override: None,
        cli_locations: Vec::new(),
    }
}

pub async fn start_app(settings: Settings, wiring: Wiring) -> App {
    tokio::time::timeout(STEP, App::start(settings, wiring))
        .await
        .expect("서버가 시간 안에 떠야 한다")
        .expect("서버가 떠야 한다")
}

pub async fn stop(app: App) {
    tokio::time::timeout(STEP, app.shutdown())
        .await
        .expect("시간 안에 꺼져야 한다")
        .expect("끄기가 성공해야 한다");
}

fn origin_of(port: u16) -> String {
    format!("http://127.0.0.1:{port}")
}

/// 세션을 받고 실시간 연결을 연다.
pub async fn connect(app: &App) -> Ws {
    let port = app.local_addr().port();
    let origin = origin_of(port);
    let reply = reqwest::Client::new()
        .post(format!("{origin}{}", pi_gate::SESSION_PATH))
        .header("Origin", &origin)
        .send()
        .await
        .expect("세션 요청이 닿아야 한다");
    let raw = reply
        .headers()
        .get("set-cookie")
        .expect("세션 쿠키가 있어야 한다")
        .to_str()
        .unwrap()
        .to_string();
    let cookie = raw.split(';').next().unwrap().trim().to_string();
    let url = format!(
        "ws://127.0.0.1:{port}{}?{}=0",
        pi_gate::WS_PATH,
        pi_gate::LAST_SEQ_PARAM
    );
    let mut request = url.into_client_request().unwrap();
    let headers = request.headers_mut();
    headers.insert("Cookie", cookie.parse().unwrap());
    headers.insert("Origin", origin.parse().unwrap());
    let (ws, _) = tokio::time::timeout(STEP, tokio_tungstenite::connect_async(request))
        .await
        .expect("웹소켓 응답이 시간 안에 와야 한다")
        .expect("세션이 있으면 웹소켓이 열려야 한다");
    ws
}

async fn next_text(ws: &mut Ws, deadline: tokio::time::Instant) -> Result<Option<String>, ()> {
    loop {
        match tokio::time::timeout_at(deadline, ws.next()).await {
            Err(_) => return Err(()),
            Ok(None | Some(Err(_)) | Some(Ok(Message::Close(_)))) => return Ok(None),
            Ok(Some(Ok(Message::Text(text)))) => return Ok(Some(text.as_str().to_string())),
            Ok(Some(Ok(_))) => {}
        }
    }
}

/// `wanted`가 값을 돌려주는 시세 묶음(`quotes` 지나가는 이벤트)이 올 때까지 읽는다.
pub async fn find_quotes<T>(
    ws: &mut Ws,
    wait: Duration,
    mut wanted: impl FnMut(&Value) -> Option<T>,
) -> Option<T> {
    let deadline = tokio::time::Instant::now() + wait;
    while let Ok(Some(text)) = next_text(ws, deadline).await {
        let frame: Value = serde_json::from_str(&text).expect("프레임은 JSON이어야 한다");
        if frame["kind"] == json!(KIND_QUOTES)
            && frame.get("seq").is_none()
            && let Some(found) = wanted(&frame["payload"])
        {
            return Some(found);
        }
    }
    None
}

/// 시세 묶음에서 그 종목의 한 칸.
pub fn quote_of<'a>(quotes: &'a Value, symbol: &str) -> Option<&'a Value> {
    quotes["quotes"]
        .as_array()?
        .iter()
        .find(|q| q["symbol"] == json!(symbol))
}

/// 그 종목의 거래량 글자(칸이 없으면 `None`).
pub fn volume_of(quotes: &Value, symbol: &str) -> Option<String> {
    quote_of(quotes, symbol)?
        .get("volume")?
        .as_str()
        .map(str::to_string)
}
