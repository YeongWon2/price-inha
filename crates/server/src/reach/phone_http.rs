//! 폰 경로(세션이 있어야 열린다): 푸시 공개키 · 구독 · 알림 목록 · 승인 카드 · 승인/거절 · 하루 정리.

use std::net::{Ipv4Addr, Ipv6Addr};
use std::sync::Arc;

use axum::Router;
use axum::body::Bytes;
use axum::extract::{Path, RawQuery, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use pi_notify::NewSubscription;
use serde_json::{Value, json};
use url::{Host, Url};

use super::linked::Linked;
use super::office_http::reply;
use super::summary::daily_summary;
use super::{
    APPROVAL_DECIDE_PATH, APPROVAL_PATH, DAILY_SUMMARY_PATH, NOTICES_PATH, PUSH_KEY_PATH,
    PUSH_SUBSCRIBE_PATH, SUBSCRIBE_REFUSED,
};
use crate::trading::approval_status_word;
use crate::{Crew, Trading};

/// 폰 경로의 상태.
pub(crate) struct Phone {
    pub(crate) linked: Arc<Linked>,
    pub(crate) trading: Arc<Trading>,
    pub(crate) crew: Option<Arc<Crew>>,
}

fn refuse(status: StatusCode, error: &str) -> Response {
    reply(status, json!({ "error": error }))
}

fn internal_error(error: impl std::fmt::Display) -> Response {
    tracing::warn!(%error, "폰 요청을 처리하지 못했습니다");
    refuse(StatusCode::INTERNAL_SERVER_ERROR, "internal")
}

fn inside_v4(ip: Ipv4Addr) -> bool {
    let [a, b, ..] = ip.octets();
    ip.is_loopback()
        || ip.is_private()
        || ip.is_link_local()
        || ip.is_unspecified()
        || ip.is_broadcast()
        || (a == 100 && (64..128).contains(&b))
}

fn inside_v6(ip: Ipv6Addr) -> bool {
    let first = ip.segments()[0];
    ip.is_loopback()
        || ip.is_unspecified()
        || (first & 0xfe00) == 0xfc00
        || (first & 0xffc0) == 0xfe80
        || ip.to_ipv4_mapped().is_some_and(inside_v4)
}

/// 구독 주소의 이름이 내부망인가(해석 전 글자로 본다).
fn inside(url: &Url) -> bool {
    match url.host() {
        Some(Host::Domain(name)) => {
            let name = name.trim_end_matches('.').to_ascii_lowercase();
            name == "localhost"
                || [".localhost", ".local", ".internal"]
                    .iter()
                    .any(|end| name.ends_with(end))
        }
        Some(Host::Ipv4(ip)) => inside_v4(ip),
        Some(Host::Ipv6(ip)) => inside_v6(ip),
        None => true,
    }
}

async fn push_key(State(phone): State<Arc<Phone>>) -> Response {
    reply(
        StatusCode::OK,
        json!({ "key": phone.linked.notifier.vapid_public_key() }),
    )
}

async fn subscribe(State(phone): State<Arc<Phone>>, body: Bytes) -> Response {
    let body: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
    let text = |name: &str| body[name].as_str().unwrap_or_default().to_string();
    let endpoint = text("endpoint");
    let Ok(url) = Url::parse(&endpoint) else {
        return refuse(StatusCode::BAD_REQUEST, "invalid_input");
    };
    if url.scheme() != "https" {
        return refuse(StatusCode::BAD_REQUEST, "invalid_input");
    }
    if inside(&url) {
        return refuse(StatusCode::BAD_REQUEST, SUBSCRIBE_REFUSED);
    }
    let new = NewSubscription {
        endpoint,
        p256dh: text("p256dh"),
        auth: text("auth"),
        device: text("device").chars().take(60).collect(),
    };
    match phone.linked.notifier.subscribe(new) {
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(_) => refuse(StatusCode::BAD_REQUEST, "invalid_input"),
    }
}

/// `after=` · `max=` 를 읽는다.
fn query_number(query: Option<&str>, name: &str) -> Option<u64> {
    query?
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .find(|(key, _)| *key == name)
        .and_then(|(_, value)| value.parse().ok())
}

async fn notices(State(phone): State<Arc<Phone>>, RawQuery(query): RawQuery) -> Response {
    let after = query_number(query.as_deref(), "after").unwrap_or(0);
    let max = query_number(query.as_deref(), "max")
        .unwrap_or(50)
        .clamp(1, 200);
    let max = usize::try_from(max).unwrap_or(50);
    match phone.linked.notifier.notices_after(after, max) {
        Ok(list) => {
            let items: Vec<Value> = list
                .iter()
                .map(|n| {
                    let mut item = json!({
                        "seq": n.seq, "kind": n.kind.name(), "title": n.title, "body": n.body,
                        "at": n.at.map(|at| at.0),
                    });
                    if let Some(link) = &n.link {
                        item["link"] = json!(link);
                    }
                    item
                })
                .collect();
            reply(StatusCode::OK, Value::Array(items))
        }
        Err(error) => internal_error(error),
    }
}

async fn approval(State(phone): State<Arc<Phone>>, Path(id): Path<String>) -> Response {
    match phone.trading.approval_view(&id) {
        Ok(Some(view)) => reply(
            StatusCode::OK,
            json!({
                "number": view.number.as_str(),
                "summary": phone.linked.secrets.redact(&view.reason),
                "fingerprint": view.fingerprint.as_str(),
                "short_code": view.short_code,
                "status": approval_status_word(view.status),
            }),
        ),
        Ok(None) => refuse(StatusCode::NOT_FOUND, "not_found"),
        Err(error) => internal_error(error),
    }
}

async fn decide(
    State(phone): State<Arc<Phone>>,
    Path((id, verb)): Path<(String, String)>,
    body: Bytes,
) -> Response {
    if verb != "approve" && verb != "reject" {
        return refuse(StatusCode::NOT_FOUND, "not_found");
    }
    let body: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
    let shown = body["fingerprint"].as_str().unwrap_or_default();
    match phone.trading.answer(&id, shown, &verb).await {
        Ok(answered) => {
            let result = if answered.approved {
                "approved"
            } else {
                "rejected"
            };
            let status = if verb == "approve" && !answered.approved {
                StatusCode::CONFLICT
            } else {
                StatusCode::OK
            };
            reply(
                status,
                json!({ "result": result, "reason": answered.reason }),
            )
        }
        Err(error) => internal_error(error),
    }
}

async fn summary(State(phone): State<Arc<Phone>>) -> Response {
    match daily_summary(&phone.linked, &phone.trading, phone.crew.as_ref()).await {
        Ok(body) => reply(StatusCode::OK, body),
        Err(error) => internal_error(error),
    }
}

/// `GateBuilder::protected_routes`에 끼운다.
pub(crate) fn routes(phone: Arc<Phone>) -> Router {
    Router::new()
        .route(PUSH_KEY_PATH, get(push_key))
        .route(PUSH_SUBSCRIBE_PATH, post(subscribe))
        .route(NOTICES_PATH, get(notices))
        .route(APPROVAL_PATH, get(approval))
        .route(APPROVAL_DECIDE_PATH, post(decide))
        .route(DAILY_SUMMARY_PATH, get(summary))
        .with_state(phone)
}
