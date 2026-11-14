//! 5차 작업 4번: 등록 요청 본문이 너무 크면 413(4차 검토 참고 5).
//!
//! 근거: `docs/dev/modules/gate.md` "5차 작업" 4. 원본(Hermes, 읽기만 했다)
//! `gateway/platforms/api_server.py`는 본문이 상한을 넘으면 `code="body_too_large"`와 HTTP 413으로 거부한다.
//! 이 서버의 거부 응답 모양(`{"error": 까닭}`)을 따라 `{"error": "body_too_large"}`로 본다.
//! 키 등록과 종목 등록은 같은 본문 읽기를 쓰므로 둘 다 본다(가장 보수적인 쪽).

use super::setup::{broker_body, post_json, setup_status, symbols_body};
use super::*;
use price_inha::{SETUP_KEYS_PATH, SETUP_SYMBOLS_PATH, SetupRefusal};
use serde_json::Value;

/// 상한(16KiB)을 넉넉히 넘는 본문을 만드는 덧칸 길이.
const PADDING_CHARS: usize = 64 * 1024;

/// 본문에 큰 덧칸을 붙인다.
fn padded(mut body: Value) -> Value {
    body["padding"] = Value::String("x".repeat(PADDING_CHARS));
    body
}

// 검증: E4, C15
#[tokio::test]
async fn 키_등록_본문이_너무_크면_413으로_거부하고_확인하지_않는다() {
    let toss = FakeToss::start().await;
    let ai = FakeAi::start().await;
    let dir = tempfile::tempdir().unwrap();
    let app = start_app(
        settings_without_symbols(dir.path(), &[]),
        wiring_both(&toss, &ai, TestClock::at(kst(10, 0, 0))),
    )
    .await;
    let addr = app.local_addr();
    let toss_before = toss.total_requests().await;
    let ai_before = ai.total_requests().await;

    let body = padded(broker_body(CLIENT_ID, CLIENT_SECRET));
    assert!(body.to_string().len() > PADDING_CHARS);
    let reply = post_json(addr, SETUP_KEYS_PATH, &body).await;
    assert_eq!(reply.status, 413, "본문이 너무 크면 413이다");
    assert_eq!(
        reply.json(),
        json!({ "error": "body_too_large" }),
        "까닭만 알린다"
    );
    assert_eq!(SetupRefusal::BodyTooLarge.status(), 413);
    assert_eq!(SetupRefusal::BodyTooLarge.reason(), "body_too_large");

    assert_eq!(
        toss.total_requests().await,
        toss_before,
        "증권사에 확인하러 가지 않는다"
    );
    assert_eq!(
        ai.total_requests().await,
        ai_before,
        "AI 공급자에도 가지 않는다"
    );
    let status = setup_status(addr).await;
    assert_eq!(
        status["ready"],
        json!(false),
        "아무것도 등록하지 않았다: {status}"
    );
    let secrets = SecretStore::open(dir.path()).unwrap();
    assert!(
        secrets.status().unwrap().iter().all(|s| !s.registered),
        "보관 폴더에 아무것도 없다"
    );

    // 크기가 맞는 같은 요청은 그대로 등록된다.
    let reply = post_json(
        addr,
        SETUP_KEYS_PATH,
        &broker_body(CLIENT_ID, CLIENT_SECRET),
    )
    .await;
    assert_eq!(reply.status, 200);
    stop(app).await;
}

// 검증: E4, C15
#[tokio::test]
async fn 종목_등록_본문이_너무_크면_413으로_거부하고_저장하지_않는다() {
    let toss = FakeToss::start().await;
    let ai = FakeAi::start().await;
    let dir = tempfile::tempdir().unwrap();
    let app = start_app(
        settings_without_symbols(dir.path(), &[]),
        wiring_both(&toss, &ai, TestClock::at(kst(10, 0, 0))),
    )
    .await;
    let addr = app.local_addr();

    let body = padded(symbols_body(&["005930"]));
    let reply = post_json(addr, SETUP_SYMBOLS_PATH, &body).await;
    assert_eq!(reply.status, 413, "본문이 너무 크면 413이다");
    assert_eq!(reply.json(), json!({ "error": "body_too_large" }));
    assert!(
        !dir.path().join(price_inha::WATCHLIST_FILE).exists(),
        "종목 목록을 저장하지 않는다"
    );

    let reply = post_json(addr, SETUP_SYMBOLS_PATH, &symbols_body(&["005930"])).await;
    assert_eq!(reply.status, 200, "크기가 맞으면 등록된다");
    stop(app).await;
}
