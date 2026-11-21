//! 시작 전 필수 설정의 경로: 설정 상태 읽기 · 키 등록 · 종목 등록. 모양은 [`crate::setup`]의 문서에 있다.
//!
//! 요청 본문을 기록하지 않는다. 응답은 "등록됨"이나 까닭뿐이다.

use std::pin::Pin;
use std::sync::Arc;

use axum::Router;
use axum::body::HttpBody;
use axum::extract::{Request, State};
use axum::http::StatusCode;
use axum::http::header::CACHE_CONTROL;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use serde_json::{Value, json};
use zeroize::Zeroize;

use crate::desk::Desk;
use crate::{
    REMOTE_KEYS_PATH, SETUP_AI_PATH, SETUP_KEYS_PATH, SETUP_PATH, SETUP_PROVIDERS_PATH,
    SETUP_SLOTS_PATH, SETUP_SYMBOLS_PATH, SetupRefusal,
};

/// 등록 요청 본문의 크기 상한(바이트). 키 몇 개나 종목 200개에는 넉넉하다. 넘으면 더 읽지 않고
/// 413 `body_too_large`로 거부한다(5차, 원본 Hermes api_server).
const MAX_BODY_BYTES: usize = 16 * 1024;

/// 세션이 있어야 열리는 경로: 설정 상태와 다른 기기의 키 등록([`REMOTE_KEYS_PATH`], 몸통 · 상한 · 답은
/// [`SETUP_KEYS_PATH`]와 같다: 답은 등록 여부뿐, 키 값은 다시 보여 주지 않는다).
/// `GateBuilder::protected_routes`에 끼운다.
pub(crate) fn status_routes(desk: Arc<Desk>) -> Router {
    Router::new()
        .route(SETUP_PATH, get(status))
        .route(SETUP_SLOTS_PATH, get(slot_status))
        .route(SETUP_PROVIDERS_PATH, get(provider_status))
        .route(REMOTE_KEYS_PATH, post(register_key))
        .with_state(desk)
}

/// 세션 · 같은 출처에 더해 그 컴퓨터에서만 열리는 경로: 키 등록 · 종목 등록.
/// `GateBuilder::local_routes`에 끼운다(그 경로 자체가 `pi_gate::local_only`를 거친다).
pub(crate) fn register_routes(desk: Arc<Desk>) -> Router {
    Router::new()
        .route(SETUP_KEYS_PATH, post(register_key))
        .route(SETUP_SYMBOLS_PATH, post(register_symbols))
        .route(SETUP_AI_PATH, post(choose_ai))
        .with_state(desk)
}

/// JSON 응답. 브라우저나 중간 자리가 간직하지 않게 한다.
fn reply(status: StatusCode, body: Value) -> Response {
    (status, [(CACHE_CONTROL, "no-store")], axum::Json(body)).into_response()
}

/// 등록 결과를 응답으로 옮긴다: "등록됨" 또는 까닭. 그 밖의 칸은 없다.
fn registered(outcome: Result<(), SetupRefusal>) -> Response {
    match outcome {
        Ok(()) => reply(StatusCode::OK, json!({ "registered": true })),
        Err(why) => {
            let status =
                StatusCode::from_u16(why.status()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
            reply(status, json!({ "error": why.reason() }))
        }
    }
}

/// 본문을 상한까지만 읽는다. 넘으면 [`SetupRefusal::BodyTooLarge`](확인기를 부르지 않고 저장하지 않는다),
/// 읽지 못하면 [`SetupRefusal::InvalidInput`].
///
/// 상한만큼 미리 잡아 두고 옮겨 담는다: 늘리며 옮기다 옛 자리에 본문 조각이 남지 않게 한다.
/// 거부할 때는 담은 것을 0으로 덮는다. 웹 서버 라이브러리가 쥔 조각은 지우지 못한다(한계).
async fn body_of(request: Request) -> Result<Vec<u8>, SetupRefusal> {
    let mut body = request.into_body();
    // 길이를 미리 알리면(Content-Length) 읽기 전에 거부한다.
    if body.size_hint().lower() > MAX_BODY_BYTES as u64 {
        return Err(SetupRefusal::BodyTooLarge);
    }
    let mut out = Vec::with_capacity(MAX_BODY_BYTES);
    while let Some(frame) = std::future::poll_fn(|cx| Pin::new(&mut body).poll_frame(cx)).await {
        let Ok(frame) = frame else {
            out.zeroize();
            return Err(SetupRefusal::InvalidInput);
        };
        let Ok(data) = frame.into_data() else {
            continue;
        };
        if data.len() > MAX_BODY_BYTES - out.len() {
            out.zeroize();
            return Err(SetupRefusal::BodyTooLarge);
        }
        out.extend_from_slice(&data);
    }
    Ok(out)
}

async fn status(State(desk): State<Arc<Desk>>) -> Response {
    reply(StatusCode::OK, desk.status().to_json())
}

/// (8차) 바깥 키 칸별 등록 여부([`crate::slots`]).
async fn slot_status(State(desk): State<Arc<Desk>>) -> Response {
    reply(
        StatusCode::OK,
        crate::slots::slots_json(&desk.slot_status()),
    )
}

/// (10b 10번) 공급자 선언에서 만든 키 칸 목록.
async fn provider_status(State(desk): State<Arc<Desk>>) -> Response {
    reply(
        StatusCode::OK,
        crate::slots::providers_json(&desk.provider_status()),
    )
}

async fn register_key(State(desk): State<Arc<Desk>>, request: Request) -> Response {
    let outcome = match body_of(request).await {
        Ok(mut body) => {
            // 읽은 뒤 0으로 덮인다.
            let outcome = desk.register_key(&mut body).await;
            body.zeroize();
            outcome
        }
        Err(why) => Err(why),
    };
    registered(outcome)
}

async fn register_symbols(State(desk): State<Arc<Desk>>, request: Request) -> Response {
    let outcome = match body_of(request).await {
        Ok(body) => {
            // 파일을 쓰는 동안 다른 요청을 막지 않게 따로 돌린다.
            tokio::task::spawn_blocking(move || desk.register_symbols(&body))
                .await
                .unwrap_or(Err(SetupRefusal::SaveFailed))
        }
        Err(why) => Err(why),
    };
    registered(outcome)
}

/// (6차) AI 연결 고르기. 갖춰졌는지(`{"registered": bool}`)나 까닭만 돌려준다.
async fn choose_ai(State(desk): State<Arc<Desk>>, request: Request) -> Response {
    let outcome = match body_of(request).await {
        Ok(body) => desk.choose_ai(&body).await,
        Err(why) => Err(why),
    };
    match outcome {
        Ok(registered) => reply(StatusCode::OK, json!({ "registered": registered })),
        Err(why) => registered(Err(why)),
    }
}
