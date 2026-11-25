//! (39차) 메신저 경로: 상태 읽기 · 토큰 등록 · 지우기 · 페어링 승인 · 빼기(계약은 `crate::messenger` 문서).
//!
//! 상태 읽기는 세션이 있어야 열리는 경로에, 나머지는 같은 컴퓨터에서만 열리는 경로(`GateBuilder::local_routes`)에
//! 단다(원격은 관문이 403).
//!
//! 몸통(토큰 · 코드)을 기록하지 않는다. 응답 · 이벤트에 토큰 · 코드 · 보낸 사람 ID 전체를 싣지 않는다.

use std::sync::{Arc, Mutex, PoisonError};

use axum::Router;
use axum::extract::{Request, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use axum::routing::{get, post};
use pi_channel::ApproveError;
use pi_gate::{FORWARDING_HEADERS, Peer};
use pi_secrets::{Credentials, KeyKind, ProviderKeys, SecretsError, Verifier, VerifyFailure};
use serde_json::{Value, json};
use zeroize::{Zeroize, Zeroizing};

use super::hub::Messenger;
use super::status::{
    MESSENGER_ACTION_PAIRED, MESSENGER_ACTION_REVOKED, MESSENGER_ACTION_TOKEN_REMOVED,
    MESSENGER_ACTION_TOKEN_SET, sender_tail,
};
use crate::SetupRefusal;
use crate::setup_http::{body_of, reply};

/// 상태 읽기 `GET`(세션).
pub const MESSENGER_PATH: &str = "/api/messenger";
/// 텔레그램 봇 토큰 등록 `POST` `{"token"}` · 지우기 `DELETE`(같은 컴퓨터에서만).
pub const MESSENGER_TELEGRAM_PATH: &str = "/api/messenger/telegram";
/// 페어링 승인 `POST` `{"code"}`(같은 컴퓨터에서만).
pub const MESSENGER_PAIRING_APPROVE_PATH: &str = "/api/messenger/pairing/approve";
/// 허용된 사람 빼기 `POST`(같은 컴퓨터에서만).
pub const MESSENGER_PAIRING_REVOKE_PATH: &str = "/api/messenger/pairing/revoke";

/// 틀린 페어링 코드의 답(400, 남은 횟수를 싣지 않는다).
pub const MESSENGER_WRONG_CODE: &str = "코드가 맞지 않습니다";
/// 틀린 코드가 쌓여 잠겼을 때의 답(423, 풀리는 시각은 `locked_until`).
pub const MESSENGER_LOCKED: &str = "틀린 코드가 많아 잠시 승인을 받지 않습니다";
/// 이미 허용된 사람이 있을 때의 답(409).
pub const MESSENGER_OWNER_EXISTS: &str = "이미 연결된 사람이 있습니다 — 먼저 빼세요";
/// 시연 모드에서 등록하려 할 때의 답(409).
pub const MESSENGER_DEMO_REFUSED: &str = "시연 모드에서는 메신저를 쓰지 않습니다";

/// 세션 경로(`GateBuilder::protected_routes`)에 끼운다. 바꾸는 경로는 그 안에서 `pi_gate::local_only`로 닫는다.
pub(crate) fn routes(hub: Arc<Messenger>) -> Router {
    let change = Router::new()
        .route(MESSENGER_TELEGRAM_PATH, post(register).delete(remove))
        .route(MESSENGER_PAIRING_APPROVE_PATH, post(approve))
        .route(MESSENGER_PAIRING_REVOKE_PATH, post(revoke))
        .with_state(Arc::clone(&hub));
    Router::new()
        .route(MESSENGER_PATH, get(status))
        .with_state(hub)
        .merge(pi_gate::local_only(change))
}

/// 거절 답(`{"error"}` — 키 등록과 같은 글자 · 상태).
fn refused(why: SetupRefusal) -> Response {
    let status = StatusCode::from_u16(why.status()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    reply(status, json!({ "error": why.reason() }))
}

/// 이 요청이 그 컴퓨터에서 왔는가(`pi_gate::local_only`와 같은 판단).
fn from_this_computer(peer: Option<&Peer>, headers: &HeaderMap) -> bool {
    peer.is_some_and(|Peer(ip)| ip.to_canonical().is_loopback())
        && FORWARDING_HEADERS
            .iter()
            .all(|name| !headers.contains_key(*name))
}

async fn status(
    State(hub): State<Arc<Messenger>>,
    peer: Option<axum::Extension<Peer>>,
    headers: HeaderMap,
) -> Response {
    let editable = from_this_computer(peer.as_deref(), &headers);
    reply(StatusCode::OK, hub.status_json(editable))
}

/// 몸통의 글자 칸 하나(앞뒤 공백을 뗀다). 없거나 비었으면 [`SetupRefusal::InvalidInput`]. 읽은 몸통은 0으로 덮는다.
async fn field_of(request: Request, name: &str) -> Result<Zeroizing<String>, SetupRefusal> {
    let mut body = body_of(request).await?;
    let parsed: Result<Value, _> = serde_json::from_slice(&body);
    body.zeroize();
    let Ok(mut value) = parsed else {
        return Err(SetupRefusal::InvalidInput);
    };
    let found = value
        .get(name)
        .and_then(Value::as_str)
        .map(|text| Zeroizing::new(text.trim().to_string()));
    if let Some(Value::String(raw)) = value.get_mut(name) {
        raw.zeroize();
    }
    found
        .filter(|text| !text.is_empty())
        .ok_or(SetupRefusal::InvalidInput)
}

async fn register(State(hub): State<Arc<Messenger>>, request: Request) -> Response {
    if hub.demo {
        return reply(
            StatusCode::CONFLICT,
            json!({ "error": "demo_mode", "message": MESSENGER_DEMO_REFUSED }),
        );
    }
    if !hub.allow_attempt() {
        tracing::warn!("메신저 토큰 등록 시도가 너무 많아 거부합니다");
        return refused(SetupRefusal::TooManyAttempts);
    }
    let token = match field_of(request, "token").await {
        Ok(token) => token,
        Err(why) => return refused(why),
    };
    match hub.register(token).await {
        Ok(bot) => reply(StatusCode::OK, json!({ "registered": true, "bot": bot })),
        Err(why) => refused(why),
    }
}

async fn remove(State(hub): State<Arc<Messenger>>) -> Response {
    match hub.remove().await {
        Ok(()) => reply(StatusCode::OK, json!({ "removed": true })),
        Err(why) => refused(why),
    }
}

async fn approve(State(hub): State<Arc<Messenger>>, request: Request) -> Response {
    let code = match field_of(request, "code").await {
        Ok(code) => code,
        Err(why) => return refused(why),
    };
    let now = hub.now();
    let approved = hub.with_gate(|gate| gate.approve(&code, now));
    drop(code);
    match approved {
        Ok(sender) => {
            hub.publish(MESSENGER_ACTION_PAIRED, Some(sender_tail(&sender.0)))
                .await;
            reply(StatusCode::OK, json!({ "paired": true }))
        }
        Err(ApproveError::WrongCode) => reply(
            StatusCode::BAD_REQUEST,
            json!({ "error": "wrong_code", "message": MESSENGER_WRONG_CODE }),
        ),
        Err(ApproveError::Locked { until }) => reply(
            StatusCode::LOCKED,
            json!({
                "error": "locked",
                "message": MESSENGER_LOCKED,
                "locked_until": until.timestamp_millis(),
            }),
        ),
        Err(ApproveError::OwnerExists) => reply(
            StatusCode::CONFLICT,
            json!({ "error": "owner_exists", "message": MESSENGER_OWNER_EXISTS }),
        ),
        Err(ApproveError::Book(_)) => refused(SetupRefusal::SaveFailed),
    }
}

async fn revoke(State(hub): State<Arc<Messenger>>) -> Response {
    let revoked = hub.with_gate(|gate| {
        let Some(owner) = gate.allowed()? else {
            return Ok(None);
        };
        gate.revoke(&owner).map(|done| done.then_some(owner))
    });
    match revoked {
        Ok(Some(owner)) => {
            hub.publish(MESSENGER_ACTION_REVOKED, Some(sender_tail(&owner.0)))
                .await;
            reply(StatusCode::OK, json!({ "revoked": true }))
        }
        Ok(None) => reply(StatusCode::OK, json!({ "revoked": false })),
        Err(error) => {
            tracing::warn!(%error, "메신저 허용된 사람을 빼지 못했습니다");
            refused(SetupRefusal::SaveFailed)
        }
    }
}

/// 등록 전 확인: 텔레그램 `getMe`(같은 조립 주소). 확인한 봇 이름을 쥔다.
struct TelegramCheck<'a> {
    hub: &'a Messenger,
    bot: Mutex<Option<String>>,
}

impl Verifier for TelegramCheck<'_> {
    async fn verify(&self, _kind: KeyKind, _values: &Credentials) -> Result<(), VerifyFailure> {
        Err(VerifyFailure::Unreachable)
    }

    async fn verify_provider(
        &self,
        _keys: ProviderKeys,
        values: &Credentials,
    ) -> Result<(), VerifyFailure> {
        let token = values.get("bot_token").ok_or(VerifyFailure::Rejected)?;
        let telegram = self
            .hub
            .telegram(token.expose())
            .ok_or(VerifyFailure::Unreachable)?;
        match pi_channel::Channel::check(&telegram).await {
            Ok(bot) => {
                *self.bot.lock().unwrap_or_else(PoisonError::into_inner) = Some(bot);
                Ok(())
            }
            // 텔레그램은 틀린 토큰에 401을, 모양이 틀린 토큰에 404를 준다.
            Err(pi_channel::Error::Unauthorized | pi_channel::Error::Api { status: 404, .. }) => {
                Err(VerifyFailure::Rejected)
            }
            Err(_) => Err(VerifyFailure::Unreachable),
        }
    }
}

/// 등록 실패 → 화면 까닭(키 등록과 같은 옮김).
fn refusal(error: &SecretsError) -> SetupRefusal {
    match error {
        SecretsError::VerifyFailed(VerifyFailure::Rejected) => SetupRefusal::KeyRejected,
        SecretsError::VerifyFailed(VerifyFailure::Unreachable) => SetupRefusal::Unreachable,
        SecretsError::MissingField { .. }
        | SecretsError::UnknownField
        | SecretsError::TooShort { .. }
        | SecretsError::InvalidValue { .. } => SetupRefusal::InvalidInput,
        _ => SetupRefusal::SaveFailed,
    }
}

impl Messenger {
    /// 토큰을 확인하고(getMe) 저장한 뒤 루프를 (다시) 켜고 이벤트를 낸다. 봇 이름을 돌려준다.
    pub(crate) async fn register(
        self: &Arc<Self>,
        token: Zeroizing<String>,
    ) -> Result<String, SetupRefusal> {
        let check = TelegramCheck {
            hub: self,
            bot: Mutex::new(None),
        };
        let values = Credentials::new().with("bot_token", token.as_str());
        if let Err(error) = self
            .secrets
            .register_provider(super::TELEGRAM_KEYS, values, &check)
            .await
        {
            tracing::warn!(%error, "메신저 토큰을 등록하지 못했습니다");
            return Err(refusal(&error));
        }
        let bot = check
            .bot
            .into_inner()
            .unwrap_or_else(PoisonError::into_inner)
            .unwrap_or_default();
        self.start(&token, Some(bot.clone())).await;
        self.publish(MESSENGER_ACTION_TOKEN_SET, None).await;
        Ok(bot)
    }

    /// 토큰 · 장부 · 받은 자리를 지우고 루프를 멈춘다.
    pub(crate) async fn remove(&self) -> Result<(), SetupRefusal> {
        self.forget_token()?;
        self.stop().await;
        self.with_book_lock(|| super::book::remove_files(&self.home))
            .map_err(|_| SetupRefusal::SaveFailed)?;
        self.publish(MESSENGER_ACTION_TOKEN_REMOVED, None).await;
        Ok(())
    }

    /// 비밀 보관소에서 토큰을 지운다(secrets 9차 `remove_provider`). 없었으면 아무것도 바꾸지 않는다.
    /// 지운 값은 보관소가 살아 있는 동안 가리기 대상으로 남는다.
    fn forget_token(&self) -> Result<(), SetupRefusal> {
        match self.secrets.remove_provider(super::TELEGRAM_KEYS) {
            Ok(_) => Ok(()),
            Err(error) => {
                tracing::warn!(%error, "메신저 토큰을 지우지 못했습니다");
                Err(SetupRefusal::SaveFailed)
            }
        }
    }
}
