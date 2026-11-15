//! `POST /api/session`: 세션을 준다. 그 컴퓨터는 키 없이(C15), 밖에서는 접속 키로(C1 · V38).
//!
//! 규칙은 [`crate::AccessKeys`]가 있는 `access` 모듈의 문서에 있다.

use std::net::IpAddr;
use std::sync::Arc;

use axum::extract::{Request, State};
use axum::http::StatusCode;
use axum::http::header::SET_COOKIE;
use axum::response::{IntoResponse, Response};
use pi_store::UnixMillis;

use crate::guard::{Outside, Peer, Shared};
use crate::session::SessionKind;
use crate::{
    ACCESS_ENTER, ACCESS_KEY_FIELD, ACCESS_KEY_WRONG, LOCKED_STATUS, REMOTE_SESSION_MAX_AGE,
    SESSION_COOKIE,
};

/// 세션 요청 본문의 크기 상한(바이트). 키 하나만 담으면 되므로 작게 둔다.
const MAX_BODY_BYTES: usize = 4 * 1024;
/// 키가 틀렸거나, 없거나, 둔 키가 없을 때 같은 본문(어느 경우인지 알려 주지 않는다).
const DENIED: &str = "접속 키가 맞지 않습니다";

/// 세션을 준다. 키 글자는 응답 · 기록 · 로그 어디에도 싣지 않는다.
pub(crate) async fn enter(State(shared): State<Arc<Shared>>, request: Request) -> Response {
    let Some(Peer(ip)) = request.extensions().get::<Peer>().copied() else {
        return reject(StatusCode::UNAUTHORIZED, DENIED);
    };
    let outside = request.extensions().get::<Outside>().is_some();
    let body = axum::body::to_bytes(request.into_body(), MAX_BODY_BYTES).await;
    let key = body.ok().as_deref().and_then(key_in);
    if outside {
        enter_remote(&shared, ip, key).await
    } else {
        enter_local(&shared, ip, key).await
    }
}

/// 본문 `{"access_key": "<키>"}`의 키. 없거나 JSON이 아니면 없음.
fn key_in(body: &[u8]) -> Option<String> {
    let value: serde_json::Value = serde_json::from_slice(body).ok()?;
    value.get(ACCESS_KEY_FIELD)?.as_str().map(str::to_owned)
}

/// 그 컴퓨터: 키가 없으면 그대로 들어온다. 키를 넣었으면 맞춰 보고, 틀리면 잠그지 않고 늦춘다.
async fn enter_local(shared: &Shared, ip: IpAddr, key: Option<String>) -> Response {
    let access = &shared.access;
    let Some(key) = key else {
        return grant(shared, ip, SessionKind::Local).await;
    };
    match access.verify(&key).await {
        Ok(Some(_)) => {
            access.strikes.loopback_succeed(ip);
            grant(shared, ip, SessionKind::Local).await
        }
        Ok(None) => {
            if !access
                .record(&ip.to_string(), ACCESS_KEY_WRONG, access.now())
                .await
            {
                tracing::error!("접속 키 틀림을 기록하지 못했습니다");
            }
            let (start, max) = access.loopback_delay;
            let delay = access.strikes.loopback_fail(ip, start, max);
            tokio::time::sleep(delay).await;
            reject(StatusCode::UNAUTHORIZED, DENIED)
        }
        Err(_) => unreadable(),
    }
}

/// 다른 기기(또는 프록시를 거친 요청): 잠겨 있으면 막고, 키가 맞을 때만 원격 세션을 준다.
async fn enter_remote(shared: &Shared, ip: IpAddr, key: Option<String>) -> Response {
    let access = &shared.access;
    let now = access.now();
    // 잠금 확인과 시도 기록을 한 번에: 자리를 잡은 시도만 키를 맞춰 본다.
    let Some(slot) = access.strikes.reserve(ip, now) else {
        let status = StatusCode::from_u16(LOCKED_STATUS).unwrap_or(StatusCode::TOO_MANY_REQUESTS);
        return reject(status, "잠시 뒤에 다시 시도하세요");
    };
    let Some(key) = key else {
        slot.release();
        return reject(StatusCode::UNAUTHORIZED, DENIED);
    };
    match access.verify(&key).await {
        Ok(Some(generation)) => {
            slot.succeed();
            let kind = SessionKind::Remote {
                generation,
                made_at: now,
            };
            grant(shared, ip, kind).await
        }
        Ok(None) => {
            access.remote_failed(slot, ip, now).await;
            reject(StatusCode::UNAUTHORIZED, DENIED)
        }
        Err(_) => {
            slot.release();
            unreadable()
        }
    }
}

/// 접속 기록을 남기고 세션을 준다(C15 · M6). 기록을 남기지 못하면 세션도 주지 않는다.
async fn grant(shared: &Shared, ip: IpAddr, kind: SessionKind) -> Response {
    let access = &shared.access;
    let at: UnixMillis = match kind {
        SessionKind::Local => access.now(),
        SessionKind::Remote { made_at, .. } => made_at,
    };
    if !access.record(&ip.to_string(), ACCESS_ENTER, at).await {
        tracing::error!("접속 기록을 남기지 못해 세션을 주지 않습니다");
        return reject(
            StatusCode::INTERNAL_SERVER_ERROR,
            "접속 기록을 남기지 못했습니다",
        );
    }
    let Ok(value) = access.sessions.create(kind, access.session_now()) else {
        tracing::error!("난수를 얻지 못해 세션을 주지 않습니다");
        return reject(
            StatusCode::INTERNAL_SERVER_ERROR,
            "세션을 만들지 못했습니다",
        );
    };
    let mut cookie = format!("{SESSION_COOKIE}={value}; HttpOnly; SameSite=Strict; Path=/");
    if matches!(kind, SessionKind::Remote { .. }) {
        cookie.push_str(&format!("; Max-Age={}", REMOTE_SESSION_MAX_AGE.as_secs()));
    }
    (StatusCode::NO_CONTENT, [(SET_COOKIE, cookie)]).into_response()
}

fn unreadable() -> Response {
    tracing::error!("저장된 접속 키를 읽지 못했습니다");
    reject(
        StatusCode::INTERNAL_SERVER_ERROR,
        "접속 키를 확인하지 못했습니다",
    )
}

fn reject(status: StatusCode, why: &'static str) -> Response {
    (status, why).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 본문에서_키_칸만_읽는다() {
        assert_eq!(key_in(br#"{"access_key":"ABC"}"#).as_deref(), Some("ABC"));
        assert_eq!(key_in(br#"{"access_key":""}"#).as_deref(), Some(""));
        assert!(key_in(br#"{"access_key":1}"#).is_none());
        assert!(key_in(br#"{"key":"ABC"}"#).is_none());
        assert!(key_in(b"").is_none());
        assert!(key_in(b"not json").is_none());
    }
}
