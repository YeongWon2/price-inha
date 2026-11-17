//! 팀 논의 방 로그(10차 1번). 원본 근거: `docs/research/agent-runtime-deep.md` 7.1(Hermes hosted rooms).
//!
//! - 방마다 번호(`seq`)가 1부터 빠짐없이 커진다. `(방, seq)`가 한 줄을 가리킨다.
//! - `(방, event_id)`는 겹치지 않는다: `event_id`가 멱등 키다. 같은 `event_id`로 같은 내용을 다시
//!   쓰면 이미 있는 줄을 돌려주고(새 번호를 쓰지 않는다), 다른 내용이면 거부한다.
//! - 누가(쓴 쪽의 종류) 어떤 종류의 이벤트를 쓸 수 있는지가 고정돼 있다([`RoomActorKind`]).
//! - 쌓기만 한다. 지우거나 고치는 API는 없다.

use rusqlite::{Connection, OptionalExtension, Row, params};

use crate::codec::seq;
use crate::model::UnixMillis;
use crate::{Store, StoreError};

/// 방 이벤트를 쓴 쪽의 종류(원본의 `actor.kind`). 쓸 수 있는 이벤트 종류가 정해져 있다:
///
/// | 종류 | 쓸 수 있는 이벤트 |
/// |---|---|
/// | `User` | `message.user` |
/// | `Member` | `message.member` |
/// | `Gateway` | `member.unavailable`, `room.activity`, `room.assignment`, `room.continued`, `room.routed`, `room.stop_requested`, `turn.started`, `turn.settled`, `turn.failed`, `turn.cancelled`, `turn.deferred`, `turn.reassigned` |
/// | `System` | `authority.claimed`, `authority.lost`, `room.context_set`, `room.created`, `room.disbanded`, `room.members_changed`, `room.renamed` |
///
/// 표에 없는 짝(표에 없는 종류 포함)은 [`StoreError::RoomEventNotAllowed`]다.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RoomActorKind {
    /// 사용자.
    User,
    /// 방의 멤버(담당).
    Member,
    /// 방을 돌리는 쪽(차례 · 턴 기록).
    Gateway,
    /// 시스템(방 만들기 · 설정).
    System,
}

/// 쓸 방 이벤트.
#[derive(Debug, Clone, PartialEq)]
pub struct NewRoomEvent {
    /// 방 이름.
    pub room: String,
    /// 멱등 키. 같은 방 안에서 겹치지 않는다.
    pub event_id: String,
    /// 이벤트 종류(예: `message.user`). [`RoomActorKind`]의 표를 따른다.
    pub kind: String,
    /// 쓴 쪽의 종류.
    pub actor_kind: RoomActorKind,
    /// 쓴 쪽의 id(사용자 · 멤버 · 게이트웨이 이름).
    pub actor_id: String,
    /// 내용(JSON). 넣은 그대로 나온다.
    pub payload: serde_json::Value,
    /// 쓴 시각.
    pub at: UnixMillis,
}

/// 저장된 방 이벤트.
#[derive(Debug, Clone, PartialEq)]
pub struct RoomEvent {
    /// 방 이름.
    pub room: String,
    /// 방 안의 번호. 1부터 빠짐없이 커진다.
    pub seq: u64,
    /// 멱등 키.
    pub event_id: String,
    /// 종류.
    pub kind: String,
    /// 쓴 쪽의 종류.
    pub actor_kind: RoomActorKind,
    /// 쓴 쪽의 id.
    pub actor_id: String,
    /// 내용.
    pub payload: serde_json::Value,
    /// 쓴 시각.
    pub at: UnixMillis,
}

/// 방 이벤트 쓰기의 결과.
#[derive(Debug, Clone, PartialEq)]
pub enum RoomAppend {
    /// 새로 썼다.
    Appended(RoomEvent),
    /// 같은 `event_id`의 같은 내용이 이미 있다. 새로 쓰지 않았고 있던 줄을 돌려준다.
    AlreadyExists(RoomEvent),
}

impl Store {
    /// 방 이벤트를 쓴다.
    ///
    /// - 쓴 쪽의 종류가 그 이벤트 종류를 쓸 수 없으면 [`StoreError::RoomEventNotAllowed`]이고
    ///   아무것도 쓰지 않는다(번호를 쓰지 않는다).
    /// - 같은 방에 같은 `event_id`가 이미 있으면: 종류 · 쓴 쪽(종류와 id) · 내용이 모두 같으면
    ///   [`RoomAppend::AlreadyExists`](있던 줄, 시각은 처음 것), 하나라도 다르면
    ///   [`StoreError::RoomEventConflict`]이고 그대로다. 시각은 비교하지 않는다.
    /// - 다른 방의 같은 `event_id`는 겹치지 않는다.
    /// - 여러 스레드가 함께 써도 방 안의 번호가 겹치거나 비지 않는다. 다시 열어도 이어진다.
    pub fn append_room_event(&self, event: NewRoomEvent) -> Result<RoomAppend, StoreError> {
        self.write(|conn| {
            let actor = actor_code(event.actor_kind);
            let allowed: bool = conn.query_row(
                "SELECT EXISTS (SELECT 1 FROM room_event_kinds WHERE actor_kind = ?1 AND kind = ?2)",
                params![actor, event.kind],
                |row| row.get(0),
            )?;
            if !allowed {
                return Err(StoreError::RoomEventNotAllowed {
                    actor_kind: event.actor_kind,
                    kind: event.kind,
                });
            }
            let existing = conn
                .query_row(
                    &format!("{EVENT_SELECT} WHERE room = ?1 AND event_id = ?2"),
                    params![event.room, event.event_id],
                    event_from,
                )
                .optional()?
                .transpose()?;
            if let Some(found) = existing {
                return if same_content(&found, &event) {
                    Ok(RoomAppend::AlreadyExists(found))
                } else {
                    Err(StoreError::RoomEventConflict {
                        room: event.room,
                        event_id: event.event_id,
                    })
                };
            }
            insert_event(conn, event).map(RoomAppend::Appended)
        })
    }

    /// 방의 이벤트 가운데 번호가 `after`보다 큰 것을 번호 순서로 `max`개까지 준다.
    /// `after`가 0이면 처음부터다. 없는 방이면 빈 목록이다.
    pub fn room_events(
        &self,
        room: &str,
        after: u64,
        max: usize,
    ) -> Result<Vec<RoomEvent>, StoreError> {
        if max == 0 {
            return Ok(Vec::new());
        }
        let after = i64::try_from(after).unwrap_or(i64::MAX);
        let max = i64::try_from(max).unwrap_or(i64::MAX);
        self.read(|conn| {
            let mut stmt = conn.prepare(&format!(
                "{EVENT_SELECT} WHERE room = ?1 AND seq > ?2 ORDER BY seq LIMIT ?3"
            ))?;
            let rows = stmt.query_map(params![room, after, max], event_from)?;
            rows.map(|row| row?).collect()
        })
    }

    /// 방의 마지막 번호. 이벤트가 없거나 없는 방이면 0이다.
    pub fn room_last_seq(&self, room: &str) -> Result<u64, StoreError> {
        self.read(|conn| {
            let last: i64 = conn.query_row(
                "SELECT COALESCE(MAX(seq), 0) FROM room_events WHERE room = ?1",
                [room],
                |row| row.get(0),
            )?;
            u64::try_from(last)
                .map_err(|_| StoreError::Incompatible(format!("방 번호가 이상하다: {last}")))
        })
    }
}

const EVENT_SELECT: &str =
    "SELECT room, seq, event_id, kind, actor_kind, actor_id, payload, at FROM room_events";

fn actor_code(kind: RoomActorKind) -> &'static str {
    match kind {
        RoomActorKind::User => "user",
        RoomActorKind::Member => "member",
        RoomActorKind::Gateway => "gateway",
        RoomActorKind::System => "system",
    }
}

fn actor_from(code: &str) -> Result<RoomActorKind, StoreError> {
    match code {
        "user" => Ok(RoomActorKind::User),
        "member" => Ok(RoomActorKind::Member),
        "gateway" => Ok(RoomActorKind::Gateway),
        "system" => Ok(RoomActorKind::System),
        other => Err(StoreError::Incompatible(format!(
            "모르는 방 actor 값: {other}"
        ))),
    }
}

fn event_from(row: &Row<'_>) -> rusqlite::Result<Result<RoomEvent, StoreError>> {
    let build = || -> Result<RoomEvent, StoreError> {
        Ok(RoomEvent {
            room: row.get(0)?,
            seq: seq(row, 1)?,
            event_id: row.get(2)?,
            kind: row.get(3)?,
            actor_kind: actor_from(&row.get::<_, String>(4)?)?,
            actor_id: row.get(5)?,
            payload: serde_json::from_str(&row.get::<_, String>(6)?)?,
            at: UnixMillis(row.get(7)?),
        })
    };
    Ok(build())
}

/// 멱등 비교: 종류 · 쓴 쪽(종류와 id) · 내용. 시각은 보지 않는다.
fn same_content(found: &RoomEvent, event: &NewRoomEvent) -> bool {
    found.kind == event.kind
        && found.actor_kind == event.actor_kind
        && found.actor_id == event.actor_id
        && found.payload == event.payload
}

/// 방의 다음 번호로 쓴다(쓰기 잠금 안에서 부르므로 번호가 겹치거나 비지 않는다).
fn insert_event(conn: &Connection, event: NewRoomEvent) -> Result<RoomEvent, StoreError> {
    let last: i64 = conn.query_row(
        "SELECT COALESCE(MAX(seq), 0) FROM room_events WHERE room = ?1",
        [&event.room],
        |row| row.get(0),
    )?;
    let next = last + 1;
    conn.execute(
        "INSERT INTO room_events (room, seq, event_id, kind, actor_kind, actor_id, payload, at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            event.room,
            next,
            event.event_id,
            event.kind,
            actor_code(event.actor_kind),
            event.actor_id,
            event.payload.to_string(),
            event.at.0,
        ],
    )?;
    Ok(RoomEvent {
        room: event.room,
        seq: u64::try_from(next)
            .map_err(|_| StoreError::Incompatible(format!("방 번호가 이상하다: {next}")))?,
        event_id: event.event_id,
        kind: event.kind,
        actor_kind: event.actor_kind,
        actor_id: event.actor_id,
        payload: event.payload,
        at: event.at,
    })
}
