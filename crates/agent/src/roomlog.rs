//! 논의 하나가 방 로그에 쓰는 줄들(`message.member` · `turn.failed` · `room.activity`).

use std::sync::atomic::{AtomicU32, Ordering};

use pi_store::{NewRoomEvent, RoomActorKind, Store, TradeId};
use serde_json::{Value, json};

use crate::ports::Clock;
use crate::role::Role;

/// 실행기가 쓰는 줄의 쓴 쪽 id(`Gateway`).
const GATEWAY_ID: &str = "agent";

/// 논의 하나의 방 로그 쓰기.
pub(crate) struct RoomLog<'a> {
    store: &'a Store,
    clock: &'a dyn Clock,
    room: &'a str,
    prefix: String,
    next: AtomicU32,
}

impl<'a> RoomLog<'a> {
    /// 방 `room`에서 매매 `trade`의 논의를 쓴다. 줄 id는 매매 번호 · 시작 시각 · 순번이다.
    pub(crate) fn new(
        store: &'a Store,
        clock: &'a dyn Clock,
        room: &'a str,
        trade: TradeId,
    ) -> RoomLog<'a> {
        let prefix = format!("trade-{}-{}", trade.0, clock.now().0);
        RoomLog {
            store,
            clock,
            room,
            prefix,
            next: AtomicU32::new(0),
        }
    }

    /// 담당의 글(`message.member`).
    pub(crate) fn member(&self, role: Role, payload: Value) -> Result<(), pi_store::StoreError> {
        self.append("message.member", RoomActorKind::Member, role.id(), payload)
    }

    /// 담당 턴 실패(`turn.failed`).
    pub(crate) fn failed(&self, role: Role, reason: &str) -> Result<(), pi_store::StoreError> {
        let payload = json!({"member": role.id(), "reason": reason});
        self.append("turn.failed", RoomActorKind::Gateway, GATEWAY_ID, payload)
    }

    /// 지켜보기로 끝남(`room.activity`).
    pub(crate) fn watch(&self, code: &str) -> Result<(), pi_store::StoreError> {
        let payload = json!({"status": "watch", "reason_code": code});
        self.append("room.activity", RoomActorKind::Gateway, GATEWAY_ID, payload)
    }

    fn append(
        &self,
        kind: &str,
        actor_kind: RoomActorKind,
        actor_id: &str,
        payload: Value,
    ) -> Result<(), pi_store::StoreError> {
        let number = self.next.fetch_add(1, Ordering::Relaxed);
        let event = NewRoomEvent {
            room: self.room.to_string(),
            event_id: format!("{}-{number}", self.prefix),
            kind: kind.to_string(),
            actor_kind,
            actor_id: actor_id.to_string(),
            payload,
            at: self.clock.now(),
        };
        self.store.append_room_event(event).map(|_| ())
    }
}
