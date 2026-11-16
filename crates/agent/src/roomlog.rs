//! 논의 하나가 방 로그에 쓰는 줄들(`message.member` · `turn.failed` · `room.activity`).

use std::sync::atomic::{AtomicU32, Ordering};

use pi_ai::Redactor;
use pi_store::{NewRoomEvent, RoomActorKind, Store, StoreError, TradeId};
use serde_json::{Value, json};

use crate::ports::Clock;
use crate::role::Role;

/// 실행기가 쓰는 줄의 쓴 쪽 id(`Gateway`).
const GATEWAY_ID: &str = "agent";

/// 방의 마지막 번호를 찾을 때 한 번에 읽는 줄 수.
const PAGE: usize = 500;

/// 실행기가 쓰는 줄이 거치는 것: 저장소 · 시계 · 가리기.
pub(crate) struct Writer<'a> {
    pub(crate) store: &'a Store,
    pub(crate) clock: &'a dyn Clock,
    pub(crate) redactor: &'a dyn Redactor,
}

/// 논의 하나의 방 로그 쓰기.
pub(crate) struct RoomLog<'a> {
    writer: Writer<'a>,
    room: &'a str,
    prefix: String,
    next: AtomicU32,
}

impl<'a> RoomLog<'a> {
    /// 방 `room`에서 매매 `trade`의 논의를 쓴다.
    ///
    /// 줄 id는 매매 번호 · 시작 시각 · 시작 때 방의 마지막 번호 · 순번이다. 같은 시각에 같은
    /// 매매를 다시 논의해도(앞 논의가 줄을 남겨 마지막 번호가 커졌으므로) 겹치지 않는다.
    pub(crate) fn new(
        writer: Writer<'a>,
        room: &'a str,
        trade: TradeId,
    ) -> Result<RoomLog<'a>, StoreError> {
        let last = last_seq(writer.store, room)?;
        let prefix = format!("trade-{}-{}-{last}", trade.0, writer.clock.now().0);
        Ok(RoomLog {
            writer,
            room,
            prefix,
            next: AtomicU32::new(0),
        })
    }

    /// 담당의 글(`message.member`).
    pub(crate) fn member(&self, role: Role, payload: Value) -> Result<(), pi_store::StoreError> {
        self.append("message.member", RoomActorKind::Member, role.id(), payload)
    }

    /// 담당 턴 실패(`turn.failed`).
    /// 까닭 글(공급자 오류 문구 등)은 가려서 남긴다(E38).
    pub(crate) fn failed(&self, role: Role, reason: &str) -> Result<(), pi_store::StoreError> {
        let reason = self.writer.redactor.redact(reason);
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
            at: self.writer.clock.now(),
        };
        self.writer.store.append_room_event(event).map(|_| ())
    }
}

/// 방의 마지막 줄 번호. 빈 방이면 0이다.
fn last_seq(store: &Store, room: &str) -> Result<u64, StoreError> {
    let mut last = 0;
    loop {
        let page = store.room_events(room, last, PAGE)?;
        let Some(end) = page.last() else {
            return Ok(last);
        };
        last = end.seq;
        if page.len() < PAGE {
            return Ok(last);
        }
    }
}
