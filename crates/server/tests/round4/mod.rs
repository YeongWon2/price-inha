//! 4차 작업: 원본 대조(D21 보안 응답 헤더 · C9 이벤트 종류 이름)와 3차 AI 검토의 참고 1 · 2 · 3 · 5 · 6 · 7.
//! (참고 4는 T17로 미뤘다. D17 살아 있음 신호와 참고 8은 `pi-gate`의 `round4/`가 본다.)
//!
//! 근거: `docs/dev/modules/gate.md` "4차 작업", `docs/dev/conformance.md` 4장 D21 · C9.
//! 원본에서 직접 확인한 것(Hermes, 읽기만 했다):
//! - D21: `gateway/platforms/api_server.py:633-640`(`_SECURITY_HEADERS`의 이름 · 값),
//!   `:681-687`(모든 응답 · 오류 응답에 `setdefault`로 붙인다).
//! - C9: `gateway/hosted_rooms.py:27`(64자), `:48`(`^[a-z][a-z0-9_.-]*$`).
//!
//! 화면(참고 1 · 5)은 브라우저 없이 확인할 수 있는 것만 본다: 화면 파일에 필요한 요소와 코드가 있는가.
//! 화면 요소의 이름(4차에서 정한 것): 본 화면에서 설정을 여는 단추 `open-setup`, 설정 화면에서 본 화면으로
//! 돌아가는 단추 `close-setup`, "거절한 종목" 옆의 `fix-symbols`, "증권사가 키를 받지 않음" 옆의 `fix-broker`.
//!
//! 시간에 기대는 테스트는 실제 시간으로 재되 간격을 작게 잡고 범위에 여유를 둔다.

use super::setup::{assert_registered, broker_body, latest, post_json, setup_status, symbols_body};
use super::*;
use price_inha::{
    DEFAULT_SYMBOLS_ATTEMPTS, DEFAULT_SYMBOLS_WINDOW, ENV_SYMBOLS_ATTEMPTS, ENV_SYMBOLS_WINDOW_MS,
    FEED_REQUEST_REJECTED, KIND_SETUP, KIND_SYMBOLS_REJECTED, KeyRequest, SETUP_KEYS_PATH,
    SETUP_PATH, SETUP_REQUIRED, SETUP_STARTED, SETUP_SYMBOLS_PATH, SetupRefusal, WATCHLIST_FILE,
    load_watchlist, parse_symbols,
};

// ───────────── 도우미 ─────────────

/// 물러서기를 길게(30초) 잡는다: 실패한 뒤에는 등록이 깨우지 않으면 다시 시도하지 않는다.
fn slow(mut settings: Settings) -> Settings {
    settings.retry_initial = Duration::from_secs(30);
    settings.retry_max = Duration::from_secs(30);
    settings
}

/// 종목 등록 시도 제한을 테스트가 줄인다.
fn symbols_limited(mut settings: Settings, attempts: u32, window: Duration) -> Settings {
    settings.symbols_attempts = attempts;
    settings.symbols_window = window;
    settings
}

/// 보관 폴더에 지켜볼 종목을 미리 둔다.
fn save_symbols(home: &Path, symbols: &[&str]) {
    std::fs::write(
        home.join(WATCHLIST_FILE),
        json!({ "symbols": symbols }).to_string(),
    )
    .unwrap();
}

fn saved_symbols(home: &Path) -> Value {
    let text = std::fs::read_to_string(home.join(WATCHLIST_FILE)).expect("종목 파일이 있어야 한다");
    serde_json::from_str::<Value>(&text).unwrap()["symbols"].clone()
}

/// 그 경로의 요청이 `count`번이 될 때까지 기다린다.
async fn wait_requests(toss: &FakeToss, wanted: &str, count: usize) {
    let deadline = tokio::time::Instant::now() + STEP;
    while toss.requests(wanted).await.len() < count {
        assert!(
            tokio::time::Instant::now() < deadline,
            "{wanted} 요청이 {count}번은 나가야 한다"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

/// 그 상태의 설정 알림이 올 때까지 기다린다.
async fn wait_setup(ws: &mut Ws, state: &str, wait: Duration) -> Option<Value> {
    find_frame(ws, wait, |frame| {
        transient(frame, KIND_SETUP).filter(|notice| notice["state"] == json!(state))
    })
    .await
}

/// 실패 응답은 상태와 까닭뿐이다.
fn assert_refused(reply: &super::setup::HttpReply, status: u16, reason: &str) {
    assert_eq!(reply.status, status, "{reason}");
    assert_eq!(reply.json(), json!({ "error": reason }));
}

/// 이벤트 종류 이름 규칙(저장소 · 원본과 같다): `^[a-z][a-z0-9_.-]*$`, 64자 이하.
fn kind_follows_rule(kind: &str) -> bool {
    let mut chars = kind.chars();
    chars.next().is_some_and(|c| c.is_ascii_lowercase())
        && chars.all(|c| matches!(c, 'a'..='z' | '0'..='9' | '_' | '.' | '-'))
        && kind.chars().count() <= pi_store::EVENT_KIND_MAX_CHARS
}

fn page() -> String {
    let page_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("web/index.html");
    std::fs::read_to_string(&page_path).expect("web/index.html이 있어야 한다")
}

/// 화면에서 `<section id="...">`의 시작과 끝(닫는 태그 뒤) 자리.
fn section_range(page: &str, id: &str) -> std::ops::Range<usize> {
    let start = page
        .find(&format!("<section id=\"{id}\""))
        .unwrap_or_else(|| panic!("화면에 {id} 구역이 있어야 한다"));
    let end = page[start..]
        .find("</section>")
        .map(|offset| start + offset + "</section>".len())
        .unwrap_or_else(|| panic!("{id} 구역이 닫혀야 한다"));
    start..end
}

/// 그 id를 가진 요소의 자리.
fn element_at(page: &str, id: &str) -> usize {
    page.find(&format!("id=\"{id}\""))
        .unwrap_or_else(|| panic!("화면에 id=\"{id}\" 요소가 있어야 한다"))
}

/// 화면의 스크립트 부분.
fn script_of(page: &str) -> &str {
    let start = page.find("<script").expect("화면에 스크립트가 있어야 한다");
    &page[start..]
}

/// 응답에 원본과 같은 보안 헤더가 한 번씩 붙어 있다.
fn assert_security_headers(reply: &reqwest::Response, what: &str) {
    for (name, value) in [
        ("x-content-type-options", "nosniff"),
        ("x-frame-options", "DENY"),
        ("referrer-policy", "no-referrer"),
    ] {
        let values: Vec<&str> = reply
            .headers()
            .get_all(name)
            .iter()
            .map(|v| v.to_str().unwrap())
            .collect();
        assert_eq!(
            values,
            [value],
            "{what}(HTTP {}): {name}가 원본 값으로 한 번 붙어야 한다",
            reply.status()
        );
    }
}

async fn send(request: reqwest::RequestBuilder) -> reqwest::Response {
    tokio::time::timeout(STEP, request.send())
        .await
        .expect("응답이 시간 안에 와야 한다")
        .expect("요청이 닿아야 한다")
}

// ───────────── D21 보안 응답 헤더 ─────────────

// 검증: D11, V46
#[tokio::test]
async fn 화면과_api의_모든_응답에_보안_헤더가_붙고_화면의_csp는_그대로다() {
    let dir = tempfile::tempdir().unwrap();
    let app = start_app(
        settings(dir.path(), &["--demo"]),
        wiring(None, TestClock::at(kst(10, 0, 0))),
    )
    .await;
    let addr = app.local_addr();
    let origin = origin_of(addr);
    let client = reqwest::Client::new();

    let page = send(client.get(format!("{origin}/"))).await;
    assert_eq!(page.status().as_u16(), 200);
    assert_security_headers(&page, "화면");
    let csp: Vec<&str> = page
        .headers()
        .get_all("content-security-policy")
        .iter()
        .map(|v| v.to_str().unwrap())
        .collect();
    assert_eq!(csp.len(), 1, "화면의 CSP는 한 번이다: {csp:?}");
    assert!(
        csp[0].contains(&format!(
            "connect-src 'self' ws://127.0.0.1:{}",
            addr.port()
        )) && csp[0].contains("frame-ancestors 'none'")
            && csp[0].contains("default-src 'none'"),
        "화면에는 지금의 CSP를 그대로 둔다: {}",
        csp[0]
    );

    let no_session = send(client.get(format!("{origin}{SETUP_PATH}"))).await;
    assert_eq!(no_session.status().as_u16(), 401);
    assert_security_headers(&no_session, "세션 없는 설정 상태(401)");

    let cookie = login(addr).await;
    let status = send(
        client
            .get(format!("{origin}{SETUP_PATH}"))
            .header("Cookie", &cookie),
    )
    .await;
    assert_eq!(status.status().as_u16(), 200);
    assert_security_headers(&status, "설정 상태");

    let other_origin = send(
        client
            .post(format!("{origin}{SETUP_SYMBOLS_PATH}"))
            .header("Cookie", &cookie)
            .header("Origin", "http://localhost:1")
            .header("Content-Type", "application/json")
            .body(symbols_body(&["005930"]).to_string()),
    )
    .await;
    assert!(
        other_origin.status().is_client_error(),
        "{}",
        other_origin.status()
    );
    assert_security_headers(&other_origin, "다른 출처의 등록(4xx)");

    let missing = send(
        client
            .get(format!("{origin}/no/such/path"))
            .header("Cookie", &cookie),
    )
    .await;
    assert!(missing.status().is_client_error(), "{}", missing.status());
    assert_security_headers(&missing, "없는 경로");

    let bad_input = send(
        client
            .post(format!("{origin}{SETUP_KEYS_PATH}"))
            .header("Cookie", &cookie)
            .header("Origin", &origin)
            .header("Content-Type", "application/json")
            .body("not json"),
    )
    .await;
    assert_eq!(bad_input.status().as_u16(), 400);
    assert_security_headers(&bad_input, "잘못된 입력(400)");
    stop(app).await;
}

// ───────────── C9 이벤트 종류 이름 ─────────────

// 검증: D11, V32
#[tokio::test]
async fn 이_프로그램이_내는_이벤트_종류는_이름_규칙을_지킨다() {
    for kind in [
        KIND_FEED_STATUS,
        KIND_QUOTES,
        KIND_QUOTES_STALE,
        KIND_SYMBOLS_REJECTED,
        KIND_SETUP,
    ] {
        assert!(kind_follows_rule(kind), "{kind}는 이름 규칙에 맞아야 한다");
    }

    // 실제로 실시간 연결로 나가는 프레임의 종류도 규칙을 지킨다.
    let dir = tempfile::tempdir().unwrap();
    let app = start_app(
        settings(dir.path(), &["--demo"]),
        wiring(None, TestClock::at(kst(10, 0, 0))),
    )
    .await;
    let mut ws = connect(app.local_addr(), 0).await;
    let mut kinds = Vec::new();
    let none: Option<()> = find_frame(&mut ws, Duration::from_millis(800), |frame| {
        kinds.push(frame["kind"].as_str().unwrap_or("").to_string());
        None
    })
    .await;
    assert!(none.is_none());
    assert!(!kinds.is_empty(), "시연 모드는 프레임을 보낸다");
    for kind in &kinds {
        assert!(
            kind_follows_rule(kind),
            "{kind:?}는 이름 규칙에 맞아야 한다"
        );
    }
    stop(app).await;
}

// ───────────── 참고 1: 시작한 뒤에도 화면에서 설정을 고친다 ─────────────

// 검증: C3, C16, V31
#[test]
fn 본_화면에서_설정_화면을_다시_열_수_있고_고치러_가는_길이_있다() {
    let page = page();
    let setup_view = section_range(&page, "setup-view");
    let main_view = section_range(&page, "main-view");
    let open = element_at(&page, "open-setup");
    assert!(
        !setup_view.contains(&open),
        "설정을 여는 단추는 설정 화면 밖(본 화면 쪽)에 있다"
    );
    let close = element_at(&page, "close-setup");
    assert!(
        setup_view.contains(&close),
        "설정 화면에서 본 화면으로 돌아가는 단추가 있다"
    );
    let fix_symbols = element_at(&page, "fix-symbols");
    assert!(
        main_view.contains(&fix_symbols),
        "거절한 종목 옆(본 화면)에 종목을 고치러 가는 길이 있다"
    );
    let fix_broker = element_at(&page, "fix-broker");
    assert!(
        !setup_view.contains(&fix_broker),
        "증권사가 키를 받지 않음 옆(설정 화면 밖)에 키를 고치러 가는 길이 있다"
    );
    let script = script_of(&page);
    for id in ["open-setup", "close-setup", "fix-symbols", "fix-broker"] {
        assert!(
            script.contains(&format!("\"{id}\"")) || script.contains(&format!("'{id}'")),
            "스크립트가 {id}의 동작을 잇는다"
        );
    }
}

// 검증: C3, C16, V31
#[tokio::test]
async fn 시작한_뒤에도_설정_상태와_등록_경로가_동작하고_다시_등록하면_흐름이_이어진다() {
    let toss = FakeToss::start().await;
    let ai = FakeAi::start().await;
    let dir = tempfile::tempdir().unwrap();
    register_broker(dir.path(), &toss).await;
    register_ai(dir.path(), &ai).await;
    save_symbols(dir.path(), &["005930"]);
    let app = start_app(
        settings_without_symbols(dir.path(), &[]),
        wiring_both(&toss, &ai, TestClock::at(kst(10, 0, 0))),
    )
    .await;
    let addr = app.local_addr();
    let mut ws = connect(addr, 0).await;
    wait_price(&mut ws, "005930", "72000").await;

    let status = setup_status(addr).await;
    assert_eq!(status["started"], json!(true));
    assert_eq!(status["ready"], json!(true));

    // 시작한 뒤에 종목을 다시 등록한다.
    assert_registered(
        &post_json(
            addr,
            SETUP_SYMBOLS_PATH,
            &symbols_body(&["005930", "000660"]),
        )
        .await,
    );
    wait_price(&mut ws, "000660", "72000").await;
    let status = setup_status(addr).await;
    assert_eq!(status["symbols"], json!(["005930", "000660"]));
    assert_eq!(status["symbols_source"], json!("saved"));
    assert_eq!(status["started"], json!(true));

    // 시작한 뒤에 키를 다시 등록해도 흐름이 이어진다.
    assert_registered(
        &post_json(
            addr,
            SETUP_KEYS_PATH,
            &broker_body(CLIENT_ID, CLIENT_SECRET),
        )
        .await,
    );
    toss.set_price("72100");
    wait_price(&mut ws, "000660", "72100").await;
    assert_eq!(
        latest(addr, KIND_SETUP).await["state"],
        json!(SETUP_STARTED),
        "다시 등록해도 설정 화면으로 돌아가지 않는다"
    );
    stop(app).await;
}

// ───────────── 참고 2: 거절을 봤고 시세가 0건이면 바로 요청 거절 ─────────────

// 검증: V31, V32
#[tokio::test]
async fn 종목이_많고_전부_거절되면_가려내는_중에도_바로_요청_거절이라고_알린다() {
    let toss = FakeToss::start().await;
    let dir = tempfile::tempdir().unwrap();
    register_keys(dir.path(), &toss).await;
    let codes: Vec<String> = (0..64).map(|n| format!("BAD{n:02}")).collect();
    toss.unknown_symbols
        .lock()
        .unwrap()
        .extend(codes.iter().cloned());
    let list = codes.join(",");
    // 한 번 실패하면 30초를 물러선다: 첫 받기 한 번의 결과로 알려야 한다.
    let app = start_app(
        slow(settings_without_symbols(dir.path(), &["--symbols", &list])),
        wiring(Some(&toss), TestClock::at(kst(10, 0, 0))),
    )
    .await;
    let addr = app.local_addr();
    let mut ws = connect(addr, 0).await;

    let states = feed_states_for(&mut ws, Duration::from_millis(2000)).await;
    assert_eq!(
        states.last().map(String::as_str),
        Some(FEED_REQUEST_REJECTED),
        "거절을 봤고 시세가 하나도 없으면 다 가려내기 전이라도 요청 거절이다: {states:?}"
    );
    for wrong in [FEED_LIVE, FEED_BROKER_UNREACHABLE, FEED_BROKER_REJECTED] {
        assert!(
            !states.iter().any(|state| state == wrong),
            "{wrong}가 아니다: {states:?}"
        );
    }
    let rejected = latest(addr, KIND_SYMBOLS_REJECTED).await;
    assert!(
        rejected["symbols"]
            .as_array()
            .is_some_and(|list| !list.is_empty()),
        "가려낸 거절 종목을 알린다: {rejected}"
    );
    assert_eq!(
        latest(addr, KIND_FEED_STATUS).await["state"],
        json!(FEED_REQUEST_REJECTED),
        "나중에 연결한 화면도 연결 중이 아니라 요청 거절을 본다"
    );
    stop(app).await;
}

// ───────────── 참고 3: 종목 등록도 아껴 깨운다 ─────────────

// 검증: V31
#[tokio::test]
async fn 같은_종목_목록을_다시_등록하면_흐름을_깨우지_않는다() {
    let toss = FakeToss::start().await;
    let dir = tempfile::tempdir().unwrap();
    register_keys(dir.path(), &toss).await;
    save_symbols(dir.path(), &["005930"]);
    toss.failing.store(true, Ordering::SeqCst);
    let app = start_app(
        slow(settings_without_symbols(dir.path(), &[])),
        wiring(Some(&toss), TestClock::at(kst(10, 0, 0))),
    )
    .await;
    let addr = app.local_addr();
    let mut ws = connect(addr, 0).await;
    wait_feed_state(&mut ws, FEED_BROKER_UNREACHABLE).await;
    toss.failing.store(false, Ordering::SeqCst);

    let before = toss.requests(PRICES_PATH).await.len();
    assert_registered(&post_json(addr, SETUP_SYMBOLS_PATH, &symbols_body(&["005930"])).await);
    tokio::time::sleep(Duration::from_millis(1500)).await;
    assert_eq!(
        toss.requests(PRICES_PATH).await.len(),
        before,
        "목록이 그대로면 물러서 있는 흐름을 깨우지 않는다"
    );

    // 목록이 바뀌면 깨운다.
    assert_registered(
        &post_json(
            addr,
            SETUP_SYMBOLS_PATH,
            &symbols_body(&["005930", "000660"]),
        )
        .await,
    );
    let price = find_frame(&mut ws, Duration::from_secs(3), |frame| {
        transient(frame, KIND_QUOTES).and_then(|quotes| price_of(&quotes, "000660"))
    })
    .await;
    assert_eq!(
        price.as_deref(),
        Some("72000"),
        "목록이 바뀌면 바로 다시 시도한다"
    );
    stop(app).await;
}

// 검증: V31, C3
#[tokio::test]
async fn 종목_등록을_정한_횟수보다_많이_하면_저장하지_않고_거부한다() {
    let dir = tempfile::tempdir().unwrap();
    let app = start_app(
        symbols_limited(
            settings_without_symbols(dir.path(), &[]),
            2,
            Duration::from_secs(60),
        ),
        wiring(None, TestClock::at(kst(10, 0, 0))),
    )
    .await;
    let addr = app.local_addr();

    // 입력 검사에서 걸린 것은 세지 않는다.
    for _ in 0..3 {
        let reply = post_json(addr, SETUP_SYMBOLS_PATH, &symbols_body(&[])).await;
        assert_refused(&reply, 400, "invalid_input");
    }
    assert_registered(&post_json(addr, SETUP_SYMBOLS_PATH, &symbols_body(&["005930"])).await);
    assert_registered(&post_json(addr, SETUP_SYMBOLS_PATH, &symbols_body(&["000660"])).await);
    let reply = post_json(addr, SETUP_SYMBOLS_PATH, &symbols_body(&["035720"])).await;
    assert_refused(&reply, 429, "too_many_attempts");

    assert_eq!(
        saved_symbols(dir.path()),
        json!(["000660"]),
        "거부된 등록은 저장하지 않는다"
    );
    assert_eq!(setup_status(addr).await["symbols"], json!(["000660"]));
    stop(app).await;
}

// 검증: V31, C3
#[tokio::test]
async fn 종목_등록_제한은_정한_시간이_지나면_풀린다() {
    let dir = tempfile::tempdir().unwrap();
    let window = Duration::from_millis(300);
    let app = start_app(
        symbols_limited(settings_without_symbols(dir.path(), &[]), 1, window),
        wiring(None, TestClock::at(kst(10, 0, 0))),
    )
    .await;
    let addr = app.local_addr();
    assert_registered(&post_json(addr, SETUP_SYMBOLS_PATH, &symbols_body(&["005930"])).await);
    let reply = post_json(addr, SETUP_SYMBOLS_PATH, &symbols_body(&["000660"])).await;
    assert_refused(&reply, 429, "too_many_attempts");
    tokio::time::sleep(window + Duration::from_millis(200)).await;
    assert_registered(&post_json(addr, SETUP_SYMBOLS_PATH, &symbols_body(&["000660"])).await);
    assert_eq!(saved_symbols(dir.path()), json!(["000660"]));
    stop(app).await;
}

// 검증: C3
#[test]
fn 종목_등록_시도_제한은_설정으로_준다() {
    let home = ["--home", "/tmp/pi-home"];
    let parse = |extra: &[&str]| {
        let mut list = home.to_vec();
        list.extend_from_slice(extra);
        Settings::parse(&args(&list), &no_env, None)
    };
    let defaults = parse(&[]).unwrap();
    assert_eq!(defaults.symbols_attempts, DEFAULT_SYMBOLS_ATTEMPTS);
    assert_eq!(defaults.symbols_window, DEFAULT_SYMBOLS_WINDOW);

    let given = parse(&["--symbols-attempts", "3", "--symbols-window-ms", "20000"])
        .expect("4차 설정은 인자로 준다");
    assert_eq!(given.symbols_attempts, 3);
    assert_eq!(given.symbols_window, Duration::from_millis(20_000));

    let env = |name: &str| -> Option<String> {
        let value = match name {
            n if n == ENV_HOME => "/srv/pi",
            n if n == ENV_SYMBOLS_ATTEMPTS => "9",
            n if n == ENV_SYMBOLS_WINDOW_MS => "45000",
            _ => return None,
        };
        Some(value.to_string())
    };
    let from_env = Settings::parse(&[], &env, None).unwrap();
    assert_eq!(from_env.symbols_attempts, 9);
    assert_eq!(from_env.symbols_window, Duration::from_millis(45_000));
    let mixed = Settings::parse(&args(&["--symbols-attempts", "2"]), &env, None)
        .expect("인자가 환경 변수를 이긴다");
    assert_eq!(mixed.symbols_attempts, 2);
    assert_eq!(mixed.symbols_window, Duration::from_millis(45_000));

    for name in ["--symbols-attempts", "--symbols-window-ms"] {
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

// ───────────── 참고 5: 키 입력 칸 ─────────────

// 검증: V46, C3
#[test]
fn 키_입력_칸은_브라우저가_저장을_권하지_않게_한다() {
    let page = page();
    let inputs: Vec<&str> = page
        .split("<input")
        .skip(1)
        .map(|rest| rest.split('>').next().unwrap_or(""))
        .collect();
    let hidden: Vec<&&str> = inputs
        .iter()
        .filter(|tag| tag.contains("type=\"password\""))
        .collect();
    assert!(
        hidden.len() >= 5,
        "키 입력 칸은 가려진 칸이다: {}개",
        hidden.len()
    );
    for tag in hidden {
        assert!(
            tag.contains("autocomplete=\"new-password\""),
            "가려진 칸은 autocomplete=\"new-password\"여야 한다: <input{tag}>"
        );
    }
}

// 검증: V46, C3
#[test]
fn 자료_키는_아직_등록할_수_없다고_안내한다() {
    let page = page();
    let data = section_range(&page, "setup-view");
    let guide = page
        .find("아직 등록할 수 없음")
        .expect("자료 키는 \"아직 등록할 수 없음\"으로 안내한다");
    assert!(data.contains(&guide), "안내는 설정 화면에 있다");
}

// ───────────── 참고 6: 받은 키 값은 다 쓰면 바로 지운다 ─────────────

// 검증: C3, V46
#[test]
fn 키_등록_본문은_읽은_뒤_0으로_덮는다() {
    let mut body = broker_body(CLIENT_ID, CLIENT_SECRET)
        .to_string()
        .into_bytes();
    let request = KeyRequest::parse_wiping(&mut body);
    assert!(
        matches!(&request, Ok(parsed) if parsed.kind == KeyKind::Broker),
        "맞는 본문은 읽힌다"
    );
    assert!(!body.is_empty());
    assert!(
        body.iter().all(|byte| *byte == 0),
        "읽은 본문은 0으로 덮인다"
    );
    drop(request);

    let short = json!({ "item": "broker", "fields": { "client_id": "x", "client_secret": "y" } });
    for bad in [
        b"not json".to_vec(),
        json!({ "item": "nope", "fields": {} })
            .to_string()
            .into_bytes(),
        short.to_string().into_bytes(),
        format!("{{\"item\":\"broker\",\"fields\":{{\"client_id\":\"{CLIENT_ID}\"}}}}")
            .into_bytes(),
    ] {
        let mut body = bad;
        let refused = KeyRequest::parse_wiping(&mut body);
        assert!(matches!(refused, Err(SetupRefusal::InvalidInput)));
        assert!(
            body.iter().all(|byte| *byte == 0),
            "거부한 본문도 0으로 덮인다"
        );
    }
}

// ───────────── 참고 7: Retry-After는 다시 시작해도 기억한다 · 종목 글자의 `..` ─────────────

// 검증: V31, C16
#[tokio::test]
async fn 증권사가_준_기다림은_흐름이_다시_시작해도_지킨다() {
    let toss = FakeToss::start().await;
    let ai = FakeAi::start().await;
    let dir = tempfile::tempdir().unwrap();
    register_broker(dir.path(), &toss).await;
    register_ai(dir.path(), &ai).await;
    // 현재가가 429이고 `Retry-After: 6`이다.
    toss.rate_limited_secs.store(6, Ordering::SeqCst);
    let mut wiring = wiring_both(&toss, &ai, TestClock::at(kst(10, 0, 0)));
    wiring.upstreams.toss.rate_limit_retries = 0;
    let app = start_app(fast_retry(settings(dir.path(), &[])), wiring).await;
    let addr = app.local_addr();
    let mut ws = connect(addr, 0).await;
    wait_requests(&toss, PRICES_PATH, 1).await;
    let limited_at = tokio::time::Instant::now();

    // 흐름이 멈췄다가(키 파일이 지워짐) 다시 등록되어 다시 시작한다.
    std::fs::remove_file(dir.path().join(pi_secrets::ENV_FILE)).unwrap();
    wait_setup(&mut ws, SETUP_REQUIRED, STEP)
        .await
        .expect("필수 항목이 빠지면 멈춘다");
    assert_registered(
        &post_json(
            addr,
            SETUP_KEYS_PATH,
            &broker_body(CLIENT_ID, CLIENT_SECRET),
        )
        .await,
    );
    assert_registered(
        &post_json(
            addr,
            SETUP_KEYS_PATH,
            &json!({ "item": "ai_api_key", "fields": { "api_key": AI_KEY } }),
        )
        .await,
    );
    wait_setup(&mut ws, SETUP_STARTED, Duration::from_secs(2))
        .await
        .expect("다시 다 등록되면 바로 시작한다");

    // 증권사가 기다리라고 한 6초(여유 1초를 뺀다)가 지나기 전에는 다시 묻지 않는다.
    let quiet_until = limited_at + Duration::from_secs(5);
    assert!(
        tokio::time::Instant::now() < quiet_until,
        "다시 시작한 것을 기다림 안에 봐야 한다"
    );
    while tokio::time::Instant::now() < quiet_until {
        assert_eq!(
            toss.requests(PRICES_PATH).await.len(),
            1,
            "흐름이 다시 시작해도 증권사가 준 기다림보다 빨리 다시 시도하지 않는다"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    // 기다림이 지나면 다시 시도한다(멈춰 버리지 않는다).
    toss.rate_limited_secs.store(0, Ordering::SeqCst);
    wait_price(&mut ws, "005930", "72000").await;
    stop(app).await;
}

// 검증: V31
#[tokio::test]
async fn 종목_글자에_점_두_개를_받지_않는다() {
    for list in [
        vec!["A..B"],
        vec![".."],
        vec!["..."],
        vec!["005930", "x..y"],
        vec!["..005930"],
    ] {
        let body = symbols_body(&list).to_string();
        assert!(
            matches!(
                parse_symbols(body.as_bytes()),
                Err(SetupRefusal::InvalidInput)
            ),
            "{list:?}"
        );
    }
    let single_dot = symbols_body(&["BRK.B"]).to_string();
    assert!(
        parse_symbols(single_dot.as_bytes()).is_ok(),
        "점 하나는 종목 글자다"
    );

    let parsed = Settings::parse(
        &args(&["--home", "/tmp/pi-home", "--symbols", "005930,A..B"]),
        &no_env,
        None,
    );
    assert_eq!(
        parsed,
        Err(SettingsError::InvalidValue { name: "--symbols" })
    );
    let env = |name: &str| (name == ENV_SYMBOLS).then(|| "x..y".to_string());
    let parsed = Settings::parse(&args(&["--home", "/tmp/pi-home"]), &env, None);
    assert_eq!(
        parsed,
        Err(SettingsError::InvalidValue { name: ENV_SYMBOLS })
    );

    let dir = tempfile::tempdir().unwrap();
    save_symbols(dir.path(), &["005930", "A..B"]);
    assert!(
        load_watchlist(dir.path()).is_err(),
        "저장된 파일에 있어도 받지 않는다"
    );

    let fresh = tempfile::tempdir().unwrap();
    let app = start_app(
        settings_without_symbols(fresh.path(), &[]),
        wiring(None, TestClock::at(kst(10, 0, 0))),
    )
    .await;
    let reply = post_json(
        app.local_addr(),
        SETUP_SYMBOLS_PATH,
        &symbols_body(&["A..B"]),
    )
    .await;
    assert_refused(&reply, 400, "invalid_input");
    stop(app).await;
    assert!(!fresh.path().join(WATCHLIST_FILE).exists());
}
