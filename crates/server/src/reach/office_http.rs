//! 사무실 경로(세션이 있어야 열린다): 접속 스냅샷 [`OFFICE_PATH`]와 말하기 [`OFFICE_CHAT_PATH`].
//! (33차) 팀 방 멘션 글은 `team_chat`, 답 만들기는 `office_work`, 공지는 `notice_http`, 부르기는 `summon_http`에 있다.

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
    KIND_OFFICE_MESSAGE, OFFICE_BAD_INPUT, OFFICE_CHAT_PATH, OFFICE_NOTICE_PATH,
    OFFICE_NOTICES_PATH, OFFICE_PATH, OFFICE_PENDING_APPROVALS, OFFICE_SEND_INTERVAL,
    OFFICE_SOURCE_ID_MAX, OFFICE_SUMMON_ARRIVED_PATH, OFFICE_SUMMON_PATH, OFFICE_SUMMON_VIEW_PATH,
    OFFICE_TEXT_MAX, OFFICE_TOO_FAST, OFFICE_TOO_LONG,
};
use crate::trading::approval_agent;
use crate::{Crew, Trading};

pub(super) fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

pub(crate) fn reply(status: StatusCode, body: Value) -> Response {
    (status, [(CACHE_CONTROL, "no-store")], axum::Json(body)).into_response()
}

pub(super) fn refuse(status: StatusCode, error: &str) -> Response {
    reply(status, json!({ "error": error }))
}

/// 응답 객체(`office.chat`과 같은 모양).
pub(super) fn chat_json(response: &ChatResponse) -> Value {
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
    pub(super) linked: Arc<Linked>,
    pub(super) crew: Option<Arc<Crew>>,
    pub(super) store: Store,
    /// (34차 1번) 기다리는 승인 요청을 읽는 주문 길(없으면 기다리는 승인 칸은 빈 목록).
    trading: Option<Arc<Trading>>,
    /// 답을 기다리는 글(응답 번호 → 가린 글).
    pub(super) texts: Mutex<HashMap<String, String>>,
    /// (33차 3번) 받은 공지의 처음 답(글 번호 → 답). 같은 글 번호는 한 번만 처리한다.
    pub(super) notice_replies: tokio::sync::Mutex<Vec<(String, Value)>>,
    /// 담당마다 한 번에 하나씩 답한다.
    pub(super) turns: HashMap<&'static str, tokio::sync::Mutex<()>>,
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
    let (text, source) = parse_words(body)?;
    Ok(Ask {
        room,
        agent,
        text,
        source,
    })
}

/// 글(`text`)과 글 번호(`sourceMessageId`)를 읽는다: 없거나 비면 [`OFFICE_BAD_INPUT`], 길면 [`OFFICE_TOO_LONG`].
/// (33차) 팀 방 멘션 글 · 공지도 같은 한도다.
pub(super) fn parse_words(body: &Value) -> Result<(String, String), &'static str> {
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
    Ok((text.to_string(), source.to_string()))
}

impl OfficeDesk {
    /// (34차 1번, C27) 스냅샷의 기다리는 승인 칸: 한 줄은 정확히 `{agent, number, created_at, summary, short_code}`.
    /// 요약은 승인 카드와 같이 비밀 값을 가린다. 전체 지문은 싣지 않는다. 읽지 못하면 빈 목록이다.
    fn pending_approvals(&self) -> Vec<Value> {
        let Some(trading) = &self.trading else {
            return Vec::new();
        };
        match trading.pending_approvals() {
            Ok(list) => list
                .into_iter()
                .map(|(view, created_at)| {
                    json!({
                        "agent": approval_agent(&view.subject),
                        "number": view.number.as_str(),
                        "created_at": created_at,
                        "summary": self.linked.secrets.redact(&view.reason),
                        "short_code": view.short_code,
                    })
                })
                .collect(),
            Err(error) => {
                tracing::warn!(%error, "기다리는 승인 요청을 읽지 못했다");
                Vec::new()
            }
        }
    }

    pub(crate) fn new(
        linked: Arc<Linked>,
        crew: Option<Arc<Crew>>,
        store: Store,
        trading: Option<Arc<Trading>>,
    ) -> Self {
        let turns = Agent::ALL
            .iter()
            .map(|a| (a.id(), tokio::sync::Mutex::new(())))
            .collect();
        Self {
            linked,
            crew,
            store,
            trading,
            texts: Mutex::new(HashMap::new()),
            notice_replies: tokio::sync::Mutex::new(Vec::new()),
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
    pub(super) fn pace(&self, device: IpAddr) -> bool {
        let now = self.linked.now();
        let gap = i64::try_from(OFFICE_SEND_INTERVAL.as_millis()).unwrap_or(i64::MAX);
        let mut sent = lock(&self.sent);
        // 간격이 지난 기기는 지운다(오래된 것부터 쌓이지 않게).
        sent.retain(|_, last| now.0 - last.0 < gap);
        if sent.get(&device).is_some_and(|last| now.0 - last.0 < gap) {
            return false;
        }
        sent.insert(device, now);
        true
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
        .map(|v| {
            json!({
                "agent": v.agent.id(), "place": v.place.as_str(), "work": v.work.as_str(),
                // (33차 4번) 부르기 단계(다시 들어와도 곁에 온 담당을 그린다).
                "phase": v.phase.as_str(), "reason": v.reason.map(pi_office::SummonReason::as_str),
            })
        })
        .collect();
    let pending: Vec<Value> = office.pending_responses().iter().map(chat_json).collect();
    // (22차 1번) 첫 화면의 팀 대화: 최근 줄 가운데 `last_seq`까지(그 뒤는 실시간 연결로 온다). 번호를 붙여 준다.
    let talk: Vec<Value> = match desk.linked.talk.recent().await {
        Ok(lines) => lines
            .into_iter()
            .filter(|(seq, _)| *seq <= last_seq)
            .map(|(seq, mut talk)| {
                talk["seq"] = json!(seq);
                talk
            })
            .collect(),
        Err(error) => {
            tracing::warn!(%error, "팀 대화 최근 줄을 읽지 못했다");
            Vec::new()
        }
    };
    // (34차 1번, C27 · E49) 아직 기다리는 승인 요청(다시 열어도 ✋를 다시 띄운다). 마지막 번호 뒤에 읽으므로
    // 그 사이에 생긴 요청은 여기와 실시간 연결 양쪽에 올 수 있다(화면은 같은 번호를 하나로 본다).
    let approvals = desk.pending_approvals();
    reply(
        StatusCode::OK,
        json!({
            "agents": agents, "pending": pending, "last_seq": last_seq, "talk": talk,
            OFFICE_PENDING_APPROVALS: approvals,
            // (51차, C21) 지금 도는 논의의 진행 단계(없으면 null).
            "progress": desk.linked.progress.current(),
        }),
    )
}

async fn chat(
    State(desk): State<Arc<OfficeDesk>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    body: Bytes,
) -> Response {
    let body: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
    if body["room"] == "team" && body.get("agent").is_none_or(Value::is_null) {
        return super::team_chat::post(&desk, &body, peer.ip()).await;
    }
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
    // (33차 4번) 곁에서 기다리는 담당에게 말을 걸면 10초를 다시 센다.
    office.note_conversation(ask.agent);
    // (54차) 사람이 직접 물은 채팅 답은 막지 않고 그 담당 · 팀의 AI 호출 하나로 센다.
    if let Some(crew) = &desk.crew {
        crew.count_turn(ask.agent);
    }
    tokio::spawn(Arc::clone(&desk).work(ask.agent));
    reply(StatusCode::OK, chat_json(&response))
}

/// `GateBuilder::protected_routes`에 끼운다.
pub(crate) fn routes(desk: Arc<OfficeDesk>) -> Router {
    Router::new()
        .route(OFFICE_PATH, get(snapshot))
        .route(OFFICE_CHAT_PATH, post(chat))
        .route(OFFICE_NOTICE_PATH, post(super::notice_http::send))
        .route(OFFICE_NOTICES_PATH, get(super::notice_http::list))
        .route(OFFICE_SUMMON_PATH, post(super::summon_http::summon))
        .route(
            OFFICE_SUMMON_ARRIVED_PATH,
            post(super::summon_http::arrived),
        )
        .route(OFFICE_SUMMON_VIEW_PATH, post(super::summon_http::view))
        .with_state(desk)
}
