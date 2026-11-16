//! 사무실 경로(세션이 있어야 열린다): 접속 스냅샷 [`OFFICE_PATH`]와 말하기 [`OFFICE_CHAT_PATH`].

use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use axum::Router;
use axum::body::Bytes;
use axum::extract::{ConnectInfo, State};
use axum::http::StatusCode;
use axum::http::header::CACHE_CONTROL;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use pi_office::{Agent, ChatResponse, Room};
use pi_store::{Store, UnixMillis};
use serde_json::{Value, json};

use super::linked::Linked;
use super::{
    CHAT_TOTAL_TIMEOUT, KIND_OFFICE_MESSAGE, OFFICE_BAD_INPUT, OFFICE_CHAT_PATH, OFFICE_PATH,
    OFFICE_SEND_INTERVAL, OFFICE_SOURCE_ID_MAX, OFFICE_TEXT_MAX, OFFICE_TOO_FAST, OFFICE_TOO_LONG,
};
use crate::Crew;
use crate::team::role_of;

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

pub(crate) fn reply(status: StatusCode, body: Value) -> Response {
    (status, [(CACHE_CONTROL, "no-store")], axum::Json(body)).into_response()
}

fn refuse(status: StatusCode, error: &str) -> Response {
    reply(status, json!({ "error": error }))
}

/// 응답 객체(`office.chat`과 같은 모양).
fn chat_json(response: &ChatResponse) -> Value {
    let room = match response.room {
        Room::Team => "team",
        Room::Direct => "direct",
    };
    json!({
        "requestId": response.request_id,
        "sourceMessageId": response.source_message_id,
        "agent": response.agent.id(),
        "room": room,
        "status": response.status.as_str(),
        "content": response.content,
        "updatedAt": response.updated_at.0,
        "error": response.error,
    })
}

/// 사무실 경로의 상태.
pub(crate) struct OfficeDesk {
    linked: Arc<Linked>,
    crew: Option<Arc<Crew>>,
    store: Store,
    /// 답을 기다리는 글(응답 번호 → 가린 글).
    texts: Mutex<HashMap<String, String>>,
    /// 담당마다 한 번에 하나씩 답한다.
    turns: HashMap<&'static str, tokio::sync::Mutex<()>>,
    /// 기기마다 마지막으로 받은 글의 시각.
    sent: Mutex<HashMap<IpAddr, UnixMillis>>,
    /// 마지막으로 본 이벤트 번호(스냅샷의 `last_seq`를 빨리 읽으려고).
    seen: Mutex<u64>,
}

/// 받은 말하기.
struct Ask {
    room: Room,
    agent: Agent,
    text: String,
    source: String,
}

fn parse_ask(body: &Value) -> Result<Ask, &'static str> {
    let room = match body["room"].as_str() {
        Some("team") => Room::Team,
        Some("direct") => Room::Direct,
        _ => return Err(OFFICE_BAD_INPUT),
    };
    let agent = body["agent"]
        .as_str()
        .and_then(Agent::from_id)
        .ok_or(OFFICE_BAD_INPUT)?;
    let text = body["text"].as_str().ok_or(OFFICE_BAD_INPUT)?;
    let source = body["sourceMessageId"].as_str().ok_or(OFFICE_BAD_INPUT)?;
    if source.is_empty() {
        return Err(OFFICE_BAD_INPUT);
    }
    if text.chars().count() > OFFICE_TEXT_MAX || source.chars().count() > OFFICE_SOURCE_ID_MAX {
        return Err(OFFICE_TOO_LONG);
    }
    if text.trim().is_empty() {
        return Err(OFFICE_BAD_INPUT);
    }
    Ok(Ask {
        room,
        agent,
        text: text.to_string(),
        source: source.to_string(),
    })
}

impl OfficeDesk {
    pub(crate) fn new(linked: Arc<Linked>, crew: Option<Arc<Crew>>, store: Store) -> Self {
        let turns = Agent::ALL
            .iter()
            .map(|a| (a.id(), tokio::sync::Mutex::new(())))
            .collect();
        Self {
            linked,
            crew,
            store,
            texts: Mutex::new(HashMap::new()),
            turns,
            sent: Mutex::new(HashMap::new()),
            seen: Mutex::new(0),
        }
    }

    /// 저장된 마지막 이벤트 번호.
    fn last_seq(&self) -> Result<u64, pi_store::StoreError> {
        let mut seen = lock(&self.seen);
        loop {
            let batch = self.store.read_after(*seen, 1000)?;
            match batch.last() {
                Some(last) => *seen = last.seq,
                None => return Ok(*seen),
            }
        }
    }

    /// 같은 기기가 간격 안에 또 보냈으면 거짓. 받으면 시각을 남긴다.
    fn pace(&self, device: IpAddr) -> bool {
        let now = self.linked.now();
        let gap = i64::try_from(OFFICE_SEND_INTERVAL.as_millis()).unwrap_or(i64::MAX);
        let mut sent = lock(&self.sent);
        if sent.get(&device).is_some_and(|last| now.0 - last.0 < gap) {
            return false;
        }
        sent.insert(device, now);
        true
    }

    /// 그 담당의 대기열에서 차례가 온 것에 답한다(뒤에서 돈다).
    async fn work(self: Arc<Self>, agent: Agent) {
        let Some(turn) = self.turns.get(agent.id()) else {
            return;
        };
        let _turn = turn.lock().await;
        let office = &self.linked.office;
        let next = match office.start_next(agent).await {
            Ok(Some(next)) => next,
            Ok(None) => return,
            Err(error) => {
                tracing::warn!(%error, "사무실 답을 시작하지 못했다");
                return;
            }
        };
        let text = lock(&self.texts)
            .remove(&next.request_id)
            .unwrap_or_default();
        let answer = match &self.crew {
            Some(crew) => {
                let asked = crew.chat(role_of(agent), &text);
                match tokio::time::timeout(CHAT_TOTAL_TIMEOUT, asked).await {
                    Ok(Ok(answer)) => Ok(self.linked.secrets.redact(&answer)),
                    Ok(Err(_)) => Err("agent_failed"),
                    Err(_) => Err("timeout"),
                }
            }
            None => Err("no_team"),
        };
        let done = match answer {
            Ok(answer) if !answer.trim().is_empty() => {
                office.complete(&next.request_id, &answer).await
            }
            Ok(_) => office.fail(&next.request_id, "empty_response").await,
            Err(why) => office.fail(&next.request_id, why).await,
        };
        if let Err(error) = done {
            tracing::warn!(%error, "사무실 답을 끝내지 못했다");
        }
    }
}

async fn snapshot(State(desk): State<Arc<OfficeDesk>>) -> Response {
    // 마지막 번호를 먼저 읽는다: 그 뒤의 바뀜은 실시간 연결로 받는다(E3).
    let last_seq = match desk.last_seq() {
        Ok(seq) => seq,
        Err(error) => {
            tracing::warn!(%error, "사무실 스냅샷을 읽지 못했다");
            return refuse(StatusCode::INTERNAL_SERVER_ERROR, "internal");
        }
    };
    let office = &desk.linked.office;
    let agents: Vec<Value> = office
        .agents()
        .iter()
        .map(|v| json!({"agent": v.agent.id(), "place": v.place.as_str(), "work": v.work.as_str()}))
        .collect();
    let pending: Vec<Value> = office.pending_responses().iter().map(chat_json).collect();
    reply(
        StatusCode::OK,
        json!({"agents": agents, "pending": pending, "last_seq": last_seq}),
    )
}

async fn chat(
    State(desk): State<Arc<OfficeDesk>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    body: Bytes,
) -> Response {
    let body: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
    let ask = match parse_ask(&body) {
        Ok(ask) => ask,
        Err(error) => return refuse(StatusCode::BAD_REQUEST, error),
    };
    if !desk.pace(peer.ip()) {
        return refuse(StatusCode::TOO_MANY_REQUESTS, OFFICE_TOO_FAST);
    }
    let text = desk.linked.secrets.redact(&ask.text);
    let office = &desk.linked.office;
    let response = match office.ask(ask.room, ask.agent, &text, &ask.source).await {
        Ok(response) => response,
        Err(pi_office::Error::QueueFull { .. }) => {
            return refuse(StatusCode::TOO_MANY_REQUESTS, "queue_full");
        }
        Err(pi_office::Error::EmptyMessage) => {
            return refuse(StatusCode::BAD_REQUEST, OFFICE_BAD_INPUT);
        }
        Err(error) => {
            tracing::warn!(%error, "사무실 글을 받지 못했다");
            return refuse(StatusCode::INTERNAL_SERVER_ERROR, "internal");
        }
    };
    if ask.room == Room::Team {
        let message = json!({
            "room": "team", "agent": ask.agent.id(), "text": text, "sourceMessageId": ask.source,
        });
        if let Err(error) =
            pi_office::EventSink::publish(desk.linked.sink.as_ref(), KIND_OFFICE_MESSAGE, message)
                .await
        {
            tracing::warn!(%error, "팀 방 글을 남기지 못했다");
        }
    }
    lock(&desk.texts).insert(response.request_id.clone(), text);
    tokio::spawn(Arc::clone(&desk).work(ask.agent));
    reply(StatusCode::OK, chat_json(&response))
}

/// `GateBuilder::protected_routes`에 끼운다.
pub(crate) fn routes(desk: Arc<OfficeDesk>) -> Router {
    Router::new()
        .route(OFFICE_PATH, get(snapshot))
        .route(OFFICE_CHAT_PATH, post(chat))
        .with_state(desk)
}
