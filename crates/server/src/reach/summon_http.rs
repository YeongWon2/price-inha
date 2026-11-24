//! (33차 2번 · 4번, C20 · E47 · E48 · N10) 담당 부르기: 부르기 [`OFFICE_SUMMON_PATH`] · 도착 알림
//! [`OFFICE_SUMMON_ARRIVED_PATH`] · 보는 방 알림 [`OFFICE_SUMMON_VIEW_PATH`].
//!
//! 단계와 시각은 사무실 크레이트가 쥐고(원본 DeskRPG: called → waiting → returning → idle), 걷는 길 · 도착 판정은
//! 화면이 한다. 단계가 바뀌면 감싼 이벤트 포트가 `office.summon`을 낸다. 10초 · 30초는 서버의 시계 굴리기가 본다.

use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::Response;
use pi_office::{Agent, Room, SummonPhase};
use serde_json::{Value, json};

use super::office_http::{OfficeDesk, refuse, reply};
use super::{OFFICE_BAD_INPUT, OFFICE_MEETING_RESERVED, OFFICE_UNKNOWN_AGENT};

/// 몸통의 `agent`(모르는 담당 · 빠짐이면 `None`).
fn agent_of(body: &Bytes) -> Option<Agent> {
    let body: Value = serde_json::from_slice(body).ok()?;
    body["agent"].as_str().and_then(Agent::from_id)
}

fn phase_reply(agent: Agent, phase: SummonPhase) -> Response {
    reply(
        StatusCode::OK,
        json!({"agent": agent.id(), "phase": phase.as_str()}),
    )
}

fn failed(error: &pi_office::Error) -> Response {
    tracing::warn!(%error, "부르기 단계를 바꾸지 못했다");
    refuse(StatusCode::INTERNAL_SERVER_ERROR, "internal")
}

/// `POST {"agent"}` 담당을 부른다.
pub(super) async fn summon(State(desk): State<Arc<OfficeDesk>>, body: Bytes) -> Response {
    let Some(agent) = agent_of(&body) else {
        return refuse(StatusCode::BAD_REQUEST, OFFICE_UNKNOWN_AGENT);
    };
    match desk.linked.office.summon(agent).await {
        Ok(phase) => phase_reply(agent, phase),
        Err(pi_office::Error::MeetingReserved { .. }) => {
            refuse(StatusCode::CONFLICT, OFFICE_MEETING_RESERVED)
        }
        Err(error) => failed(&error),
    }
}

/// `POST {"agent"}` 걸어온(또는 자리로 돌아간) 담당이 도착했다.
pub(super) async fn arrived(State(desk): State<Arc<OfficeDesk>>, body: Bytes) -> Response {
    let Some(agent) = agent_of(&body) else {
        return refuse(StatusCode::BAD_REQUEST, OFFICE_UNKNOWN_AGENT);
    };
    let office = &desk.linked.office;
    // 원본 `npc:arrived`는 곁에 닿은 것과 자리에 닿은 것이 같은 알림이다: 오는 중이면 기다림, 돌아가는 중이면 자리.
    let phase = match office.summon_arrived(agent).await {
        Ok(SummonPhase::Returning) => office.summon_returned(agent).await,
        other => other,
    };
    match phase {
        Ok(phase) => phase_reply(agent, phase),
        Err(error) => failed(&error),
    }
}

/// `POST {"room"}` 화면이 지금 보는 방. 팀 방이 아니면 멘션으로 온 담당이 바로 돌아간다.
pub(super) async fn view(State(desk): State<Arc<OfficeDesk>>, body: Bytes) -> Response {
    let body: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
    let room = match body["room"].as_str() {
        Some("team") => Room::Team,
        Some("direct") => Room::Direct,
        _ => return refuse(StatusCode::BAD_REQUEST, OFFICE_BAD_INPUT),
    };
    if room != Room::Team
        && let Err(error) = desk.linked.office.user_left_room(Room::Team).await
    {
        return failed(&error);
    }
    reply(StatusCode::OK, json!({"room": body["room"]}))
}
