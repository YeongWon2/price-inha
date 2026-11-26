//! (33차 1번, C18 · E43 · E44) 팀 방 멘션 글: `{"room": "team", "text", "sourceMessageId"}`(`agent` 없음).
//!
//! 원본 DeskRPG 오피스 방(정책 mention)과 같다: 글 속 `@[이름]`으로 멘션된 담당만 멘션한 차례로 답하고, 멘션이
//! 없으면 아무도 답하지 않는다(`no_mention`). 이름이 명단과 하나도 맞지 않으면 보낸 사람에게만 `no_match`를
//! 돌려준다(이벤트로 퍼뜨리지 않는다). 글은 가린 뒤 [`KIND_OFFICE_MESSAGE`]로 남는다.

use std::net::IpAddr;
use std::sync::Arc;

use axum::http::StatusCode;
use axum::response::Response;
use pi_office::{Agent, Member};
use serde_json::{Value, json};

use super::office_http::{OfficeDesk, chat_json, lock, parse_words, refuse, reply};
use super::{KIND_OFFICE_MESSAGE, OFFICE_BAD_INPUT, OFFICE_ROSTER, OFFICE_TOO_FAST};

/// 멘션 명단([`OFFICE_ROSTER`]).
pub(super) fn roster() -> Vec<Member> {
    OFFICE_ROSTER
        .iter()
        .filter_map(|(id, name)| {
            Agent::from_id(id).map(|agent| Member {
                agent,
                name: (*name).to_owned(),
            })
        })
        .collect()
}

/// 담당의 표시 이름(명단에 없으면 id).
pub(super) fn name_of(agent: Agent) -> &'static str {
    OFFICE_ROSTER
        .iter()
        .find(|(id, _)| *id == agent.id())
        .map_or(agent.id(), |(_, name)| name)
}

/// 팀 방 멘션 글을 받는다(관문은 지났다).
pub(super) async fn post(desk: &Arc<OfficeDesk>, body: &Value, device: IpAddr) -> Response {
    let (text, source) = match parse_words(body) {
        Ok(words) => words,
        Err(error) => return refuse(StatusCode::BAD_REQUEST, error),
    };
    if !desk.pace(device) {
        return refuse(StatusCode::TOO_MANY_REQUESTS, OFFICE_TOO_FAST);
    }
    let text = desk.linked.secrets.redact(&text);
    let office = &desk.linked.office;
    let posted = match office.post_team(&text, &source, &roster()).await {
        Ok(posted) => posted,
        Err(pi_office::Error::EmptyMessage) => {
            return refuse(StatusCode::BAD_REQUEST, OFFICE_BAD_INPUT);
        }
        Err(error) => {
            tracing::warn!(%error, "팀 방 글을 받지 못했다");
            return refuse(StatusCode::INTERNAL_SERVER_ERROR, "internal");
        }
    };
    let message = json!({"room": "team", "agent": null, "text": text, "sourceMessageId": source});
    if let Err(error) =
        pi_office::EventSink::publish(desk.linked.sink.as_ref(), KIND_OFFICE_MESSAGE, message).await
    {
        tracing::warn!(%error, "팀 방 글을 남기지 못했다");
    }
    if let Some(hint) = posted.hint {
        return reply(
            StatusCode::OK,
            json!({"responders": [], "responses": [], "hint": hint.as_str()}),
        );
    }
    let mut turns = Vec::new();
    for response in &posted.responses {
        if response.status.is_terminal() {
            continue;
        }
        lock(&desk.texts).insert(response.request_id.clone(), text.clone());
        turns.push(response.agent);
        // (54차) 사람이 직접 물은 채팅 답은 막지 않고 그 담당 · 팀의 AI 호출 하나로 센다.
        if let Some(crew) = &desk.crew {
            crew.count_turn(response.agent);
        }
    }
    // (결정 29) 글을 받은 그때 멘션된 담당 모두가 곧바로 곁으로 온다(답은 아래에서 차례로).
    desk.gather(&turns).await;
    tokio::spawn(Arc::clone(desk).work_in_turn(turns));
    let responders: Vec<&str> = posted.responders.iter().map(|a| a.id()).collect();
    let responses: Vec<Value> = posted.responses.iter().map(chat_json).collect();
    reply(
        StatusCode::OK,
        json!({"responders": responders, "responses": responses}),
    )
}
