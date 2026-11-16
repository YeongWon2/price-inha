#![cfg(test)]
//! 14차: 방 로그 마지막 번호 한 줄 읽기 · 접속 기록 최근 n줄(새것부터).

mod common;

use common::{t, temp_store};
use pi_store::{NewAccess, NewRoomEvent, RoomActorKind, Store};
use serde_json::json;

fn put(store: &Store, room: &str, id: &str) {
    store
        .append_room_event(NewRoomEvent {
            room: room.to_owned(),
            event_id: id.to_owned(),
            kind: "message.user".to_owned(),
            actor_kind: RoomActorKind::User,
            actor_id: "me".to_owned(),
            payload: json!({ "id": id }),
            at: t(1),
        })
        .unwrap();
}

fn access(store: &Store, n: i64) {
    store
        .record_access(NewAccess {
            at: t(n),
            origin: format!("o{n}"),
            action: format!("a{n}"),
        })
        .unwrap();
}

#[test]
fn 방_마지막_번호는_없는_방이면_0이고_쓸수록_따라간다() {
    let (_dir, _path, store) = temp_store();
    assert_eq!(store.room_last_seq("r1").unwrap(), 0);
    put(&store, "r1", "a");
    put(&store, "r1", "b");
    put(&store, "r2", "a");
    assert_eq!(store.room_last_seq("r1").unwrap(), 2);
    assert_eq!(store.room_last_seq("r2").unwrap(), 1);
    assert_eq!(store.room_last_seq("none").unwrap(), 0);
}

#[test]
fn 방_마지막_번호는_같은_id_다시_쓰기로_늘지_않고_다시_열어도_이어진다() {
    let (dir, path, store) = temp_store();
    put(&store, "r1", "a");
    put(&store, "r1", "a");
    assert_eq!(store.room_last_seq("r1").unwrap(), 1);
    drop(store);
    let again = Store::open(&path).unwrap();
    assert_eq!(again.room_last_seq("r1").unwrap(), 1);
    put(&again, "r1", "b");
    assert_eq!(again.room_last_seq("r1").unwrap(), 2);
    drop(dir);
}

#[test]
fn 접속_최근_n줄은_새것부터_n개만_준다() {
    let (_dir, _path, store) = temp_store();
    assert!(store.access_recent(5).unwrap().is_empty());
    for n in 1..=5 {
        access(&store, n);
    }
    let seqs = |n: usize| -> Vec<u64> {
        store
            .access_recent(n)
            .unwrap()
            .iter()
            .map(|r| r.seq)
            .collect()
    };
    assert_eq!(seqs(2), vec![5, 4]);
    assert_eq!(seqs(5), vec![5, 4, 3, 2, 1]);
    assert_eq!(seqs(99), vec![5, 4, 3, 2, 1]);
    assert!(seqs(0).is_empty());
    let top = &store.access_recent(1).unwrap()[0];
    assert_eq!(
        (top.at, top.origin.as_str(), top.action.as_str()),
        (t(5), "o5", "a5")
    );
}

#[test]
fn 접속_전체_읽기는_그대로_번호_순서다() {
    let (_dir, _path, store) = temp_store();
    for n in 1..=3 {
        access(&store, n);
    }
    let all: Vec<u64> = store.access_log().unwrap().iter().map(|r| r.seq).collect();
    assert_eq!(all, vec![1, 2, 3]);
}
