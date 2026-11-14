//! price-inha 서버 통합 테스트. 임시 보관 폴더로 서버를 `127.0.0.1:0`에 띄우고,
//! 증권사 · AI는 로컬 가짜 서버(wiremock)로 대신한다.
//!
//! 근거: `docs/dev/modules/gate.md`의 "price-inha(실행 파일)가 만드는 것" 1~6.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, TimeZone, Utc};
use futures::StreamExt;
use pi_ai::{AnthropicConfig, Credential, Secret};
use pi_market::{Market, Symbol, TossConfig};
use pi_secrets::{Credentials, KeyKind, SecretStore, SecretsError, VerifyFailure};
use price_inha::{
    AiKeys, App, BrokerKeys, DEFAULT_QUOTE_INTERVAL, ENV_BIND, ENV_HOME, ENV_MARKET, ENV_PORT,
    ENV_QUOTE_INTERVAL_MS, ENV_SYMBOLS, FEED_DEMO, FEED_LIVE, FEED_NO_BROKER_KEY, KIND_FEED_STATUS,
    KIND_QUOTES, KIND_QUOTES_STALE, KeyVerifier, Settings, SettingsError, Upstreams, Wiring,
};
use serde_json::{Value, json};
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

// ───────────── 도우미 ─────────────

/// 한 걸음을 기다리는 가장 긴 시간.
const STEP: Duration = Duration::from_secs(10);
/// "오지 않는다"를 확인할 때 기다리는 시간.
const QUIET: Duration = Duration::from_millis(400);
/// 테스트에서 쓰는 시세 간격.
const INTERVAL_MS: &str = "20";
/// 시험용 자격. 진짜 키처럼 보이지 않는 글자다.
const CLIENT_ID: &str = "test-client-id-0001";
const CLIENT_SECRET: &str = "test-client-secret-not-real";
const AI_KEY: &str = "test-ai-key-not-real-0001";
const OAUTH_ACCESS: &str = "test-oauth-access-not-real";
const OAUTH_REFRESH: &str = "test-oauth-refresh-not-real";
const TOSS_TOKEN: &str = "test-toss-token-not-real";
/// 토스증권 경로(공식 명세, `docs/dev/toss-api-notes.md`).
const TOKEN_PATH: &str = "/oauth2/token";
const PRICES_PATH: &str = "/api/v1/prices";
const CALENDAR_KR_PATH: &str = "/api/v1/market-calendar/KR";
/// Anthropic 경로.
const MODELS_PATH: &str = "/v1/models";

type Ws = WebSocketStream<MaybeTlsStream<TcpStream>>;

/// 테스트가 시각을 정하는 시계.
struct TestClock(AtomicI64);

impl TestClock {
    fn at(time: DateTime<Utc>) -> Arc<Self> {
        Arc::new(Self(AtomicI64::new(time.timestamp_millis())))
    }

    fn set(&self, time: DateTime<Utc>) {
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

/// 2026-03-25(수) 한국 시각. 가짜 달력에서 08:00~20:00이 열려 있다.
fn kst(hour: u32, minute: u32, second: u32) -> DateTime<Utc> {
    let offset = chrono::FixedOffset::east_opt(9 * 3600).unwrap();
    offset
        .with_ymd_and_hms(2026, 3, 25, hour, minute, second)
        .unwrap()
        .with_timezone(&Utc)
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

/// 가짜 토스증권. 테스트가 가격과 장애를 바꾼다.
struct FakeToss {
    server: MockServer,
    price: Arc<Mutex<String>>,
    failing: Arc<AtomicBool>,
}

impl FakeToss {
    /// 토큰 · 현재가 · 국내 달력을 모두 돌려주는 가짜 서버.
    async fn start() -> Self {
        let server = MockServer::start().await;
        let price = Arc::new(Mutex::new("72000".to_string()));
        let failing = Arc::new(AtomicBool::new(false));
        Mock::given(method("POST"))
            .and(path(TOKEN_PATH))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "access_token": TOSS_TOKEN, "token_type": "Bearer", "expires_in": 86_400
            })))
            .mount(&server)
            .await;
        let (p, f) = (Arc::clone(&price), Arc::clone(&failing));
        Mock::given(method("GET"))
            .and(path(PRICES_PATH))
            .respond_with(move |req: &Request| {
                if f.load(Ordering::SeqCst) {
                    return ResponseTemplate::new(503).set_body_json(json!({
                        "error": { "requestId": "r-1", "code": "maintenance", "message": "점검" }
                    }));
                }
                let wanted = req
                    .url
                    .query_pairs()
                    .find(|(k, _)| k == "symbols")
                    .map(|(_, v)| v.into_owned())
                    .unwrap_or_default();
                let now = p.lock().unwrap().clone();
                let items: Vec<Value> = wanted
                    .split(',')
                    .filter(|s| !s.is_empty())
                    .map(|s| {
                        json!({
                            "symbol": s,
                            "timestamp": "2026-03-25T10:00:00.000+09:00",
                            "lastPrice": now,
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
        Self {
            server,
            price,
            failing,
        }
    }

    fn set_price(&self, price: &str) {
        *self.price.lock().unwrap() = price.to_string();
    }

    async fn requests(&self, wanted: &str) -> Vec<Request> {
        let all = self.server.received_requests().await.unwrap();
        all.into_iter().filter(|r| r.url.path() == wanted).collect()
    }

    async fn total_requests(&self) -> usize {
        self.server.received_requests().await.unwrap().len()
    }
}

/// 아무도 듣지 않는 로컬 주소(연결 실패를 만들 때 쓴다).
fn dead_base_url() -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    format!("http://127.0.0.1:{port}")
}

fn toss_config(base_url: &str) -> TossConfig {
    TossConfig {
        base_url: base_url.to_string(),
        timeout: Duration::from_secs(3),
        ..TossConfig::default()
    }
}

fn anthropic_config(base_url: &str) -> AnthropicConfig {
    AnthropicConfig {
        base_url: base_url.to_string(),
        timeout: Duration::from_secs(3),
        max_retries: 0,
        retry_base_delay: Duration::from_millis(1),
    }
}

/// 바깥 서버 설정. 주지 않은 쪽은 아무도 듣지 않는 로컬 주소다(실제 주소로 나가지 않게).
fn upstreams(toss: Option<&str>, anthropic: Option<&str>) -> Upstreams {
    let dead = dead_base_url();
    Upstreams {
        toss: toss_config(toss.unwrap_or(&dead)),
        anthropic: anthropic_config(anthropic.unwrap_or(&dead)),
    }
}

fn broker_values() -> Credentials {
    Credentials::new()
        .with("client_id", CLIENT_ID)
        .with("client_secret", CLIENT_SECRET)
}

/// 가짜 토스증권으로 확인해 증권사 키를 보관 폴더에 등록한다.
async fn register_broker(home: &Path, toss: &FakeToss) {
    let secrets = SecretStore::open(home).unwrap();
    let verifier = KeyVerifier::new(upstreams(Some(&toss.server.uri()), None));
    secrets
        .register(KeyKind::Broker, broker_values(), &verifier)
        .await
        .expect("가짜 서버가 토큰을 주면 등록된다");
}

fn no_env(_name: &str) -> Option<String> {
    None
}

fn args(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| (*s).to_string()).collect()
}

/// 임시 보관 폴더 · 빈 포트 · 짧은 시세 간격의 설정.
fn settings(home: &Path, extra: &[&str]) -> Settings {
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
    Settings::parse(&args(&list), &no_env, None).expect("설정을 읽어야 한다")
}

async fn start_app(settings: Settings, wiring: Wiring) -> App {
    tokio::time::timeout(STEP, App::start(settings, wiring))
        .await
        .expect("서버가 시간 안에 떠야 한다")
        .expect("서버가 떠야 한다")
}

fn wiring(toss: Option<&FakeToss>, clock: Arc<TestClock>) -> Wiring {
    let uri = toss.map(|t| t.server.uri());
    Wiring {
        upstreams: upstreams(uri.as_deref(), None),
        clock,
    }
}

fn origin_of(addr: SocketAddr) -> String {
    format!("http://127.0.0.1:{}", addr.port())
}

/// 같은 출처에서 세션을 받아 `Cookie` 헤더 글자를 돌려준다.
async fn login(addr: SocketAddr) -> String {
    let origin = origin_of(addr);
    let reply = reqwest::Client::new()
        .post(format!("{origin}{}", pi_gate::SESSION_PATH))
        .header("Origin", &origin)
        .send()
        .await
        .expect("세션 요청이 닿아야 한다");
    assert!(
        reply.status().is_success(),
        "세션을 받아야 한다: {}",
        reply.status()
    );
    let raw = reply
        .headers()
        .get("set-cookie")
        .expect("세션 쿠키가 있어야 한다")
        .to_str()
        .unwrap();
    raw.split(';').next().unwrap().trim().to_string()
}

/// 세션을 받고 실시간 연결을 연다.
async fn connect(addr: SocketAddr, last_seq: u64) -> Ws {
    let cookie = login(addr).await;
    let url = format!(
        "ws://127.0.0.1:{}{}?{}={last_seq}",
        addr.port(),
        pi_gate::WS_PATH,
        pi_gate::LAST_SEQ_PARAM
    );
    let mut request = url.into_client_request().unwrap();
    let headers = request.headers_mut();
    headers.insert("Cookie", cookie.parse().unwrap());
    headers.insert("Origin", origin_of(addr).parse().unwrap());
    let (ws, _) = tokio::time::timeout(STEP, tokio_tungstenite::connect_async(request))
        .await
        .expect("웹소켓 응답이 시간 안에 와야 한다")
        .expect("세션이 있으면 웹소켓이 열려야 한다");
    ws
}

/// 다음 글자 프레임. 닫혔으면 `None`, 시간 안에 아무 일도 없으면 `Err`.
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

/// `wanted`가 값을 돌려주는 프레임이 올 때까지 읽는다. 시간 안에 없으면 `None`.
async fn find_frame<T>(
    ws: &mut Ws,
    wait: Duration,
    mut wanted: impl FnMut(&Value) -> Option<T>,
) -> Option<T> {
    let deadline = tokio::time::Instant::now() + wait;
    while let Ok(Some(text)) = next_text(ws, deadline).await {
        let frame: Value = serde_json::from_str(&text).expect("프레임은 JSON이어야 한다");
        if let Some(found) = wanted(&frame) {
            return Some(found);
        }
    }
    None
}

/// 그 종류의 지나가는 이벤트(번호 없음)의 내용.
fn transient(frame: &Value, kind: &str) -> Option<Value> {
    (frame["kind"] == json!(kind) && frame.get("seq").is_none()).then(|| frame["payload"].clone())
}

/// 시세 묶음에서 그 종목의 가격 글자.
fn price_of(quotes: &Value, symbol: &str) -> Option<String> {
    quotes["quotes"]
        .as_array()?
        .iter()
        .find(|q| q["symbol"] == json!(symbol))
        .and_then(|q| q["price"].as_str())
        .map(str::to_string)
}

/// 그 종목의 가격이 `price`인 시세 묶음이 올 때까지 기다린다.
async fn wait_price(ws: &mut Ws, symbol: &str, price: &str) -> Value {
    find_frame(ws, STEP, |frame| {
        transient(frame, KIND_QUOTES)
            .filter(|quotes| price_of(quotes, symbol).as_deref() == Some(price))
    })
    .await
    .unwrap_or_else(|| panic!("{symbol}의 시세 {price}가 실시간 연결로 와야 한다"))
}

async fn wait_feed_status(ws: &mut Ws) -> Value {
    find_frame(ws, STEP, |frame| transient(frame, KIND_FEED_STATUS))
        .await
        .expect("시세 상태를 알려야 한다")
}

async fn stop(app: App) {
    tokio::time::timeout(STEP, app.shutdown())
        .await
        .expect("시간 안에 꺼져야 한다")
        .expect("끄기가 성공해야 한다");
}

// ───────────── 1. 설정 ─────────────

// 검증: D2
#[test]
fn 아무것도_주지_않으면_그_컴퓨터_주소로_연다() {
    let user_home = Path::new("/home/tester");
    let got = Settings::parse(&[], &no_env, Some(user_home)).unwrap();
    assert_eq!(
        got.bind,
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        "기본은 127.0.0.1"
    );
    assert_eq!(got.port, pi_gate::DEFAULT_PORT);
    assert_eq!(
        got.home,
        pi_secrets::resolve_dir(None, Some(user_home)).unwrap(),
        "보관 폴더의 기본값은 pi-secrets와 같다"
    );
    assert!(got.symbols.is_empty());
    assert_eq!(got.market, Market::Kr);
    assert_eq!(got.quote_interval, DEFAULT_QUOTE_INTERVAL);
    assert!(!got.demo);
}

// 검증: D2
#[test]
fn 인자로_설정을_준다() {
    let got = Settings::parse(
        &args(&[
            "--home",
            "/tmp/pi-home",
            "--bind",
            "192.168.0.10",
            "--port",
            "9100",
            "--symbols",
            "AAPL, MSFT,,NVDA",
            "--market",
            "us",
            "--interval-ms",
            "2500",
            "--demo",
        ]),
        &no_env,
        None,
    )
    .unwrap();
    assert_eq!(got.home, Path::new("/tmp/pi-home"));
    assert_eq!(
        got.bind,
        "192.168.0.10".parse::<IpAddr>().unwrap(),
        "다른 주소는 분명히 줬을 때만 쓴다"
    );
    assert_eq!(got.port, 9100);
    assert_eq!(
        got.symbols,
        [
            Symbol::new("AAPL"),
            Symbol::new("MSFT"),
            Symbol::new("NVDA")
        ]
    );
    assert_eq!(got.market, Market::Us);
    assert_eq!(got.quote_interval, Duration::from_millis(2500));
    assert!(got.demo);
}

// 검증: D2
#[test]
fn 환경_변수로_설정을_주고_인자가_이긴다() {
    let env = |name: &str| -> Option<String> {
        let value = match name {
            n if n == ENV_HOME => "/srv/pi",
            n if n == ENV_BIND => "10.0.0.5",
            n if n == ENV_PORT => "9200",
            n if n == ENV_SYMBOLS => "005930,035720",
            n if n == ENV_MARKET => "kr",
            n if n == ENV_QUOTE_INTERVAL_MS => "500",
            _ => return None,
        };
        Some(value.to_string())
    };
    let from_env = Settings::parse(&[], &env, None).unwrap();
    assert_eq!(from_env.home, Path::new("/srv/pi"));
    assert_eq!(from_env.bind, "10.0.0.5".parse::<IpAddr>().unwrap());
    assert_eq!(from_env.port, 9200);
    assert_eq!(
        from_env.symbols,
        [Symbol::new("005930"), Symbol::new("035720")]
    );
    assert_eq!(from_env.market, Market::Kr);
    assert_eq!(from_env.quote_interval, Duration::from_millis(500));
    assert!(!from_env.demo);

    let mixed = Settings::parse(
        &args(&["--port", "9300", "--bind", "127.0.0.1"]),
        &env,
        None,
    )
    .unwrap();
    assert_eq!(mixed.port, 9300, "인자가 환경 변수를 이긴다");
    assert_eq!(mixed.bind, IpAddr::V4(Ipv4Addr::LOCALHOST));
    assert_eq!(mixed.home, Path::new("/srv/pi"), "주지 않은 것은 환경 변수");
}

// 검증: D2
#[test]
fn 읽을_수_없는_설정은_오류다() {
    let home = ["--home", "/tmp/pi-home"];
    let parse = |extra: &[&str]| {
        let mut list = home.to_vec();
        list.extend_from_slice(extra);
        Settings::parse(&args(&list), &no_env, None)
    };
    assert_eq!(
        parse(&["--port", "70000"]),
        Err(SettingsError::InvalidValue { name: "--port" })
    );
    assert_eq!(
        parse(&["--port", "abc"]),
        Err(SettingsError::InvalidValue { name: "--port" })
    );
    assert_eq!(
        parse(&["--bind", "localhost"]),
        Err(SettingsError::InvalidValue { name: "--bind" }),
        "주소는 IP로만 받는다"
    );
    assert_eq!(
        parse(&["--market", "jp"]),
        Err(SettingsError::InvalidValue { name: "--market" })
    );
    assert_eq!(
        parse(&["--interval-ms", "0"]),
        Err(SettingsError::InvalidValue {
            name: "--interval-ms"
        })
    );
    assert_eq!(
        parse(&["--port"]),
        Err(SettingsError::MissingValue { name: "--port" })
    );
    assert_eq!(
        parse(&["--verbose"]),
        Err(SettingsError::UnknownArgument),
        "모르는 인자를 조용히 넘기지 않는다"
    );
    assert_eq!(
        Settings::parse(&[], &no_env, None),
        Err(SettingsError::HomeNotFound)
    );
    let bad_env = |name: &str| (name == ENV_PORT).then(|| "많이".to_string());
    assert_eq!(
        Settings::parse(&args(&home), &bad_env, None),
        Err(SettingsError::InvalidValue { name: ENV_PORT })
    );
}

// 검증: V31
#[test]
fn 비밀_값은_인자로_받지_않는다() {
    for name in [
        "--client-secret",
        "--client-id",
        "--api-key",
        "--access-token",
        "--broker-client-secret",
        "--ai-api-key",
    ] {
        for list in [
            vec!["--home", "/tmp/pi-home", name, CLIENT_SECRET],
            vec!["--home", "/tmp/pi-home", name],
        ] {
            let error = Settings::parse(&args(&list), &no_env, None)
                .expect_err("비밀 값을 받는 인자는 없다");
            assert_eq!(error, SettingsError::UnknownArgument, "{name}");
            let shown = format!("{error} {error:?}");
            assert!(!shown.contains(CLIENT_SECRET), "오류에 값이 실리면 안 된다");
        }
    }
    let joined = format!("--client-secret={CLIENT_SECRET}");
    let error = Settings::parse(&args(&["--home", "/tmp/pi-home", &joined]), &no_env, None)
        .expect_err("비밀 값을 받는 인자는 없다");
    assert!(!format!("{error} {error:?}").contains(CLIENT_SECRET));
}

// ───────────── 2. 조립: 자격 자리와 확인기 ─────────────

// 검증: V31
#[tokio::test]
async fn 증권사_키는_토큰이_발급될_때만_등록되고_자격_자리로_나온다() {
    let toss = FakeToss::start().await;
    let dir = tempfile::tempdir().unwrap();
    let secrets = Arc::new(SecretStore::open(dir.path()).unwrap());
    let source = BrokerKeys::new(Arc::clone(&secrets));
    assert!(
        pi_market::CredentialSource::credentials(&source).is_none(),
        "키를 넣기 전에는 자격이 없다"
    );

    let verifier = KeyVerifier::new(upstreams(Some(&toss.server.uri()), None));
    secrets
        .register(KeyKind::Broker, broker_values(), &verifier)
        .await
        .expect("토큰이 발급되면 등록된다");
    let tokens = toss.requests(TOKEN_PATH).await;
    assert_eq!(tokens.len(), 1, "확인은 토큰 발급 한 번이다");
    let body = String::from_utf8_lossy(&tokens[0].body).into_owned();
    assert!(body.contains(CLIENT_ID), "넘겨받은 자격으로 확인한다");

    let found = pi_market::CredentialSource::credentials(&source).expect("넣은 뒤에는 자격이 있다");
    assert_eq!(found.client_id(), CLIENT_ID);
    assert_eq!(found.client_secret(), CLIENT_SECRET);

    // 다른 손잡이로 등록해도(파일이 바뀌어도) 부를 때마다 새로 읽는다.
    let other_dir = tempfile::tempdir().unwrap();
    let late = Arc::new(SecretStore::open(other_dir.path()).unwrap());
    let late_source = BrokerKeys::new(Arc::clone(&late));
    assert!(pi_market::CredentialSource::credentials(&late_source).is_none());
    SecretStore::open(other_dir.path())
        .unwrap()
        .register(KeyKind::Broker, broker_values(), &verifier)
        .await
        .unwrap();
    assert!(pi_market::CredentialSource::credentials(&late_source).is_some());
}

// 검증: V31
#[tokio::test]
async fn 증권사가_거절하거나_닿지_않으면_키를_저장하지_않는다() {
    let rejecting = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(TOKEN_PATH))
        .respond_with(ResponseTemplate::new(401).set_body_json(json!({
            "error": "invalid_client", "error_description": "자격이 맞지 않습니다"
        })))
        .mount(&rejecting)
        .await;
    let broken = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(TOKEN_PATH))
        .respond_with(ResponseTemplate::new(503))
        .mount(&broken)
        .await;
    let dir = tempfile::tempdir().unwrap();
    let secrets = Arc::new(SecretStore::open(dir.path()).unwrap());

    let verifier = KeyVerifier::new(upstreams(Some(&rejecting.uri()), None));
    let result = secrets
        .register(KeyKind::Broker, broker_values(), &verifier)
        .await;
    assert!(
        matches!(
            result,
            Err(SecretsError::VerifyFailed(VerifyFailure::Rejected))
        ),
        "인증 실패는 '키가 맞지 않음'이다: {result:?}"
    );

    let verifier = KeyVerifier::new(upstreams(None, None));
    let result = secrets
        .register(KeyKind::Broker, broker_values(), &verifier)
        .await;
    assert!(
        matches!(
            result,
            Err(SecretsError::VerifyFailed(VerifyFailure::Unreachable))
        ),
        "연결 실패는 '닿지 못함'이다: {result:?}"
    );

    let verifier = KeyVerifier::new(upstreams(Some(&broken.uri()), None));
    let result = secrets
        .register(KeyKind::Broker, broker_values(), &verifier)
        .await;
    assert!(
        matches!(result, Err(SecretsError::VerifyFailed(_))),
        "장애여도 확인하지 못한 키는 저장하지 않는다: {result:?}"
    );

    let source = BrokerKeys::new(Arc::clone(&secrets));
    assert!(pi_market::CredentialSource::credentials(&source).is_none());
    let status = secrets.status().unwrap();
    assert!(
        status.iter().all(|s| !s.registered),
        "아무것도 저장되지 않았다"
    );
}

// 검증: V31
#[tokio::test]
async fn ai_키는_공급자에_닿을_때만_등록되고_자격_자리로_나온다() {
    let ai = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(MODELS_PATH))
        .respond_with(|req: &Request| {
            let key_ok = req
                .headers
                .get("x-api-key")
                .is_some_and(|v| v.as_bytes() == AI_KEY.as_bytes());
            let bearer = format!("Bearer {OAUTH_ACCESS}");
            let token_ok = req
                .headers
                .get("authorization")
                .is_some_and(|v| v.as_bytes() == bearer.as_bytes());
            if key_ok || token_ok {
                ResponseTemplate::new(200).set_body_json(json!({
                    "data": [], "has_more": false, "first_id": null, "last_id": null
                }))
            } else {
                ResponseTemplate::new(401).set_body_json(json!({
                    "type": "error",
                    "error": { "type": "authentication_error", "message": "invalid x-api-key" }
                }))
            }
        })
        .mount(&ai)
        .await;
    let verifier = KeyVerifier::new(upstreams(None, Some(&ai.uri())));

    // API 키.
    let dir = tempfile::tempdir().unwrap();
    let secrets = Arc::new(SecretStore::open(dir.path()).unwrap());
    let source = AiKeys::new(Arc::clone(&secrets));
    assert!(
        pi_ai::CredentialSource::current(&source).await.is_none(),
        "키를 넣기 전에는 자격이 없다"
    );
    let wrong = Credentials::new().with("api_key", "test-ai-key-wrong-0002");
    let result = secrets.register(KeyKind::AiApiKey, wrong, &verifier).await;
    assert!(
        matches!(
            result,
            Err(SecretsError::VerifyFailed(VerifyFailure::Rejected))
        ),
        "{result:?}"
    );
    assert!(pi_ai::CredentialSource::current(&source).await.is_none());
    let right = Credentials::new().with("api_key", AI_KEY);
    secrets
        .register(KeyKind::AiApiKey, right, &verifier)
        .await
        .expect("공급자가 받으면 등록된다");
    assert_eq!(
        pi_ai::CredentialSource::current(&source).await,
        Some(Credential::ApiKey(Secret::new(AI_KEY)))
    );

    // 구독 OAuth 토큰.
    let dir = tempfile::tempdir().unwrap();
    let secrets = Arc::new(SecretStore::open(dir.path()).unwrap());
    let source = AiKeys::new(Arc::clone(&secrets));
    let tokens = Credentials::new()
        .with("access_token", OAUTH_ACCESS)
        .with("refresh_token", OAUTH_REFRESH);
    secrets
        .register(KeyKind::AiOAuth, tokens, &verifier)
        .await
        .expect("접근 토큰으로 닿으면 등록된다");
    assert_eq!(
        pi_ai::CredentialSource::current(&source).await,
        Some(Credential::OAuthToken(Secret::new(OAUTH_ACCESS)))
    );

    // 닿지 않으면 저장하지 않는다.
    let dir = tempfile::tempdir().unwrap();
    let secrets = Arc::new(SecretStore::open(dir.path()).unwrap());
    let unreachable = KeyVerifier::new(upstreams(None, None));
    let result = secrets
        .register(
            KeyKind::AiApiKey,
            Credentials::new().with("api_key", AI_KEY),
            &unreachable,
        )
        .await;
    assert!(
        matches!(
            result,
            Err(SecretsError::VerifyFailed(VerifyFailure::Unreachable))
        ),
        "{result:?}"
    );
    assert!(
        pi_ai::CredentialSource::current(&AiKeys::new(secrets))
            .await
            .is_none()
    );
}

// 검증: V31
#[tokio::test]
async fn 확인할_곳이_없는_자료_키는_저장하지_않는다() {
    let toss = FakeToss::start().await;
    let dir = tempfile::tempdir().unwrap();
    let secrets = SecretStore::open(dir.path()).unwrap();
    let verifier = KeyVerifier::new(upstreams(Some(&toss.server.uri()), None));
    let values = Credentials::new().with("api_key", "test-data-key-not-real");
    let result = secrets.register(KeyKind::Data, values, &verifier).await;
    assert!(
        matches!(result, Err(SecretsError::VerifyFailed(_))),
        "확인하지 못한 키는 저장하지 않는다: {result:?}"
    );
    assert!(secrets.status().unwrap().iter().all(|s| !s.registered));
    assert_eq!(
        toss.total_requests().await,
        0,
        "엉뚱한 서버에 키를 보내지 않는다"
    );
}

// ───────────── 3. 시세: 키가 없을 때 · 있을 때 · 시연 ─────────────

// 검증: V31
#[tokio::test]
async fn 증권사_키가_없으면_시세를_받지_않고_키가_없다고_알린다() {
    let toss = FakeToss::start().await;
    let dir = tempfile::tempdir().unwrap();
    let app = start_app(
        settings(dir.path(), &[]),
        wiring(Some(&toss), TestClock::at(kst(10, 0, 0))),
    )
    .await;
    let mut ws = connect(app.local_addr(), 0).await;

    let status = wait_feed_status(&mut ws).await;
    assert_eq!(status["state"], json!(FEED_NO_BROKER_KEY));
    let quotes = find_frame(&mut ws, QUIET, |frame| transient(frame, KIND_QUOTES)).await;
    assert!(
        quotes.is_none(),
        "키가 없으면 시세가 오지 않는다: {quotes:?}"
    );
    assert_eq!(
        toss.total_requests().await,
        0,
        "키가 없으면 증권사로 요청을 보내지 않는다"
    );

    // 나중에 연결한 화면도 같은 상태를 본다.
    let mut late = connect(app.local_addr(), 0).await;
    assert_eq!(
        wait_feed_status(&mut late).await["state"],
        json!(FEED_NO_BROKER_KEY)
    );
    stop(app).await;
}

// 검증: V31, V32
#[tokio::test]
async fn 키를_넣은_뒤에는_증권사_시세가_실시간_연결로_오고_바뀐다() {
    let toss = FakeToss::start().await;
    let dir = tempfile::tempdir().unwrap();
    register_broker(dir.path(), &toss).await;
    let app = start_app(
        settings(dir.path(), &[]),
        wiring(Some(&toss), TestClock::at(kst(10, 0, 0))),
    )
    .await;
    let mut ws = connect(app.local_addr(), 0).await;

    assert_eq!(wait_feed_status(&mut ws).await["state"], json!(FEED_LIVE));
    let quotes = wait_price(&mut ws, "005930", "72000").await;
    assert_eq!(quotes["demo"], json!(false), "실제 시세는 시연용이 아니다");
    assert_eq!(price_of(&quotes, "000660").as_deref(), Some("72000"));
    assert_eq!(
        quotes["quotes"][0]["received_at"],
        json!(kst(10, 0, 0).timestamp_millis()),
        "받은 시각은 시계가 준 값이다"
    );

    toss.set_price("72100");
    wait_price(&mut ws, "005930", "72100").await;
    toss.set_price("71900");
    wait_price(&mut ws, "000660", "71900").await;

    let prices = toss.requests(PRICES_PATH).await;
    assert!(prices.len() >= 3, "간격마다 다시 받는다");
    let auth = prices[0].headers.get("authorization").unwrap();
    assert_eq!(auth.as_bytes(), format!("Bearer {TOSS_TOKEN}").as_bytes());
    let wanted = prices[0]
        .url
        .query_pairs()
        .find(|(k, _)| k == "symbols")
        .map(|(_, v)| v.into_owned());
    assert_eq!(
        wanted.as_deref(),
        Some("005930,000660"),
        "설정한 종목을 지켜본다"
    );
    stop(app).await;
}

// 검증: V32
#[tokio::test]
async fn 시연_모드는_가짜_시세를_흘리고_시연용이라고_알린다() {
    let toss = FakeToss::start().await;
    let dir = tempfile::tempdir().unwrap();
    let app = start_app(
        settings(dir.path(), &["--demo"]),
        wiring(Some(&toss), TestClock::at(kst(10, 0, 0))),
    )
    .await;
    let mut ws = connect(app.local_addr(), 0).await;

    assert_eq!(wait_feed_status(&mut ws).await["state"], json!(FEED_DEMO));
    let mut seen: Vec<String> = Vec::new();
    let changed = find_frame(&mut ws, STEP, |frame| {
        let quotes = transient(frame, KIND_QUOTES)?;
        assert_eq!(quotes["demo"], json!(true), "시연용 표시가 있어야 한다");
        let first = price_of(&quotes, "005930").expect("지켜볼 종목의 시세가 있어야 한다");
        let second = price_of(&quotes, "000660").expect("지켜볼 종목의 시세가 있어야 한다");
        for price in [&first, &second] {
            let digits = price.replace('.', "");
            assert!(
                !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit()),
                "가격은 십진수 글자다: {price}"
            );
        }
        if !seen.contains(&first) {
            seen.push(first);
        }
        (seen.len() >= 3).then_some(())
    })
    .await;
    assert!(
        changed.is_some(),
        "시세가 실시간으로 바뀌어야 한다: {seen:?}"
    );
    assert_eq!(
        toss.total_requests().await,
        0,
        "시연 모드는 증권사로 요청을 보내지 않는다"
    );
    stop(app).await;
}

// ───────────── 4. 시세 늦음 (N6) ─────────────

// 검증: N6
#[tokio::test]
async fn 장_중에_시세가_30초_넘게_늦으면_시세_늦음을_알린다() {
    let toss = FakeToss::start().await;
    let dir = tempfile::tempdir().unwrap();
    register_broker(dir.path(), &toss).await;
    let clock = TestClock::at(kst(10, 0, 0));
    let app = start_app(
        settings(dir.path(), &[]),
        wiring(Some(&toss), Arc::clone(&clock)),
    )
    .await;
    let mut ws = connect(app.local_addr(), 0).await;
    wait_price(&mut ws, "005930", "72000").await;

    // 이제부터 받지 못한다. 가진 시세는 10:00:00에 받은 것이다.
    toss.failing.store(true, Ordering::SeqCst);
    tokio::time::sleep(Duration::from_millis(100)).await;
    clock.set(kst(10, 0, 30));
    let stale = find_frame(&mut ws, QUIET, |frame| transient(frame, KIND_QUOTES_STALE)).await;
    assert!(
        stale.is_none(),
        "딱 30초는 아직 늦은 것이 아니다: {stale:?}"
    );

    clock.set(kst(10, 0, 31));
    let stale = find_frame(&mut ws, STEP, |frame| transient(frame, KIND_QUOTES_STALE))
        .await
        .expect("30초를 넘으면 시세 늦음을 알린다");
    let symbols = stale["symbols"].as_array().expect("늦은 종목들");
    assert!(symbols.contains(&json!("005930")) && symbols.contains(&json!("000660")));

    // 늦음을 알린 뒤에도 흐름은 이어진다: 다시 받으면 새 시세가 온다.
    toss.set_price("72500");
    toss.failing.store(false, Ordering::SeqCst);
    wait_price(&mut ws, "005930", "72500").await;
    stop(app).await;
}

// 검증: N6
#[tokio::test]
async fn 장이_닫힌_동안은_시세_늦음을_알리지_않는다() {
    let toss = FakeToss::start().await;
    let dir = tempfile::tempdir().unwrap();
    register_broker(dir.path(), &toss).await;
    // 가짜 달력에서 20:00에 애프터마켓이 끝난다.
    let clock = TestClock::at(kst(21, 0, 0));
    let app = start_app(
        settings(dir.path(), &[]),
        wiring(Some(&toss), Arc::clone(&clock)),
    )
    .await;
    let mut ws = connect(app.local_addr(), 0).await;
    wait_price(&mut ws, "005930", "72000").await;

    toss.failing.store(true, Ordering::SeqCst);
    tokio::time::sleep(Duration::from_millis(100)).await;
    clock.set(kst(21, 5, 0));
    let stale = find_frame(&mut ws, QUIET, |frame| transient(frame, KIND_QUOTES_STALE)).await;
    assert!(
        stale.is_none(),
        "장이 닫혀 있으면 늦은 것으로 보지 않는다: {stale:?}"
    );
    stop(app).await;
}

// ───────────── 5. 화면 한 장 ─────────────

// 검증: D11
#[tokio::test]
async fn 서버가_화면_한_장을_내주고_화면은_바깥_주소를_부르지_않는다() {
    let page_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("web/index.html");
    let page = std::fs::read_to_string(&page_path).expect("web/index.html이 있어야 한다");
    assert!(
        page.to_ascii_lowercase().contains("<html"),
        "화면은 HTML이다"
    );
    for outside in ["http://", "https://", "src=\"//", "href=\"//"] {
        assert!(
            !page.contains(outside),
            "화면은 바깥 주소에서 아무것도 불러오지 않는다: {outside}"
        );
    }

    let dir = tempfile::tempdir().unwrap();
    let app = start_app(
        settings(dir.path(), &["--demo"]),
        wiring(None, TestClock::at(kst(10, 0, 0))),
    )
    .await;
    // 세션 없이 열린다(화면이 뜬 뒤에 세션을 받는다).
    let reply = reqwest::get(format!("{}/", origin_of(app.local_addr())))
        .await
        .expect("화면 요청이 닿아야 한다");
    assert_eq!(reply.status().as_u16(), 200);
    let content_type = reply
        .headers()
        .get("content-type")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(content_type.starts_with("text/html"), "{content_type}");
    assert_eq!(reply.text().await.unwrap(), page, "내주는 것은 그 파일이다");
    stop(app).await;
}

// ───────────── 2. 조립: 가리기 ─────────────

// 검증: V31
#[tokio::test]
async fn 등록된_키_값은_화면으로_나가지_않는다() {
    let toss = FakeToss::start().await;
    let dir = tempfile::tempdir().unwrap();
    register_broker(dir.path(), &toss).await;
    let app = start_app(
        settings(dir.path(), &[]),
        wiring(Some(&toss), TestClock::at(kst(10, 0, 0))),
    )
    .await;
    let mut ws = connect(app.local_addr(), 0).await;

    let leaky = json!({ "text": format!("토큰 발급 실패: secret={CLIENT_SECRET}") });
    let seq = app.publisher().publish("error", leaky).await.unwrap();
    let deadline = tokio::time::Instant::now() + STEP;
    loop {
        let text = next_text(&mut ws, deadline)
            .await
            .expect("프레임이 시간 안에 와야 한다")
            .expect("연결이 열려 있어야 한다");
        assert!(!text.contains(CLIENT_SECRET), "키 값이 화면으로 나갔다");
        let frame: Value = serde_json::from_str(&text).unwrap();
        if frame["seq"] == json!(seq) {
            assert!(
                text.contains(pi_secrets::REDACTED),
                "가린 자리가 보여야 한다"
            );
            break;
        }
    }
    stop(app).await;
}

// ───────────── 6. 끄기와 다시 켜기 (V6) ─────────────

// 검증: V6, E3
#[tokio::test]
async fn 끄면_연결이_정리되고_다시_켜면_이벤트_번호가_이어진다() {
    let dir = tempfile::tempdir().unwrap();
    let clock = TestClock::at(kst(10, 0, 0));
    let app = start_app(
        settings(dir.path(), &["--demo"]),
        wiring(None, Arc::clone(&clock)),
    )
    .await;
    let addr = app.local_addr();
    assert!(addr.ip().is_loopback(), "기본은 그 컴퓨터에서만 연다");
    let publisher = app.publisher();
    let first = publisher.publish("chat", json!({ "n": 1 })).await.unwrap();
    let second = publisher.publish("chat", json!({ "n": 2 })).await.unwrap();
    assert_eq!((first, second), (1, 2));
    let mut ws = connect(addr, 0).await;
    let got = find_frame(&mut ws, STEP, |frame| frame["seq"].as_u64()).await;
    assert_eq!(got, Some(1), "화면은 첫 이벤트까지만 받았다");

    stop(app).await;
    let deadline = tokio::time::Instant::now() + STEP;
    loop {
        match next_text(&mut ws, deadline).await {
            Ok(None) => break,
            Ok(Some(_)) => {}
            Err(()) => panic!("끄면 화면의 연결이 닫혀야 한다"),
        }
    }
    assert!(
        TcpStream::connect(addr).await.is_err(),
        "꺼진 뒤에는 주소가 닫혀 있다"
    );

    // 같은 보관 폴더로 다시 켠다.
    let app = start_app(
        settings(dir.path(), &["--demo"]),
        wiring(None, Arc::clone(&clock)),
    )
    .await;
    let third = app
        .publisher()
        .publish("chat", json!({ "n": 3 }))
        .await
        .unwrap();
    assert_eq!(third, 3, "다시 켜도 번호가 이어진다");

    // 화면이 다시 잇는다: 마지막으로 받은 번호(1) 뒤부터 온다.
    let mut ws = connect(app.local_addr(), 1).await;
    let mut seqs = Vec::new();
    while seqs.len() < 2 {
        let frame = find_frame(&mut ws, STEP, |frame| {
            frame["seq"]
                .as_u64()
                .map(|seq| (seq, frame["payload"].clone()))
        })
        .await
        .expect("놓친 이벤트가 와야 한다");
        seqs.push(frame);
    }
    assert_eq!(
        seqs,
        [(2, json!({ "n": 2 })), (3, json!({ "n": 3 }))],
        "놓친 이벤트부터 순서대로 받는다"
    );
    stop(app).await;
    assert!(dir.path().join(price_inha::DB_FILE).exists());
}
