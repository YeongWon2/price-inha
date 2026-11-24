//! 3차 작업 규칙 1~9: 시작 전 필수 설정.
//!
//! 근거: `docs/dev/modules/gate.md` "3차 작업", `dev.md`의 C3 · C16 · E4 · V31 · V46.
//!
//! 정한 모양(뼈대 `price_inha`의 `setup` 모듈 문서와 같다):
//! - `GET /api/setup`(세션): `{"items":[{"name","required","registered"}], "ready", "started", "demo",
//!   "symbols", "symbols_source"}`.
//! - `POST /api/setup/keys`(세션 · 같은 출처 · 그 컴퓨터): `{"item", "fields":{..}}` →
//!   200 `{"registered":true}` 또는 `{"error": 까닭}`.
//! - `POST /api/setup/symbols`(같은 조건): `{"symbols":[..]}`.
//! - 지나가는 이벤트 `setup`: `{"state":"required"|"started", "missing":[..], "demo":bool}`.

use super::*;
use price_inha::{
    DEFAULT_CALENDAR_RETRY, DEFAULT_REGISTER_ATTEMPTS, DEFAULT_REGISTER_WINDOW,
    ENV_CALENDAR_RETRY_MS, ENV_REGISTER_ATTEMPTS, ENV_REGISTER_WINDOW_MS, FEED_REQUEST_REJECTED,
    ITEM_AI, ITEM_BROKER, ITEM_DATA, ITEM_SYMBOLS, KEY_AI_API_KEY, KEY_BROKER, KIND_SETUP,
    KIND_SYMBOLS_REJECTED, SETUP_KEYS_PATH, SETUP_PATH, SETUP_REQUIRED, SETUP_STARTED,
    SETUP_SYMBOLS_PATH, WATCHLIST_FILE,
};

// ───────────── 도우미 ─────────────

/// 증권사 · AI가 받지 않는 시험용 값. 응답 · 프레임 · 파일에 나오면 안 되는 것을 찾을 때 쓴다.
const WRONG_ID: &str = "test-wrong-client-id-0009";
const WRONG_SECRET: &str = "test-wrong-client-secret-9";
const WRONG_AI_KEY: &str = "test-ai-key-wrong-0002";
/// 다시 등록할 때 쓰는 다른 증권사 키.
const OTHER_ID: &str = "test-client-id-0002";
const OTHER_SECRET: &str = "test-client-secret-other-2";
/// 칸 이름이 틀린 요청에 담는 시험용 값.
const STRAY_VALUE: &str = "test-stray-field-not-real";
/// 마지막 항목을 등록한 뒤 "시작함"이 와야 하는 시간("바로").
const SOON: Duration = Duration::from_secs(1);

/// HTTP 응답 하나.
pub(super) struct HttpReply {
    pub(super) status: u16,
    head: String,
    text: String,
}

impl HttpReply {
    pub(super) fn json(&self) -> Value {
        serde_json::from_str(&self.text)
            .unwrap_or_else(|_| panic!("응답 본문은 JSON이어야 한다(HTTP {})", self.status))
    }

    /// 헤더와 본문 전부(값이 새지 않았는지 볼 때).
    fn everything(&self) -> String {
        format!("{}\n{}", self.head, self.text)
    }
}

async fn send(request: reqwest::RequestBuilder) -> HttpReply {
    let reply = tokio::time::timeout(STEP, request.send())
        .await
        .expect("응답이 시간 안에 와야 한다")
        .expect("요청이 닿아야 한다");
    let status = reply.status().as_u16();
    let head = format!("{:?}", reply.headers());
    let text = reply.text().await.expect("본문을 읽어야 한다");
    HttpReply { status, head, text }
}

/// 설정 상태를 달라고 한다. `cookie`가 없으면 세션 없이 부른다.
async fn get_setup(addr: SocketAddr, cookie: Option<&str>) -> HttpReply {
    let mut request = reqwest::Client::new().get(format!("{}{SETUP_PATH}", origin_of(addr)));
    if let Some(cookie) = cookie {
        request = request.header("Cookie", cookie);
    }
    send(request).await
}

/// 세션을 받아 설정 상태를 읽는다.
pub(super) async fn setup_status(addr: SocketAddr) -> Value {
    let cookie = login(addr).await;
    let reply = get_setup(addr, Some(&cookie)).await;
    assert_eq!(reply.status, 200, "세션이 있으면 설정 상태를 읽는다");
    reply.json()
}

/// `POST`를 날것으로 보낸다. 쿠키와 출처는 준 것만 나간다.
async fn post_raw(
    addr: SocketAddr,
    path: &str,
    cookie: Option<&str>,
    origin: Option<&str>,
    body: String,
) -> HttpReply {
    let mut request = reqwest::Client::new()
        .post(format!("{}{path}", origin_of(addr)))
        .header("Content-Type", "application/json")
        .body(body);
    if let Some(cookie) = cookie {
        request = request.header("Cookie", cookie);
    }
    if let Some(origin) = origin {
        request = request.header("Origin", origin);
    }
    send(request).await
}

/// 세션을 받아 같은 출처에서 JSON을 보낸다.
pub(super) async fn post_json(addr: SocketAddr, path: &str, body: &Value) -> HttpReply {
    let cookie = login(addr).await;
    post_raw(
        addr,
        path,
        Some(&cookie),
        Some(&origin_of(addr)),
        body.to_string(),
    )
    .await
}

fn key_body(item: &str, fields: &[(&str, &str)]) -> Value {
    let fields: serde_json::Map<String, Value> = fields
        .iter()
        .map(|(name, value)| ((*name).to_string(), json!(value)))
        .collect();
    json!({ "item": item, "fields": fields })
}

pub(super) fn broker_body(id: &str, secret: &str) -> Value {
    key_body(KEY_BROKER, &[("client_id", id), ("client_secret", secret)])
}

fn ai_key_body(key: &str) -> Value {
    key_body(KEY_AI_API_KEY, &[("api_key", key)])
}

pub(super) fn symbols_body(symbols: &[&str]) -> Value {
    json!({ "symbols": symbols })
}

/// 성공 응답은 "등록됨"뿐이다.
pub(super) fn assert_registered(reply: &HttpReply) {
    assert_eq!(reply.status, 200, "등록돼야 한다: {}", reply.text);
    assert_eq!(
        reply.json(),
        json!({ "registered": true }),
        "성공은 등록됨만 알린다"
    );
}

/// 거부 응답은 상태와 까닭뿐이다.
fn assert_refused(reply: &HttpReply, status: u16, reason: &str) {
    assert_eq!(reply.status, status, "{reason}: {}", reply.text);
    assert_eq!(
        reply.json(),
        json!({ "error": reason }),
        "실패는 까닭만 알린다"
    );
}

/// 응답 어디에도(헤더 · 본문) 받은 값이 없다.
fn assert_no_values(reply: &HttpReply, values: &[&str]) {
    let all = reply.everything();
    for value in values {
        assert!(!all.contains(value), "응답에 받은 값이 나왔다");
    }
}

/// 설정 상태에서 그 항목.
fn item<'a>(status: &'a Value, name: &str) -> &'a Value {
    status["items"]
        .as_array()
        .expect("items는 목록이다")
        .iter()
        .find(|item| item["name"] == json!(name))
        .unwrap_or_else(|| panic!("설정 상태에 {name} 항목이 있어야 한다: {status}"))
}

fn registered(status: &Value, name: &str) -> bool {
    item(status, name)["registered"]
        .as_bool()
        .expect("registered는 참 · 거짓이다")
}

/// 새로 연결한 화면이 먼저 받는, 그 종류의 가장 최근 지나가는 이벤트.
pub(super) async fn latest(addr: SocketAddr, kind: &str) -> Value {
    let mut ws = connect(addr, 0).await;
    find_frame(&mut ws, STEP, |frame| transient(frame, kind))
        .await
        .unwrap_or_else(|| panic!("새로 연결한 화면이 {kind} 알림을 받아야 한다"))
}

/// 그 상태의 설정 알림이 올 때까지 기다린다.
async fn wait_setup(ws: &mut Ws, state: &str, wait: Duration) -> Option<Value> {
    find_frame(ws, wait, |frame| {
        transient(frame, KIND_SETUP).filter(|notice| notice["state"] == json!(state))
    })
    .await
}

/// 보관 폴더의 파일 이름들.
fn home_files(home: &Path) -> Vec<String> {
    std::fs::read_dir(home)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect()
}

/// 보관 폴더의 어느 파일에도 그 값이 없다(키 파일 · 저장소 · 설정 파일).
fn assert_not_in_home(home: &Path, values: &[&str]) {
    for name in home_files(home) {
        let bytes = std::fs::read(home.join(&name)).unwrap();
        for value in values {
            let found = bytes
                .windows(value.len())
                .any(|window| window == value.as_bytes());
            assert!(!found, "보관 폴더의 {name}에 저장하면 안 되는 값이 있다");
        }
    }
}

fn nothing_registered(home: &Path) -> bool {
    let secrets = SecretStore::open(home).unwrap();
    secrets.status().unwrap().iter().all(|s| !s.registered)
}

/// 토큰 발급 요청 가운데 본문에 `client_id`가 든 것의 수.
async fn token_requests_with(toss: &FakeToss, client_id: &str) -> usize {
    toss.requests(TOKEN_PATH)
        .await
        .iter()
        .filter(|request| String::from_utf8_lossy(&request.body).contains(client_id))
        .count()
}

// ───────────── 규칙 1 · 10: 시작 조건 ─────────────

/// 미리 등록해 둘 것.
struct Given {
    broker: bool,
    ai: bool,
    symbols: bool,
}

/// 필수 항목이 빠진 채로 띄우면: 시작하지 않고, 바깥으로 아무 요청도 보내지 않고,
/// 무엇이 빠졌는지 알린다.
async fn assert_does_not_start(given: Given, missing: &[&str]) {
    let toss = FakeToss::start().await;
    let ai = FakeAi::start().await;
    let dir = tempfile::tempdir().unwrap();
    if given.broker {
        register_broker(dir.path(), &toss).await;
    }
    if given.ai {
        register_ai(dir.path(), &ai).await;
    }
    let before = (toss.total_requests().await, ai.total_requests().await);
    let settings = if given.symbols {
        settings(dir.path(), &[])
    } else {
        settings_without_symbols(dir.path(), &[])
    };
    let app = start_app(
        settings,
        wiring_both(&toss, &ai, TestClock::at(kst(10, 0, 0))),
    )
    .await;
    let addr = app.local_addr();

    let notice = latest(addr, KIND_SETUP).await;
    assert_eq!(notice["state"], json!(SETUP_REQUIRED), "{notice}");
    assert_eq!(notice["missing"], json!(missing), "빠진 항목을 알린다");
    assert_eq!(notice["demo"], json!(false));
    assert_eq!(
        latest(addr, KIND_FEED_STATUS).await["state"],
        json!(FEED_SETUP_REQUIRED),
        "시작 전의 시세 상태는 설정이 필요함이다"
    );
    let mut ws = connect(addr, 0).await;
    let quotes = find_frame(&mut ws, QUIET, |frame| transient(frame, KIND_QUOTES)).await;
    assert!(
        quotes.is_none(),
        "시작하지 않았는데 시세가 왔다: {quotes:?}"
    );

    let status = setup_status(addr).await;
    assert_eq!(status["ready"], json!(false), "{status}");
    assert_eq!(status["started"], json!(false), "{status}");
    assert_eq!(registered(&status, ITEM_BROKER), given.broker);
    assert_eq!(registered(&status, ITEM_AI), given.ai);
    assert_eq!(registered(&status, ITEM_SYMBOLS), given.symbols);

    assert_eq!(
        (toss.total_requests().await, ai.total_requests().await),
        before,
        "시작 전에는 증권사에도 AI에도 요청을 보내지 않는다(토큰 · 달력 · 시세 모두)"
    );
    stop(app).await;
}

// 검증: V31, C3
#[tokio::test]
async fn 증권사_키가_빠져_있으면_시작하지_않는다() {
    let given = Given {
        broker: false,
        ai: true,
        symbols: true,
    };
    assert_does_not_start(given, &[ITEM_BROKER]).await;
}

// 검증: V31, C3
#[tokio::test]
async fn ai_키가_빠져_있으면_증권사_키가_있어도_시작하지_않는다() {
    let given = Given {
        broker: true,
        ai: false,
        symbols: true,
    };
    assert_does_not_start(given, &[ITEM_AI]).await;
}

// 검증: V31
#[tokio::test]
async fn 지켜볼_종목이_없으면_키가_있어도_증권사를_부르지_않는다() {
    // 규칙 10: 종목이 비어 있으면 시세도 달력도 토큰도 요청하지 않는다(시작 전 상태다).
    let given = Given {
        broker: true,
        ai: true,
        symbols: false,
    };
    assert_does_not_start(given, &[ITEM_SYMBOLS]).await;
}

// 검증: V31
#[tokio::test]
async fn 아무것도_없으면_빠진_필수_항목을_모두_알린다() {
    let given = Given {
        broker: false,
        ai: false,
        symbols: false,
    };
    assert_does_not_start(given, &[ITEM_BROKER, ITEM_AI, ITEM_SYMBOLS]).await;
}

// 검증: V31, V32
#[tokio::test]
async fn 필수_항목이_모두_있으면_시작하고_갖춰졌다고_알린다() {
    let toss = FakeToss::start().await;
    let dir = tempfile::tempdir().unwrap();
    register_keys(dir.path(), &toss).await;
    let app = start_app(
        settings(dir.path(), &[]),
        wiring(Some(&toss), TestClock::at(kst(10, 0, 0))),
    )
    .await;
    let addr = app.local_addr();
    let mut ws = connect(addr, 0).await;
    wait_price(&mut ws, "005930", "72000").await;

    let notice = latest(addr, KIND_SETUP).await;
    assert_eq!(notice["state"], json!(SETUP_STARTED), "{notice}");
    assert_eq!(notice["missing"], json!([]));
    assert_eq!(notice["demo"], json!(false));
    let status = setup_status(addr).await;
    assert_eq!(status["ready"], json!(true), "{status}");
    assert_eq!(status["started"], json!(true), "{status}");
    assert_eq!(status["demo"], json!(false));
    assert_eq!(status["symbols"], json!(["005930", "000660"]));
    assert_eq!(status["symbols_source"], json!("argument"));
    stop(app).await;
}

// 검증: V31
#[tokio::test]
async fn 옛_구독_토큰_파일만_있으면_ai_연결이_갖춰진_것이_아니다() {
    // 6차: 구독 토큰은 더 쓰지 않는다. 보관 폴더에 옛 auth.json이 남아 있어도 AI 연결이 아니다.
    let toss = FakeToss::start().await;
    let ai = FakeAi::start().await;
    let dir = tempfile::tempdir().unwrap();
    register_broker(dir.path(), &toss).await;
    std::fs::write(
        dir.path().join(pi_secrets::LEGACY_AUTH_FILE),
        r#"{"access_token":"test-legacy-access-not-real"}"#,
    )
    .unwrap();
    let before = (toss.total_requests().await, ai.total_requests().await);
    let app = start_app(
        settings(dir.path(), &[]),
        wiring_both(&toss, &ai, TestClock::at(kst(10, 0, 0))),
    )
    .await;
    let addr = app.local_addr();
    let notice = latest(addr, KIND_SETUP).await;
    assert_eq!(notice["state"], json!(SETUP_REQUIRED), "{notice}");
    assert_eq!(notice["missing"], json!([ITEM_AI]));
    let status = setup_status(addr).await;
    assert!(!registered(&status, ITEM_AI), "{status}");
    assert_eq!(status["ready"], json!(false));
    assert_eq!(
        (toss.total_requests().await, ai.total_requests().await),
        before,
        "시작하지 않았으니 바깥으로 요청을 보내지 않는다"
    );
    stop(app).await;
}

// ───────────── 규칙 2: 설정 상태 (V46) ─────────────

/// JSON 어디에도 숫자가 없다(값의 길이 같은 것을 싣지 않는다).
fn has_number(value: &Value) -> bool {
    match value {
        Value::Number(_) => true,
        Value::Array(list) => list.iter().any(has_number),
        Value::Object(map) => map.values().any(has_number),
        _ => false,
    }
}

fn keys_of(value: &Value) -> Vec<&str> {
    let mut keys: Vec<&str> = value
        .as_object()
        .expect("객체여야 한다")
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    keys
}

// 검증: V46, C3
#[tokio::test]
async fn 설정_상태는_등록_여부만_알려_주고_키_값은_어디에도_없다() {
    let toss = FakeToss::start().await;
    let dir = tempfile::tempdir().unwrap();
    register_keys(dir.path(), &toss).await;
    let app = start_app(
        settings(dir.path(), &[]),
        wiring(Some(&toss), TestClock::at(kst(10, 0, 0))),
    )
    .await;
    let addr = app.local_addr();
    let cookie = login(addr).await;
    let reply = get_setup(addr, Some(&cookie)).await;
    assert_eq!(reply.status, 200);
    assert_no_values(&reply, &[CLIENT_ID, CLIENT_SECRET, AI_KEY]);

    let status = reply.json();
    assert_eq!(
        keys_of(&status),
        [
            "ai",
            "demo",
            "items",
            "market",
            "models",
            "ready",
            "started",
            "symbols",
            "symbols_source"
        ],
        "정한 칸만 있다(6차: AI 연결 칸 ai가 더해졌다, 31차: 시장 market · 담당 모델 models가 더해졌다)"
    );
    assert_eq!(
        keys_of(&status["ai"]),
        [
            "cli",
            "cli_version",
            "command",
            "config_dir",
            "legacy_auth_file",
            "method",
            "subscription"
        ],
        "AI 연결 칸에는 고른 방식 · 경로 · 확인 상태 · 옛 파일 알림만 있다(키 값 없음)"
    );
    let items = status["items"].as_array().unwrap();
    let names: Vec<&Value> = items.iter().map(|item| &item["name"]).collect();
    assert_eq!(names, [ITEM_BROKER, ITEM_AI, ITEM_SYMBOLS, ITEM_DATA]);
    for item in items {
        assert_eq!(
            keys_of(item),
            ["name", "registered", "required"],
            "항목에는 이름 · 필수 여부 · 등록 여부만 있다(값 · 일부 · 길이 없음)"
        );
    }
    let required: Vec<&Value> = items.iter().map(|item| &item["required"]).collect();
    assert_eq!(
        required,
        [true, true, true, false],
        "자료 키는 필수가 아니다"
    );
    let flags: Vec<&Value> = items.iter().map(|item| &item["registered"]).collect();
    assert_eq!(flags, [true, true, true, false]);
    assert!(!has_number(&status), "길이 같은 숫자를 싣지 않는다");
    stop(app).await;
}

// 검증: V46, C15
#[tokio::test]
async fn 설정_상태는_세션이_있어야_읽는다() {
    let dir = tempfile::tempdir().unwrap();
    let app = start_app(
        settings(dir.path(), &[]),
        wiring(None, TestClock::at(kst(10, 0, 0))),
    )
    .await;
    let addr = app.local_addr();
    let reply = get_setup(addr, None).await;
    assert_eq!(reply.status, 401, "세션 없이 읽을 수 없다");
    assert!(
        !reply.text.contains("items"),
        "거부 응답에 상태를 싣지 않는다"
    );
    let reply = get_setup(addr, Some("pi_session=not-a-session")).await;
    assert_eq!(reply.status, 401, "모르는 세션으로 읽을 수 없다");
    let cookie = login(addr).await;
    let reply = get_setup(addr, Some(&cookie)).await;
    assert_eq!(reply.status, 200, "세션이 있으면 읽는다");
    assert!(reply.json()["items"].is_array());
    stop(app).await;
}

// ───────────── 규칙 3: 키 등록 (C3 · C16 · E4) ─────────────

// 검증: C3, V46
#[tokio::test]
async fn 증권사_키를_등록하면_확인하고_등록됨만_돌려준다() {
    let toss = FakeToss::start().await;
    let ai = FakeAi::start().await;
    let dir = tempfile::tempdir().unwrap();
    let app = start_app(
        settings(dir.path(), &[]),
        wiring_both(&toss, &ai, TestClock::at(kst(10, 0, 0))),
    )
    .await;
    let addr = app.local_addr();
    assert!(!registered(&setup_status(addr).await, ITEM_BROKER));

    let reply = post_json(
        addr,
        SETUP_KEYS_PATH,
        &broker_body(CLIENT_ID, CLIENT_SECRET),
    )
    .await;
    assert_registered(&reply);
    assert_no_values(&reply, &[CLIENT_ID, CLIENT_SECRET]);
    assert_eq!(
        token_requests_with(&toss, CLIENT_ID).await,
        1,
        "받은 키로 증권사에 실제로 닿는지 확인한다(토큰 발급 한 번)"
    );

    let status = setup_status(addr).await;
    assert!(registered(&status, ITEM_BROKER), "{status}");
    assert_eq!(status["ready"], json!(false), "AI 키가 아직 없다");
    assert_eq!(status["started"], json!(false));
    assert_eq!(
        toss.requests(PRICES_PATH).await.len(),
        0,
        "필수 항목이 다 차기 전에는 시세를 받지 않는다"
    );
    stop(app).await;

    // 다시 켜도 등록돼 있다(보관 폴더에 저장됐다).
    let secrets = SecretStore::open(dir.path()).unwrap();
    let source = BrokerKeys::new(Arc::new(secrets));
    let found = pi_market::CredentialSource::credentials(&source).expect("저장됐다");
    assert_eq!(found.client_id(), CLIENT_ID);
}

// 검증: C3, V46
#[tokio::test]
async fn ai_api_키를_등록하고_구독_토큰_항목은_받지_않는다() {
    let toss = FakeToss::start().await;
    let ai = FakeAi::start().await;

    // API 키.
    let dir = tempfile::tempdir().unwrap();
    let app = start_app(
        settings(dir.path(), &[]),
        wiring_both(&toss, &ai, TestClock::at(kst(10, 0, 0))),
    )
    .await;
    let addr = app.local_addr();
    let reply = post_json(addr, SETUP_KEYS_PATH, &ai_key_body(AI_KEY)).await;
    assert_registered(&reply);
    assert_no_values(&reply, &[AI_KEY]);
    assert_eq!(ai.total_requests().await, 1, "AI 공급자에 닿는지 확인한다");
    assert!(registered(&setup_status(addr).await, ITEM_AI));
    stop(app).await;

    // 6차: 구독 OAuth 토큰을 등록하는 길은 없다. 확인하러 가기 전에 잘못된 입력으로 거부한다.
    let dir = tempfile::tempdir().unwrap();
    let app = start_app(
        settings(dir.path(), &[]),
        wiring_both(&toss, &ai, TestClock::at(kst(10, 0, 0))),
    )
    .await;
    let addr = app.local_addr();
    let body = key_body(
        "ai_oauth",
        &[
            ("access_token", "test-oauth-access-not-real"),
            ("refresh_token", "test-oauth-refresh-not-real"),
        ],
    );
    let reply = post_json(addr, SETUP_KEYS_PATH, &body).await;
    assert_refused(&reply, 400, "invalid_input");
    assert_no_values(
        &reply,
        &["test-oauth-access-not-real", "test-oauth-refresh-not-real"],
    );
    assert_eq!(
        ai.total_requests().await,
        1,
        "구독 토큰으로 확인하러 가지 않는다"
    );
    assert!(!registered(&setup_status(addr).await, ITEM_AI));
    assert!(nothing_registered(dir.path()));
    assert!(
        !dir.path().join(pi_secrets::LEGACY_AUTH_FILE).exists(),
        "구독 토큰 파일을 만들지 않는다"
    );
    assert_eq!(
        toss.total_requests().await,
        0,
        "AI 키를 증권사로 보내지 않는다"
    );
    stop(app).await;
}

// 검증: E4, C3, V46, N8
#[tokio::test]
async fn 증권사가_받지_않는_키는_저장하지_않고_맞지_않다고만_알린다() {
    let toss = FakeToss::start().await;
    let ai = FakeAi::start().await;
    toss.token_status.store(401, Ordering::SeqCst);
    let dir = tempfile::tempdir().unwrap();
    let app = start_app(
        settings(dir.path(), &[]),
        wiring_both(&toss, &ai, TestClock::at(kst(10, 0, 0))),
    )
    .await;
    let addr = app.local_addr();
    let mut ws = connect(addr, 0).await;

    let reply = post_json(addr, SETUP_KEYS_PATH, &broker_body(WRONG_ID, WRONG_SECRET)).await;
    assert_refused(&reply, 422, "key_rejected");
    assert_no_values(&reply, &[WRONG_ID, WRONG_SECRET]);
    assert_eq!(token_requests_with(&toss, WRONG_ID).await, 1);

    let reply = post_json(addr, SETUP_KEYS_PATH, &ai_key_body(WRONG_AI_KEY)).await;
    assert_refused(&reply, 422, "key_rejected");
    assert_no_values(&reply, &[WRONG_AI_KEY]);

    // 다시 읽어도 등록되지 않았다.
    let status = setup_status(addr).await;
    assert!(!registered(&status, ITEM_BROKER) && !registered(&status, ITEM_AI));
    assert_eq!(status["started"], json!(false));

    // 그 뒤 실시간 연결로 온 프레임에도 받은 값이 없다.
    let deadline = tokio::time::Instant::now() + QUIET;
    while let Ok(Some(text)) = next_text(&mut ws, deadline).await {
        for value in [WRONG_ID, WRONG_SECRET, WRONG_AI_KEY] {
            assert!(!text.contains(value), "프레임에 받은 값이 나왔다");
        }
    }
    stop(app).await;

    // 키 파일에도 저장소(이벤트 · 접속 기록)에도 없다.
    assert_not_in_home(dir.path(), &[WRONG_ID, WRONG_SECRET, WRONG_AI_KEY]);
    assert!(nothing_registered(dir.path()));
}

// 검증: E4, C3
#[tokio::test]
async fn 서버에_닿지_않으면_저장하지_않고_닿지_않는다고_알린다() {
    let dir = tempfile::tempdir().unwrap();
    // 증권사도 AI도 아무도 듣지 않는 로컬 주소다.
    let app = start_app(
        settings(dir.path(), &[]),
        wiring(None, TestClock::at(kst(10, 0, 0))),
    )
    .await;
    let addr = app.local_addr();

    let reply = post_json(
        addr,
        SETUP_KEYS_PATH,
        &broker_body(CLIENT_ID, CLIENT_SECRET),
    )
    .await;
    assert_refused(&reply, 502, "unreachable");
    assert_no_values(&reply, &[CLIENT_ID, CLIENT_SECRET]);
    let reply = post_json(addr, SETUP_KEYS_PATH, &ai_key_body(AI_KEY)).await;
    assert_refused(&reply, 502, "unreachable");
    assert_no_values(&reply, &[AI_KEY]);

    let status = setup_status(addr).await;
    assert!(!registered(&status, ITEM_BROKER) && !registered(&status, ITEM_AI));
    stop(app).await;
    assert_not_in_home(dir.path(), &[CLIENT_ID, CLIENT_SECRET, AI_KEY]);
    assert!(nothing_registered(dir.path()));
}

// 검증: E4, C3
#[tokio::test]
async fn 증권사가_장애여도_확인하지_못한_키는_저장하지_않는다() {
    let toss = FakeToss::start().await;
    toss.token_status.store(503, Ordering::SeqCst);
    let dir = tempfile::tempdir().unwrap();
    let app = start_app(
        settings(dir.path(), &[]),
        wiring(Some(&toss), TestClock::at(kst(10, 0, 0))),
    )
    .await;
    let addr = app.local_addr();
    let reply = post_json(
        addr,
        SETUP_KEYS_PATH,
        &broker_body(CLIENT_ID, CLIENT_SECRET),
    )
    .await;
    assert_refused(&reply, 502, "unreachable");
    assert!(!registered(&setup_status(addr).await, ITEM_BROKER));
    stop(app).await;
    assert!(nothing_registered(dir.path()));
}

// ───────────── 규칙 3 · 4: 누가 등록할 수 있나 ─────────────

// 검증: C3, C15
#[tokio::test]
async fn 키_등록과_종목_등록은_세션과_같은_출처가_있어야_한다() {
    let toss = FakeToss::start().await;
    let ai = FakeAi::start().await;
    let dir = tempfile::tempdir().unwrap();
    let app = start_app(
        settings_without_symbols(dir.path(), &[]),
        wiring_both(&toss, &ai, TestClock::at(kst(10, 0, 0))),
    )
    .await;
    let addr = app.local_addr();
    let origin = origin_of(addr);
    let cookie = login(addr).await;
    let bodies = [
        (SETUP_KEYS_PATH, broker_body(CLIENT_ID, CLIENT_SECRET)),
        (SETUP_KEYS_PATH, ai_key_body(AI_KEY)),
        (SETUP_SYMBOLS_PATH, symbols_body(&["005930"])),
    ];
    for (path, body) in &bodies {
        // 세션이 없다.
        let reply = post_raw(addr, path, None, Some(&origin), body.to_string()).await;
        assert_eq!(reply.status, 401, "{path}: 세션 없이 등록할 수 없다");
        // 출처가 없거나 다르다.
        let reply = post_raw(addr, path, Some(&cookie), None, body.to_string()).await;
        assert!(
            (400..500).contains(&reply.status),
            "{path}: Origin이 없으면 거부한다: {}",
            reply.status
        );
        let evil = Some("http://evil.example");
        let reply = post_raw(addr, path, Some(&cookie), evil, body.to_string()).await;
        assert!(
            (400..500).contains(&reply.status),
            "{path}: 다른 출처면 거부한다: {}",
            reply.status
        );
        assert_no_values(&reply, &[CLIENT_ID, CLIENT_SECRET, AI_KEY]);
    }
    assert_eq!(
        (toss.total_requests().await, ai.total_requests().await),
        (0, 0),
        "거부된 요청의 키로 확인하러 가지 않는다"
    );
    let status = setup_status(addr).await;
    assert!(!registered(&status, ITEM_BROKER));
    assert!(!registered(&status, ITEM_AI));
    assert!(!registered(&status, ITEM_SYMBOLS));
    stop(app).await;
    assert!(nothing_registered(dir.path()));
    assert!(!dir.path().join(WATCHLIST_FILE).exists());
}

// 검증: C3, C16, C15
#[tokio::test]
async fn 그_컴퓨터_밖에서는_키도_종목도_등록할_수_없다() {
    let toss = FakeToss::start().await;
    let ai = FakeAi::start().await;
    let dir = tempfile::tempdir().unwrap();
    // 상대 주소를 루프백이 아닌 것으로 본다(문서용 주소 대역).
    let outside: SocketAddr = "203.0.113.9:40000".parse().unwrap();
    let mut wiring = wiring_both(&toss, &ai, TestClock::at(kst(10, 0, 0)));
    wiring.peer_override = Some(Arc::new(move |_actual: SocketAddr| outside));
    let app = start_app(settings_without_symbols(dir.path(), &[]), wiring).await;
    let addr = app.local_addr();
    let origin = origin_of(addr);

    for (path, body) in [
        (SETUP_KEYS_PATH, broker_body(CLIENT_ID, CLIENT_SECRET)),
        (SETUP_SYMBOLS_PATH, symbols_body(&["005930"])),
    ] {
        let reply = post_raw(addr, path, None, Some(&origin), body.to_string()).await;
        assert!(
            reply.status == 401 || reply.status == 403,
            "{path}: 밖에서 온 요청은 거부한다: {}",
            reply.status
        );
        assert_no_values(&reply, &[CLIENT_ID, CLIENT_SECRET]);
    }
    let reply = get_setup(addr, None).await;
    assert_eq!(reply.status, 401, "설정 상태도 밖에서는 읽지 못한다");
    // 밖에서는 세션도 받지 못한다(테스트용 상대 주소가 접속 관문까지 이어졌다는 뜻이기도 하다).
    let reply = post_raw(
        addr,
        pi_gate::SESSION_PATH,
        None,
        Some(&origin),
        String::new(),
    )
    .await;
    assert_eq!(reply.status, 401, "밖에서 온 요청은 세션을 받지 못한다");
    assert_eq!(toss.total_requests().await, 0);
    stop(app).await;
    assert!(nothing_registered(dir.path()));
    assert!(!dir.path().join(WATCHLIST_FILE).exists());
}

// ───────────── 규칙 9: 잘못된 입력과 되풀이 ─────────────

// 검증: C3, E4
#[tokio::test]
async fn 잘못된_입력은_확인하러_가기_전에_거부한다() {
    let toss = FakeToss::start().await;
    let ai = FakeAi::start().await;
    let dir = tempfile::tempdir().unwrap();
    let app = start_app(
        settings(dir.path(), &[]),
        wiring_both(&toss, &ai, TestClock::at(kst(10, 0, 0))),
    )
    .await;
    let addr = app.local_addr();
    let cookie = login(addr).await;
    let origin = origin_of(addr);

    let bad_bodies = [
        "".to_string(),
        "not json".to_string(),
        "[]".to_string(),
        "\"broker\"".to_string(),
        // 항목이 없다 · 모르는 항목 · 설정 항목 이름이지만 키 항목이 아닌 것.
        json!({ "fields": { "client_id": CLIENT_ID, "client_secret": CLIENT_SECRET } }).to_string(),
        key_body("stock_exchange", &[("api_key", AI_KEY)]).to_string(),
        key_body(ITEM_AI, &[("api_key", AI_KEY)]).to_string(),
        key_body(ITEM_SYMBOLS, &[("api_key", AI_KEY)]).to_string(),
        json!({ "item": 7, "fields": { "api_key": AI_KEY } }).to_string(),
        // 칸이 없다 · 빠졌다 · 남는다.
        json!({ "item": KEY_BROKER }).to_string(),
        json!({ "item": KEY_BROKER, "fields": [CLIENT_ID, CLIENT_SECRET] }).to_string(),
        key_body(KEY_BROKER, &[("client_id", CLIENT_ID)]).to_string(),
        key_body(
            KEY_BROKER,
            &[
                ("client_id", CLIENT_ID),
                ("client_secret", CLIENT_SECRET),
                ("account", "test-extra-field-0001"),
            ],
        )
        .to_string(),
        key_body(KEY_AI_API_KEY, &[("access_token", STRAY_VALUE)]).to_string(),
        // 빈 값 · 공백뿐 · 너무 짧은 값 · 글자가 아닌 값 · 줄바꿈이 든 값.
        broker_body(CLIENT_ID, "").to_string(),
        broker_body("", CLIENT_SECRET).to_string(),
        broker_body(CLIENT_ID, "        ").to_string(),
        broker_body(CLIENT_ID, "short").to_string(),
        ai_key_body("1234567").to_string(),
        json!({ "item": KEY_AI_API_KEY, "fields": { "api_key": 1_234_567_890_u64 } }).to_string(),
        json!({ "item": KEY_AI_API_KEY, "fields": { "api_key": null } }).to_string(),
        broker_body(CLIENT_ID, "test-client-secret\nnot-real").to_string(),
    ];
    for body in &bad_bodies {
        let reply = post_raw(
            addr,
            SETUP_KEYS_PATH,
            Some(&cookie),
            Some(&origin),
            body.clone(),
        )
        .await;
        assert_eq!(reply.status, 400, "거부해야 하는 입력: {body}");
        assert_eq!(reply.json(), json!({ "error": "invalid_input" }), "{body}");
        assert_no_values(&reply, &[CLIENT_ID, CLIENT_SECRET, AI_KEY, STRAY_VALUE]);
    }
    assert_eq!(
        (toss.total_requests().await, ai.total_requests().await),
        (0, 0),
        "잘못된 입력으로는 증권사 · AI 서버를 두드리지 않는다"
    );
    let status = setup_status(addr).await;
    assert!(!registered(&status, ITEM_BROKER) && !registered(&status, ITEM_AI));

    // 잘못된 입력은 확인기를 부르지 않았으므로 시도 횟수를 쓰지 않았다: 바른 키는 등록된다.
    assert!(bad_bodies.len() > DEFAULT_REGISTER_ATTEMPTS as usize);
    let reply = post_json(
        addr,
        SETUP_KEYS_PATH,
        &broker_body(CLIENT_ID, CLIENT_SECRET),
    )
    .await;
    assert_registered(&reply);
    stop(app).await;
}

/// 키 등록 시도 제한을 테스트가 줄인다.
fn limited(mut settings: Settings, attempts: u32, window: Duration) -> Settings {
    settings.register_attempts = attempts;
    settings.register_window = window;
    settings
}

// 검증: C3, E4
#[tokio::test]
async fn 키_등록을_정한_횟수보다_많이_시도하면_확인하러_가지_않고_거부한다() {
    let toss = FakeToss::start().await;
    toss.token_status.store(401, Ordering::SeqCst);
    let dir = tempfile::tempdir().unwrap();
    let app = start_app(
        limited(settings(dir.path(), &[]), 3, Duration::from_secs(60)),
        wiring(Some(&toss), TestClock::at(kst(10, 0, 0))),
    )
    .await;
    let addr = app.local_addr();

    for attempt in 1..=3 {
        let reply = post_json(addr, SETUP_KEYS_PATH, &broker_body(WRONG_ID, WRONG_SECRET)).await;
        assert_refused(&reply, 422, "key_rejected");
        assert_eq!(
            toss.requests(TOKEN_PATH).await.len(),
            attempt,
            "횟수 안에서는 확인하러 간다"
        );
    }
    let reply = post_json(addr, SETUP_KEYS_PATH, &broker_body(WRONG_ID, WRONG_SECRET)).await;
    assert_refused(&reply, 429, "too_many_attempts");
    assert_no_values(&reply, &[WRONG_ID, WRONG_SECRET]);

    // 이제 증권사가 받는 키여도, 횟수를 넘은 동안에는 확인하지 않고 저장하지도 않는다.
    toss.token_status.store(0, Ordering::SeqCst);
    let reply = post_json(
        addr,
        SETUP_KEYS_PATH,
        &broker_body(CLIENT_ID, CLIENT_SECRET),
    )
    .await;
    assert_refused(&reply, 429, "too_many_attempts");
    assert_eq!(
        toss.requests(TOKEN_PATH).await.len(),
        3,
        "횟수를 넘으면 증권사를 두드리지 않는다"
    );
    assert!(!registered(&setup_status(addr).await, ITEM_BROKER));
    stop(app).await;
    assert!(nothing_registered(dir.path()));
}

// 검증: C3, C16
#[tokio::test]
async fn 정한_시간이_지나면_다시_시도할_수_있다() {
    let toss = FakeToss::start().await;
    toss.token_status.store(401, Ordering::SeqCst);
    let dir = tempfile::tempdir().unwrap();
    let window = Duration::from_millis(300);
    let app = start_app(
        limited(settings(dir.path(), &[]), 1, window),
        wiring(Some(&toss), TestClock::at(kst(10, 0, 0))),
    )
    .await;
    let addr = app.local_addr();

    let reply = post_json(addr, SETUP_KEYS_PATH, &broker_body(WRONG_ID, WRONG_SECRET)).await;
    assert_refused(&reply, 422, "key_rejected");
    let reply = post_json(
        addr,
        SETUP_KEYS_PATH,
        &broker_body(CLIENT_ID, CLIENT_SECRET),
    )
    .await;
    assert_refused(&reply, 429, "too_many_attempts");
    assert_eq!(toss.requests(TOKEN_PATH).await.len(), 1);

    tokio::time::sleep(window + Duration::from_millis(200)).await;
    toss.token_status.store(0, Ordering::SeqCst);
    let reply = post_json(
        addr,
        SETUP_KEYS_PATH,
        &broker_body(CLIENT_ID, CLIENT_SECRET),
    )
    .await;
    assert_registered(&reply);
    assert_eq!(toss.requests(TOKEN_PATH).await.len(), 2);
    assert!(registered(&setup_status(addr).await, ITEM_BROKER));
    stop(app).await;
}

// 검증: C3
#[test]
fn 달력_간격과_키_등록_시도_제한은_설정으로_준다() {
    let home = ["--home", "/tmp/pi-home"];
    let parse = |extra: &[&str]| {
        let mut list = home.to_vec();
        list.extend_from_slice(extra);
        Settings::parse(&args(&list), &no_env, None)
    };
    let defaults = parse(&[]).unwrap();
    assert_eq!(defaults.calendar_retry, DEFAULT_CALENDAR_RETRY);
    assert_eq!(defaults.register_attempts, DEFAULT_REGISTER_ATTEMPTS);
    assert_eq!(defaults.register_window, DEFAULT_REGISTER_WINDOW);

    let given = parse(&[
        "--calendar-retry-ms",
        "1500",
        "--register-attempts",
        "3",
        "--register-window-ms",
        "20000",
    ])
    .expect("3차 설정은 인자로 준다");
    assert_eq!(given.calendar_retry, Duration::from_millis(1500));
    assert_eq!(given.register_attempts, 3);
    assert_eq!(given.register_window, Duration::from_millis(20_000));

    let env = |name: &str| -> Option<String> {
        let value = match name {
            n if n == ENV_HOME => "/srv/pi",
            n if n == ENV_CALENDAR_RETRY_MS => "700",
            n if n == ENV_REGISTER_ATTEMPTS => "9",
            n if n == ENV_REGISTER_WINDOW_MS => "45000",
            _ => return None,
        };
        Some(value.to_string())
    };
    let from_env = Settings::parse(&[], &env, None).unwrap();
    assert_eq!(from_env.calendar_retry, Duration::from_millis(700));
    assert_eq!(from_env.register_attempts, 9);
    assert_eq!(from_env.register_window, Duration::from_millis(45_000));
    let mixed = Settings::parse(&args(&["--register-attempts", "2"]), &env, None)
        .expect("인자가 환경 변수를 이긴다");
    assert_eq!(mixed.register_attempts, 2);
    assert_eq!(mixed.register_window, Duration::from_millis(45_000));

    for name in [
        "--calendar-retry-ms",
        "--register-attempts",
        "--register-window-ms",
    ] {
        assert_eq!(
            parse(&[name, "0"]),
            Err(SettingsError::InvalidValue { name }),
            "0은 받지 않는다"
        );
        assert_eq!(
            parse(&[name, "많이"]),
            Err(SettingsError::InvalidValue { name })
        );
    }
}

// ───────────── 규칙 5: 다 등록되면 바로 시작한다 ─────────────

// 검증: C3, V31, V32
#[tokio::test]
async fn 마지막_필수_항목을_등록하면_다시_켜지_않아도_시작한다() {
    let toss = FakeToss::start().await;
    let ai = FakeAi::start().await;
    let dir = tempfile::tempdir().unwrap();
    register_broker(dir.path(), &toss).await;
    let before = toss.total_requests().await;
    let app = start_app(
        settings(dir.path(), &[]),
        wiring_both(&toss, &ai, TestClock::at(kst(10, 0, 0))),
    )
    .await;
    let addr = app.local_addr();
    let mut ws = connect(addr, 0).await;
    let notice = wait_setup(&mut ws, SETUP_REQUIRED, STEP)
        .await
        .expect("AI 키가 없으니 설정이 필요하다고 알린다");
    assert_eq!(notice["missing"], json!([ITEM_AI]));
    assert_eq!(toss.total_requests().await, before);

    // 화면에서 마지막 항목(AI 키)을 등록한다.
    let reply = post_json(addr, SETUP_KEYS_PATH, &ai_key_body(AI_KEY)).await;
    assert_registered(&reply);

    let notice = wait_setup(&mut ws, SETUP_STARTED, SOON)
        .await
        .expect("다 등록되면 바로 시작하고 실시간 연결로 알린다");
    assert_eq!(notice["missing"], json!([]));
    assert_eq!(notice["demo"], json!(false));
    wait_feed_state(&mut ws, FEED_LIVE).await;
    wait_price(&mut ws, "005930", "72000").await;
    let status = setup_status(addr).await;
    assert_eq!(status["ready"], json!(true));
    assert_eq!(status["started"], json!(true));
    stop(app).await;
}

// 검증: V31, V32
#[tokio::test]
async fn 마지막_필수_항목이_종목이어도_등록하면_바로_시작한다() {
    let toss = FakeToss::start().await;
    let dir = tempfile::tempdir().unwrap();
    register_keys(dir.path(), &toss).await;
    let before = toss.total_requests().await;
    let app = start_app(
        settings_without_symbols(dir.path(), &[]),
        wiring(Some(&toss), TestClock::at(kst(10, 0, 0))),
    )
    .await;
    let addr = app.local_addr();
    let mut ws = connect(addr, 0).await;
    wait_setup(&mut ws, SETUP_REQUIRED, STEP)
        .await
        .expect("종목이 없으니 설정이 필요하다고 알린다");
    assert_eq!(toss.total_requests().await, before);

    let reply = post_json(
        addr,
        SETUP_SYMBOLS_PATH,
        &symbols_body(&["035720", "005930"]),
    )
    .await;
    assert_registered(&reply);
    wait_setup(&mut ws, SETUP_STARTED, SOON)
        .await
        .expect("종목이 등록되면 바로 시작한다");
    let quotes = wait_price(&mut ws, "035720", "72000").await;
    assert_eq!(price_of(&quotes, "005930").as_deref(), Some("72000"));
    let asked = asked_symbols(&toss.requests(PRICES_PATH).await[0]);
    assert_eq!(asked, ["035720", "005930"], "등록한 종목을 지켜본다");
    stop(app).await;
}

// ───────────── 규칙 6: 시작한 뒤 ─────────────

// 검증: C16, V32
#[tokio::test]
async fn 시작한_뒤_증권사_키를_다시_등록하면_새_키로_이어_간다() {
    let toss = FakeToss::start().await;
    let ai = FakeAi::start().await;
    let dir = tempfile::tempdir().unwrap();
    register_broker(dir.path(), &toss).await;
    register_ai(dir.path(), &ai).await;
    let app = start_app(
        settings(dir.path(), &[]),
        wiring_both(&toss, &ai, TestClock::at(kst(10, 0, 0))),
    )
    .await;
    let addr = app.local_addr();
    let mut ws = connect(addr, 0).await;
    wait_price(&mut ws, "005930", "72000").await;
    assert_eq!(token_requests_with(&toss, OTHER_ID).await, 0);

    let reply = post_json(addr, SETUP_KEYS_PATH, &broker_body(OTHER_ID, OTHER_SECRET)).await;
    assert_registered(&reply);
    assert_no_values(&reply, &[OTHER_ID, OTHER_SECRET]);

    // 확인할 때 한 번, 시세 흐름이 새 키로 토큰을 다시 받을 때 한 번.
    let deadline = tokio::time::Instant::now() + STEP;
    while token_requests_with(&toss, OTHER_ID).await < 2 {
        assert!(
            tokio::time::Instant::now() < deadline,
            "시세 흐름이 새 키로 토큰을 다시 받아야 한다"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    // 멈추지 않고 이어진다.
    toss.set_price("72400");
    wait_price(&mut ws, "005930", "72400").await;
    let status = setup_status(addr).await;
    assert_eq!(status["started"], json!(true));
    stop(app).await;
}

// 검증: C16, V31
#[tokio::test]
async fn 시작한_뒤_키_파일이_지워지면_멈추고_다시_등록하면_이어_간다() {
    let toss = FakeToss::start().await;
    let ai = FakeAi::start().await;
    let dir = tempfile::tempdir().unwrap();
    register_broker(dir.path(), &toss).await;
    register_ai(dir.path(), &ai).await;
    let app = start_app(
        settings(dir.path(), &[]),
        wiring_both(&toss, &ai, TestClock::at(kst(10, 0, 0))),
    )
    .await;
    let addr = app.local_addr();
    let mut ws = connect(addr, 0).await;
    wait_price(&mut ws, "005930", "72000").await;

    // 증권사 키와 AI API 키가 든 파일이 지워진다.
    std::fs::remove_file(dir.path().join(pi_secrets::ENV_FILE)).unwrap();
    let notice = wait_setup(&mut ws, SETUP_REQUIRED, STEP)
        .await
        .expect("필수 항목이 빠지면 멈추고 설정이 필요하다고 알린다");
    assert_eq!(notice["missing"], json!([ITEM_BROKER, ITEM_AI]));
    assert_eq!(
        latest(addr, KIND_FEED_STATUS).await["state"],
        json!(FEED_SETUP_REQUIRED),
        "멈춘 뒤에 연결한 화면이 증권사 시세라고 보면 안 된다"
    );
    let status = setup_status(addr).await;
    assert_eq!(status["ready"], json!(false));
    assert_eq!(status["started"], json!(false));
    assert!(!registered(&status, ITEM_BROKER));

    // 멈췄다: 더는 증권사를 부르지 않는다.
    tokio::time::sleep(Duration::from_millis(100)).await;
    let stopped_at = toss.total_requests().await;
    tokio::time::sleep(QUIET).await;
    assert_eq!(toss.total_requests().await, stopped_at);

    // 화면에서 다시 등록하면 다시 켜지 않아도 이어 간다.
    assert_registered(
        &post_json(
            addr,
            SETUP_KEYS_PATH,
            &broker_body(CLIENT_ID, CLIENT_SECRET),
        )
        .await,
    );
    assert_registered(&post_json(addr, SETUP_KEYS_PATH, &ai_key_body(AI_KEY)).await);
    wait_setup(&mut ws, SETUP_STARTED, SOON)
        .await
        .expect("다시 다 등록되면 바로 시작한다");
    toss.set_price("72600");
    wait_price(&mut ws, "005930", "72600").await;
    stop(app).await;
}

// ───────────── 규칙 8: 시연 모드 ─────────────

// 검증: V32
#[tokio::test]
async fn 시연_모드는_설정_없이_시작하고_설정_상태에_시연이라고_나온다() {
    let toss = FakeToss::start().await;
    let ai = FakeAi::start().await;
    let dir = tempfile::tempdir().unwrap();
    let app = start_app(
        settings(dir.path(), &["--demo"]),
        wiring_both(&toss, &ai, TestClock::at(kst(10, 0, 0))),
    )
    .await;
    let addr = app.local_addr();

    let status = setup_status(addr).await;
    assert_eq!(status["demo"], json!(true), "{status}");
    assert_eq!(status["started"], json!(true), "설정 없이 바로 시작한다");
    assert_eq!(status["ready"], json!(false), "키는 등록되지 않았다");
    assert!(!registered(&status, ITEM_BROKER) && !registered(&status, ITEM_AI));

    let notice = latest(addr, KIND_SETUP).await;
    assert_eq!(notice["state"], json!(SETUP_STARTED), "{notice}");
    assert_eq!(
        notice["demo"],
        json!(true),
        "화면이 시연이라고 표시할 수 있다"
    );
    assert_eq!(
        latest(addr, KIND_FEED_STATUS).await["state"],
        json!(FEED_DEMO)
    );
    let quotes = latest(addr, KIND_QUOTES).await;
    assert_eq!(quotes["demo"], json!(true));
    assert!(price_of(&quotes, "005930").is_some());
    assert_eq!(
        (toss.total_requests().await, ai.total_requests().await),
        (0, 0),
        "시연 모드는 바깥으로 요청을 보내지 않는다"
    );
    stop(app).await;
}

// ───────────── 지켜볼 종목 등록 ─────────────

fn saved_watchlist(home: &Path) -> Value {
    let text = std::fs::read_to_string(home.join(WATCHLIST_FILE))
        .expect("종목 파일이 보관 폴더에 있어야 한다");
    serde_json::from_str(&text).expect("종목 파일은 JSON이다")
}

// 검증: V31
#[tokio::test]
async fn 종목을_등록하면_보관_폴더에_저장되고_다시_켜도_남는다() {
    let toss = FakeToss::start().await;
    let dir = tempfile::tempdir().unwrap();
    let app = start_app(
        settings_without_symbols(dir.path(), &[]),
        wiring(Some(&toss), TestClock::at(kst(10, 0, 0))),
    )
    .await;
    let addr = app.local_addr();
    let status = setup_status(addr).await;
    assert!(!registered(&status, ITEM_SYMBOLS));
    assert_eq!(status["symbols"], json!([]));
    assert_eq!(status["symbols_source"], json!("none"));

    let reply = post_json(
        addr,
        SETUP_SYMBOLS_PATH,
        &symbols_body(&["005930", "BRK.B"]),
    )
    .await;
    assert_registered(&reply);
    let status = setup_status(addr).await;
    assert!(registered(&status, ITEM_SYMBOLS), "{status}");
    assert_eq!(status["symbols"], json!(["005930", "BRK.B"]));
    assert_eq!(status["symbols_source"], json!("saved"));
    assert_eq!(status["ready"], json!(false), "키가 아직 없다");
    assert_eq!(
        saved_watchlist(dir.path()),
        json!({ "symbols": ["005930", "BRK.B"] })
    );
    assert_eq!(
        toss.total_requests().await,
        0,
        "종목 등록은 바깥으로 나가지 않는다"
    );

    // 다시 등록하면 바꿔 쓴다. 임시 파일은 남지 않는다.
    let reply = post_json(addr, SETUP_SYMBOLS_PATH, &symbols_body(&["000660"])).await;
    assert_registered(&reply);
    assert_eq!(
        saved_watchlist(dir.path()),
        json!({ "symbols": ["000660"] })
    );
    stop(app).await;
    // 6차: 옛 auth.json은 더 만들지 않는다. CLI 작업 폴더 · 고른 AI 연결 파일은 있을 수 있다.
    let known = [
        pi_secrets::ENV_FILE,
        WATCHLIST_FILE,
        price_inha::CLI_WORK_DIR,
        price_inha::AI_CHOICE_FILE,
    ];
    for name in home_files(dir.path()) {
        assert!(
            known.contains(&name.as_str()) || name.starts_with(price_inha::DB_FILE),
            "보관 폴더에 모르는 파일(임시 파일)이 남았다: {name}"
        );
    }

    // 종목을 주지 않고 다시 켜면 저장된 것을 쓴다.
    let app = start_app(
        settings_without_symbols(dir.path(), &[]),
        wiring(Some(&toss), TestClock::at(kst(10, 0, 0))),
    )
    .await;
    let status = setup_status(app.local_addr()).await;
    assert!(registered(&status, ITEM_SYMBOLS));
    assert_eq!(status["symbols"], json!(["000660"]));
    assert_eq!(status["symbols_source"], json!("saved"));
    stop(app).await;
}

// 검증: V31, V32
#[tokio::test]
async fn 인자로_준_종목이_먼저이고_저장된_종목을_덮지_않는다() {
    let toss = FakeToss::start().await;
    let dir = tempfile::tempdir().unwrap();
    register_keys(dir.path(), &toss).await;
    std::fs::write(
        dir.path().join(WATCHLIST_FILE),
        json!({ "symbols": ["000660"] }).to_string(),
    )
    .unwrap();
    let app = start_app(
        settings_without_symbols(dir.path(), &["--symbols", "035720"]),
        wiring(Some(&toss), TestClock::at(kst(10, 0, 0))),
    )
    .await;
    let addr = app.local_addr();
    let mut ws = connect(addr, 0).await;
    let quotes = wait_price(&mut ws, "035720", "72000").await;
    assert!(
        price_of(&quotes, "000660").is_none(),
        "이 실행 동안에는 인자로 준 종목만 쓴다"
    );
    let status = setup_status(addr).await;
    assert_eq!(status["symbols"], json!(["035720"]));
    assert_eq!(status["symbols_source"], json!("argument"));

    // 인자로 준 동안에는 화면에서 바꾸지 못한다(쓰는 것과 저장된 것이 어긋나지 않게).
    let reply = post_json(addr, SETUP_SYMBOLS_PATH, &symbols_body(&["005930"])).await;
    assert_refused(&reply, 409, "fixed_by_argument");
    stop(app).await;
    assert_eq!(
        saved_watchlist(dir.path()),
        json!({ "symbols": ["000660"] }),
        "저장된 종목은 그대로다"
    );
    for request in toss.requests(PRICES_PATH).await {
        assert_eq!(asked_symbols(&request), ["035720"]);
    }
}

// 검증: V31
#[tokio::test]
async fn 종목으로_볼_수_없는_입력은_저장하지_않는다() {
    let dir = tempfile::tempdir().unwrap();
    let app = start_app(
        settings_without_symbols(dir.path(), &[]),
        wiring(None, TestClock::at(kst(10, 0, 0))),
    )
    .await;
    let addr = app.local_addr();
    let cookie = login(addr).await;
    let origin = origin_of(addr);
    let too_long = "A".repeat(33);
    let bad_bodies = [
        "".to_string(),
        "not json".to_string(),
        "{}".to_string(),
        symbols_body(&[]).to_string(),
        json!({ "symbols": "005930" }).to_string(),
        json!({ "symbols": [5930] }).to_string(),
        symbols_body(&["005930", ""]).to_string(),
        symbols_body(&["삼성전자"]).to_string(),
        symbols_body(&["005930&x=1"]).to_string(),
        symbols_body(&["AAPL MSFT"]).to_string(),
        symbols_body(&["a/b"]).to_string(),
        symbols_body(&[&too_long]).to_string(),
    ];
    for body in &bad_bodies {
        let reply = post_raw(
            addr,
            SETUP_SYMBOLS_PATH,
            Some(&cookie),
            Some(&origin),
            body.clone(),
        )
        .await;
        assert_eq!(reply.status, 400, "거부해야 하는 입력: {body}");
        assert_eq!(reply.json(), json!({ "error": "invalid_input" }), "{body}");
    }
    assert!(!registered(&setup_status(addr).await, ITEM_SYMBOLS));
    stop(app).await;
    assert!(!dir.path().join(WATCHLIST_FILE).exists());
}

// ───────────── 규칙 7: 화면 ─────────────

fn page() -> String {
    // 11차(Vite 이전): 스크립트가 묶음으로 나가므로 빌드한 화면과 그 묶음에서 찾는다.
    crate::screen_built::built_page("index.html")
}

// 검증: C3, V46, D11
#[test]
fn 화면에_설정_화면이_있다() {
    let page = page();
    // 상태 경로와 등록 경로를 부른다.
    for path in [SETUP_PATH, SETUP_KEYS_PATH, SETUP_SYMBOLS_PATH] {
        assert!(
            page.contains(&format!("\"{path}\"")),
            "화면이 {path}를 불러야 한다"
        );
    }
    // 항목별 입력 칸: 등록할 항목 이름과 칸 이름을 안다.
    for name in [
        KEY_BROKER,
        KEY_AI_API_KEY,
        "client_id",
        "client_secret",
        "api_key",
        "symbols",
    ] {
        assert!(page.contains(name), "화면이 {name} 칸을 알아야 한다");
    }
    // 키 입력 칸은 가려진 칸이다(증권사 둘, AI API 키 하나). 6차: 구독 토큰 칸은 없다(`round6/`).
    let hidden_inputs = page.matches("type=\"password\"").count();
    assert!(
        hidden_inputs >= 3,
        "키 입력 칸은 모두 가려진 칸이어야 한다: {hidden_inputs}개"
    );
    // 키 칸이 브라우저에 저장되지 않게 하는 속성은 4차(참고 5)에서 `autocomplete="new-password"`로
    // 바뀌었다. `round4/`의 `키_입력_칸은_브라우저가_저장을_권하지_않게_한다`가 본다.
    let buttons = page.matches("<button").count();
    assert!(buttons >= 3, "항목마다 등록 단추가 있다: {buttons}개");
    // 등록한 뒤 칸을 비운다.
    assert!(page.contains(".value = \"\""), "등록한 뒤 입력 칸을 비운다");
    // 설정 알림과 실패 까닭을 글자로 보여 줄 줄 안다.
    for word in [
        format!("\"{KIND_SETUP}\""),
        format!("\"{SETUP_REQUIRED}\""),
        format!("\"{SETUP_STARTED}\""),
        format!("\"{FEED_SETUP_REQUIRED}\""),
        "\"key_rejected\"".to_string(),
        "\"unreachable\"".to_string(),
        "\"invalid_input\"".to_string(),
        "\"too_many_attempts\"".to_string(),
    ] {
        assert!(page.contains(&word), "화면이 {word}를 알아야 한다");
    }
    assert!(page.contains("시연"), "시연 모드를 화면에 표시한다");
}

// 검증: V46, D11
#[test]
fn 화면은_키_값을_브라우저_저장소나_주소에_남기지_않는다() {
    let page = page();
    for banned in [
        "localStorage",
        "sessionStorage",
        "indexedDB",
        "document.cookie",
        "pushState",
        "replaceState",
        "location.href",
        "location.assign",
        "location.replace",
        "location.search",
        "location.hash",
        "URLSearchParams",
        "method=\"get\"",
        "console.log",
    ] {
        assert!(!page.contains(banned), "화면에 {banned}가 있으면 안 된다");
    }
    // 키는 본문으로만 보낸다: 등록 경로 뒤에 쿼리를 붙이지 않는다.
    for path in [SETUP_KEYS_PATH, SETUP_SYMBOLS_PATH] {
        assert!(
            !page.contains(&format!("{path}?")),
            "{path}에 쿼리를 붙이지 않는다"
        );
    }
    for outside in ["http://", "https://", "src=\"//", "href=\"//"] {
        assert!(
            !page.contains(outside),
            "바깥 주소가 있으면 안 된다: {outside}"
        );
    }
}

// 검증: V31
#[test]
fn 화면은_거절된_종목과_요청_거절_상태를_보여_줄_줄_안다() {
    let page = page();
    for word in [FEED_REQUEST_REJECTED, KIND_SYMBOLS_REJECTED] {
        assert!(
            page.contains(&format!("\"{word}\"")),
            "화면이 {word}를 알아야 한다"
        );
    }
}
