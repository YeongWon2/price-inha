//! 3차 작업 규칙 10~16: 2차 AI 검토 되돌림(시세 흐름).
//!
//! 근거: `docs/dev/modules/gate.md` "3차 작업 › 2차 AI 검토 되돌림에서 함께 고치는 것".
//! 규칙 10(종목이 없으면 증권사를 부르지 않는다)은 `setup/`의
//! `지켜볼_종목이_없으면_키가_있어도_증권사를_부르지_않는다`가 본다.
//!
//! 시간에 기대는 테스트는 2차처럼 실제 시간으로 재되 간격을 작게 잡고 범위에 여유를 둔다.

use super::setup::{assert_registered, broker_body, latest, post_json, symbols_body};
use super::*;
use price_inha::{
    FEED_REQUEST_REJECTED, KIND_SYMBOLS_REJECTED, SETUP_KEYS_PATH, SETUP_SYMBOLS_PATH,
    WATCHLIST_FILE,
};

/// 물러서 있는 동안 등록이 흐름을 깨우는지 볼 때 쓰는 긴 간격.
const LONG_RETRY: Duration = Duration::from_secs(30);
/// 등록한 뒤 다시 시도가 나가야 하는 시간(긴 간격을 기다리지 않는다).
const WOKEN: Duration = Duration::from_secs(3);

/// 실패하면 곧바로 긴 간격으로 물러서게 한다.
fn slow_retry(mut settings: Settings) -> Settings {
    settings.retry_initial = LONG_RETRY;
    settings.retry_max = LONG_RETRY;
    settings
}

/// 그 종목의 시세가 `wait` 안에 오는가.
async fn price_within(ws: &mut Ws, symbol: &str, wait: Duration) -> Option<String> {
    find_frame(ws, wait, |frame| {
        transient(frame, KIND_QUOTES).and_then(|quotes| price_of(&quotes, symbol))
    })
    .await
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

// ───────────── 규칙 11: 달력 받기 실패 ─────────────

// 검증: V31, N6
#[tokio::test]
async fn 달력을_받다가_인증에_실패하면_키를_받지_않는다고_알리고_물러선다() {
    let toss = FakeToss::start().await;
    let dir = tempfile::tempdir().unwrap();
    register_keys(dir.path(), &toss).await;
    // 시세는 주는데 달력은 401(`invalid-token`)이다.
    toss.calendar_status.store(401, Ordering::SeqCst);
    let app = start_app(
        fast_retry(settings(dir.path(), &[])),
        wiring(Some(&toss), TestClock::at(kst(10, 0, 0))),
    )
    .await;
    let mut ws = connect(app.local_addr(), 0).await;

    // 앞 구간: 달력만 30초마다 따로 두드리면 1번, 시세 간격(20ms)대로면 50번쯤이다.
    // 물러서면 0 · 0.1 · 0.3 · 0.7초쯤에 네댓 번이다.
    let states = feed_states_for(&mut ws, RETRY_WATCH_FIRST).await;
    assert_eq!(
        states.last().map(String::as_str),
        Some(FEED_BROKER_REJECTED),
        "달력의 인증 실패도 증권사가 키를 받지 않음으로 알린다: {states:?}"
    );
    assert_no_repeats(&states);
    let first = toss.requests(CALENDAR_KR_PATH).await.len();
    assert!(
        (2..=12).contains(&first),
        "달력도 물러서며 다시 받는다(고정 간격도 아니고 쉬지 않고 두드리지도 않는다). 앞 구간: {first}번"
    );

    // 뒤 구간: 같은 실패가 이어지는 동안 상태를 다시 알리지 않는다(증권사 시세로 오락가락하지 않는다).
    let later_states = feed_states_for(&mut ws, RETRY_WATCH_LATER).await;
    assert!(later_states.is_empty(), "{later_states:?}");
    let later = toss.requests(CALENDAR_KR_PATH).await.len() - first;
    assert!(
        (2..=10).contains(&later),
        "간격은 상한에서 멈추고 그 간격으로 다시 받는다. 뒤 구간: {later}번"
    );

    // 풀리면 증권사 시세가 된다.
    toss.calendar_status.store(0, Ordering::SeqCst);
    wait_feed_state(&mut ws, FEED_LIVE).await;
    stop(app).await;
}

// 검증: N6
#[tokio::test]
async fn 달력을_다시_받는_간격은_설정값이다() {
    let toss = FakeToss::start().await;
    let dir = tempfile::tempdir().unwrap();
    register_keys(dir.path(), &toss).await;
    // 달력이 없다(404). 키 문제가 아니므로 시세는 계속 받는다.
    toss.calendar_broken.store(true, Ordering::SeqCst);
    let mut settings = settings(dir.path(), &[]);
    settings.calendar_retry = Duration::from_millis(100);
    let app = start_app(settings, wiring(Some(&toss), TestClock::at(kst(10, 0, 0)))).await;
    let mut ws = connect(app.local_addr(), 0).await;
    wait_price(&mut ws, "005930", "72000").await;

    let before = toss.requests(CALENDAR_KR_PATH).await.len();
    let states = feed_states_for(&mut ws, Duration::from_millis(1000)).await;
    let during = toss.requests(CALENDAR_KR_PATH).await.len() - before;
    assert!(
        (3..=15).contains(&during),
        "달력은 설정한 간격(0.1초)으로 다시 받는다. 1초에 {during}번"
    );
    assert!(
        !states.iter().any(|state| state == FEED_BROKER_REJECTED),
        "달력이 없는 것은 키를 받지 않는 것이 아니다: {states:?}"
    );
    toss.set_price("72700");
    wait_price(&mut ws, "005930", "72700").await;
    stop(app).await;
}

// ───────────── 규칙 12: 빈 시세 ─────────────

// 검증: V31, V32
#[tokio::test]
async fn 빈_시세는_성공이_아니라서_연결_중에_머물며_물러선다() {
    let toss = FakeToss::start().await;
    let dir = tempfile::tempdir().unwrap();
    register_keys(dir.path(), &toss).await;
    // 증권사가 `{"result": []}`를 준다.
    toss.empty.store(true, Ordering::SeqCst);
    let app = start_app(
        fast_retry(settings(dir.path(), &[])),
        wiring(Some(&toss), TestClock::at(kst(10, 0, 0))),
    )
    .await;
    let mut ws = connect(app.local_addr(), 0).await;

    let states = feed_states_for(&mut ws, RETRY_WATCH_FIRST).await;
    assert!(
        !states.iter().any(|state| state == FEED_LIVE),
        "시세를 하나도 받지 못했는데 증권사 시세라고 알렸다: {states:?}"
    );
    assert_eq!(
        states.last().map(String::as_str),
        Some(FEED_CONNECTING),
        "{states:?}"
    );
    let first = toss.requests(PRICES_PATH).await.len();
    assert!(
        (2..=12).contains(&first),
        "원래 간격(20ms)으로 계속 두드리지 않고 물러선다. 앞 구간의 시세 요청: {first}번"
    );
    let quotes = find_frame(&mut ws, QUIET, |frame| transient(frame, KIND_QUOTES)).await;
    assert!(
        quotes.is_none(),
        "받지 못한 시세를 보내지 않는다: {quotes:?}"
    );

    // 시세가 오기 시작하면 증권사 시세가 된다.
    toss.empty.store(false, Ordering::SeqCst);
    wait_feed_state(&mut ws, FEED_LIVE).await;
    wait_price(&mut ws, "005930", "72000").await;
    stop(app).await;
}

// ───────────── 규칙 13: 증권사가 요청을 거절함 ─────────────

// 검증: V31, V32
#[tokio::test]
async fn 종목_하나가_틀려도_나머지_종목의_시세는_오고_거절된_종목을_알린다() {
    let toss = FakeToss::start().await;
    let dir = tempfile::tempdir().unwrap();
    register_keys(dir.path(), &toss).await;
    // 증권사에 없는 종목이 든 요청은 통째로 404 `stock-not-found`다.
    toss.unknown_symbols
        .lock()
        .unwrap()
        .push("NOPE".to_string());
    let app = start_app(
        fast_retry(settings_without_symbols(
            dir.path(),
            &["--symbols", "005930,NOPE,000660"],
        )),
        wiring(Some(&toss), TestClock::at(kst(10, 0, 0))),
    )
    .await;
    let addr = app.local_addr();
    let mut ws = connect(addr, 0).await;

    let quotes = wait_price(&mut ws, "000660", "72000").await;
    assert!(price_of(&quotes, "NOPE").is_none());
    wait_price(&mut ws, "005930", "72000").await;

    let rejected = latest(addr, KIND_SYMBOLS_REJECTED).await;
    assert_eq!(
        rejected["symbols"],
        json!(["NOPE"]),
        "어느 종목이 거절됐는지 알린다"
    );
    assert_eq!(
        latest(addr, KIND_FEED_STATUS).await["state"],
        json!(FEED_LIVE),
        "받는 종목이 있으면 증권사 시세다"
    );
    let states = feed_states_for(&mut ws, QUIET).await;
    assert!(
        !states.iter().any(|state| state == FEED_BROKER_UNREACHABLE),
        "거절은 닿지 않음이 아니다: {states:?}"
    );
    // 흐름은 이어진다.
    toss.set_price("72800");
    wait_price(&mut ws, "000660", "72800").await;
    stop(app).await;
}

// 검증: V31
#[tokio::test]
async fn 증권사가_요청을_거절하면_닿지_않음이_아니라_거절이라고_알린다() {
    let toss = FakeToss::start().await;
    let dir = tempfile::tempdir().unwrap();
    register_keys(dir.path(), &toss).await;
    toss.unknown_symbols
        .lock()
        .unwrap()
        .push("NOPE".to_string());
    let app = start_app(
        fast_retry(settings_without_symbols(dir.path(), &["--symbols", "NOPE"])),
        wiring(Some(&toss), TestClock::at(kst(10, 0, 0))),
    )
    .await;
    let addr = app.local_addr();
    let mut ws = connect(addr, 0).await;

    let states = feed_states_for(&mut ws, RETRY_WATCH_FIRST).await;
    assert_eq!(
        states.last().map(String::as_str),
        Some(FEED_REQUEST_REJECTED),
        "4xx 거절은 따로 알린다: {states:?}"
    );
    for wrong in [FEED_BROKER_UNREACHABLE, FEED_LIVE, FEED_BROKER_REJECTED] {
        assert!(
            !states.iter().any(|state| state == wrong),
            "{wrong}가 아니다: {states:?}"
        );
    }
    assert_no_repeats(&states);
    let asked = toss.requests(PRICES_PATH).await.len();
    assert!(
        (1..=12).contains(&asked),
        "거절된 요청을 원래 간격으로 계속 두드리지 않는다: {asked}번"
    );
    assert_eq!(
        latest(addr, KIND_SYMBOLS_REJECTED).await["symbols"],
        json!(["NOPE"])
    );
    let quotes = find_frame(&mut ws, QUIET, |frame| transient(frame, KIND_QUOTES)).await;
    assert!(quotes.is_none(), "{quotes:?}");
    stop(app).await;
}

// ───────────── 규칙 14: 등록이 흐름을 깨운다 ─────────────

// 검증: C16, V31
#[tokio::test]
async fn 물러서_있는_중에_키를_다시_등록하면_바로_다시_시도한다() {
    let toss = FakeToss::start().await;
    let ai = FakeAi::start().await;
    let dir = tempfile::tempdir().unwrap();
    register_broker(dir.path(), &toss).await;
    register_ai(dir.path(), &ai).await;
    toss.token_status.store(401, Ordering::SeqCst);
    // 한 번 실패하면 30초를 물러선다.
    let app = start_app(
        slow_retry(settings(dir.path(), &[])),
        wiring_both(&toss, &ai, TestClock::at(kst(10, 0, 0))),
    )
    .await;
    let addr = app.local_addr();
    let mut ws = connect(addr, 0).await;
    wait_feed_state(&mut ws, FEED_BROKER_REJECTED).await;

    // 증권사가 다시 키를 받는다. 화면에서 키를 다시 등록한다.
    toss.token_status.store(0, Ordering::SeqCst);
    let reply = post_json(
        addr,
        SETUP_KEYS_PATH,
        &broker_body(CLIENT_ID, CLIENT_SECRET),
    )
    .await;
    assert_registered(&reply);
    let price = price_within(&mut ws, "005930", WOKEN).await;
    assert_eq!(
        price.as_deref(),
        Some("72000"),
        "등록하면 최대 간격(30초)을 기다리지 않고 바로 다시 시도한다"
    );
    stop(app).await;
}

// 검증: V31
#[tokio::test]
async fn 물러서_있는_중에_종목을_등록하면_바로_다시_시도한다() {
    let toss = FakeToss::start().await;
    let dir = tempfile::tempdir().unwrap();
    register_keys(dir.path(), &toss).await;
    std::fs::write(
        dir.path().join(WATCHLIST_FILE),
        json!({ "symbols": ["005930"] }).to_string(),
    )
    .unwrap();
    toss.failing.store(true, Ordering::SeqCst);
    let app = start_app(
        slow_retry(settings_without_symbols(dir.path(), &[])),
        wiring(Some(&toss), TestClock::at(kst(10, 0, 0))),
    )
    .await;
    let addr = app.local_addr();
    let mut ws = connect(addr, 0).await;
    wait_feed_state(&mut ws, FEED_BROKER_UNREACHABLE).await;

    toss.failing.store(false, Ordering::SeqCst);
    let reply = post_json(
        addr,
        SETUP_SYMBOLS_PATH,
        &symbols_body(&["005930", "000660"]),
    )
    .await;
    assert_registered(&reply);
    let price = price_within(&mut ws, "000660", WOKEN).await;
    assert_eq!(
        price.as_deref(),
        Some("72000"),
        "종목이 등록되면 바로 새 종목으로 다시 시도한다"
    );
    stop(app).await;
}

// ───────────── 규칙 15: 한도 초과의 기다림 ─────────────

// 검증: V31
#[tokio::test]
async fn 증권사가_준_기다림보다_빨리_다시_시도하지_않는다() {
    let toss = FakeToss::start().await;
    let dir = tempfile::tempdir().unwrap();
    register_keys(dir.path(), &toss).await;
    // 현재가가 429이고 `Retry-After: 1`이다.
    toss.rate_limited_secs.store(1, Ordering::SeqCst);
    // 어댑터 안의 다시 시도를 꺼서 시세 흐름의 시도 하나가 요청 하나가 되게 한다.
    let mut wiring = wiring(Some(&toss), TestClock::at(kst(10, 0, 0)));
    wiring.upstreams.toss.rate_limit_retries = 0;
    // 물러서기 간격(0.1 → 0.4초)은 증권사가 준 기다림(1초)보다 짧다.
    let app = start_app(fast_retry(settings(dir.path(), &[])), wiring).await;
    let mut ws = connect(app.local_addr(), 0).await;

    wait_requests(&toss, PRICES_PATH, 1).await;
    // 물러서기만 따르면 이 0.6초 동안 0.1 · 0.3초쯤에 두 번 더 나간다.
    let states = feed_states_for(&mut ws, Duration::from_millis(600)).await;
    assert_eq!(
        toss.requests(PRICES_PATH).await.len(),
        1,
        "기다리라고 한 1초 안에는 다시 요청하지 않는다"
    );
    assert!(!states.iter().any(|state| state == FEED_LIVE), "{states:?}");
    // 기다림이 지나면 다시 시도한다(멈춰 버리지 않는다).
    wait_requests(&toss, PRICES_PATH, 2).await;

    toss.rate_limited_secs.store(0, Ordering::SeqCst);
    wait_feed_state(&mut ws, FEED_LIVE).await;
    wait_price(&mut ws, "005930", "72000").await;
    stop(app).await;
}

// 검증: N6
#[tokio::test]
async fn 요청이_끝나기를_기다리는_동안에도_시세_늦음을_제때_알린다() {
    let toss = FakeToss::start().await;
    let dir = tempfile::tempdir().unwrap();
    register_keys(dir.path(), &toss).await;
    let clock = TestClock::at(kst(10, 0, 0));
    let app = start_app(
        settings(dir.path(), &[]),
        wiring(Some(&toss), Arc::clone(&clock)),
    )
    .await;
    let mut ws = connect(app.local_addr(), 0).await;
    wait_price(&mut ws, "005930", "72000").await;

    // 이제부터 증권사의 응답이 2.5초씩 걸린다. 가진 시세는 10:00:00에 받은 것이다.
    toss.price_delay_ms.store(2500, Ordering::SeqCst);
    tokio::time::sleep(Duration::from_millis(100)).await;
    clock.set(kst(10, 0, 31));
    // 응답은 일러도 2.4초 뒤에 온다. 늦음 판단은 그 요청을 기다리지 않고 돈다.
    let stale = wait_stale(&mut ws, Duration::from_millis(1800))
        .await
        .expect("요청이 끝나기를 기다리느라 시세 늦음이 늦어지면 안 된다");
    let symbols = stale["symbols"].as_array().expect("늦은 종목들");
    assert!(symbols.contains(&json!("005930")), "{stale}");
    stop(app).await;
}

// ───────────── 규칙 16: 한 번도 안 온 종목 ─────────────

// 검증: N6
#[tokio::test]
async fn 한_번도_시세가_오지_않은_종목도_30초_뒤에는_시세_늦음에_든다() {
    let toss = FakeToss::start().await;
    let dir = tempfile::tempdir().unwrap();
    register_keys(dir.path(), &toss).await;
    // 000660의 시세는 결과에서 빠진다(요청은 성공한다).
    toss.silent_symbols
        .lock()
        .unwrap()
        .push("000660".to_string());
    let clock = TestClock::at(kst(10, 0, 0));
    let app = start_app(
        settings(dir.path(), &[]),
        wiring(Some(&toss), Arc::clone(&clock)),
    )
    .await;
    let mut ws = connect(app.local_addr(), 0).await;
    let quotes = wait_price(&mut ws, "005930", "72000").await;
    assert!(price_of(&quotes, "000660").is_none());

    let silent_is_late = |frame: &Value| {
        transient(frame, KIND_QUOTES_STALE).filter(|stale| {
            stale["symbols"]
                .as_array()
                .is_some_and(|symbols| symbols.contains(&json!("000660")))
        })
    };
    clock.set(kst(10, 0, 30));
    let early = find_frame(&mut ws, QUIET, silent_is_late).await;
    assert!(
        early.is_none(),
        "딱 30초는 아직 늦은 것이 아니다: {early:?}"
    );

    clock.set(kst(10, 0, 31));
    let stale = find_frame(&mut ws, STEP, silent_is_late)
        .await
        .expect("요청했는데 한 번도 오지 않은 종목이 조용히 빠지면 안 된다");
    assert_eq!(
        stale["hours_unknown"],
        json!(false),
        "달력을 아는 동안의 판단이다"
    );
    // 오고 있는 종목의 시세는 계속 온다.
    toss.set_price("72900");
    wait_price(&mut ws, "005930", "72900").await;
    stop(app).await;
}
