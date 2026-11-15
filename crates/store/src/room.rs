//! 팀 논의 방 로그(10차 1번). 원본 근거: `docs/research/agent-runtime-deep.md` 7.1(Hermes hosted rooms).
//!
//! - 방마다 번호(`seq`)가 1부터 빠짐없이 커진다. `(방, seq)`가 한 줄을 가리킨다.
//! - `(방, event_id)`는 겹치지 않는다: `event_id`가 멱등 키다. 같은 `event_id`로 같은 내용을 다시
//!   쓰면 이미 있는 줄을 돌려주고(새 번호를 쓰지 않는다), 다른 내용이면 거부한다.
//! - 누가(쓴 쪽의 종류) 어떤 종류의 이벤트를 쓸 수 있는지가 고정돼 있다([`RoomActorKind`]).
//! - 쌓기만 한다. 지우거나 고치는 API는 없다.

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
        todo!()
    }

    /// 방의 이벤트 가운데 번호가 `after`보다 큰 것을 번호 순서로 `max`개까지 준다.
    /// `after`가 0이면 처음부터다. 없는 방이면 빈 목록이다.
    pub fn room_events(
        &self,
        room: &str,
        after: u64,
        max: usize,
    ) -> Result<Vec<RoomEvent>, StoreError> {
        todo!()
    }
}
