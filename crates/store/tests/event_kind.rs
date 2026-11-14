#![cfg(test)]
//! 3차 규칙 2 (대조표 C9): 이벤트 종류 이름에 규칙이 있다.
//!
//! 이름은 `^[a-z][a-z0-9_.-]*$`이고 64자 이하다. 어긋나면 구분되는 오류로 거부하고 번호를 쓰지 않는다.
//!
//! 원본에서 확인한 것: Hermes `gateway/hosted_rooms.py:27`(`MAX_EVENT_KIND_CHARS = 64`),
//! `:48`(`_EVENT_KIND_RE`), `:245-247`(종류 이름 검사), `gateway/hosted_rooms_common.py:23-32`
//! (빈 이름 · 길이 초과 · 모양이 다르면 거부, 모양은 이름 전체에 맞아야 한다).
//! 원본은 검사 전에 앞뒤 공백을 떼어 낸다. 앞뒤 공백이 붙은 이름을 받을지는 지시서가 정하지 않아
//! 여기서는 확인하지 않는다(가운데 공백과 공백뿐인 이름은 원본도 거부한다).

mod common;

use common::{t, temp_store};
use pi_store::{EVENT_KIND_MAX_CHARS, Store, StoreError};
use serde_json::json;

/// 규칙에 맞는 이름.
const GOOD: [&str; 14] = [
    "a",
    "z",
    "chat",
    "run.started",
    "message.delta",
    "room.stop_requested",
    "feed_status",
    "quotes-stale",
    "a1",
    "a0.b9",
    "a..b",
    "a.",
    "a_",
    "z9._-",
];

/// 규칙에 어긋나는 이름.
const BAD: [&str; 27] = [
    "",
    " ",
    "   ",
    "A",
    "Chat",
    "run.Started",
    "feed_Status",
    "1a",
    "9",
    "_a",
    ".a",
    "-a",
    "a b",
    "a\tb",
    "a\nb",
    "a/b",
    "a:b",
    "a,b",
    "a*",
    "a$",
    "a\\b",
    "a\"b",
    "a'b",
    "a%",
    "한글",
    "a한",
    "é",
];

fn assert_invalid(result: Result<u64, StoreError>, name: &str) {
    match result {
        Err(StoreError::InvalidEventKind { kind }) => assert_eq!(kind, name),
        other => panic!("{name:?}은 이름 규칙에 어긋난다: {other:?}"),
    }
}

/// 쌓인 이벤트의 (번호, 종류).
fn kinds(store: &Store) -> Vec<(u64, String)> {
    store
        .read_after(0, 1_000)
        .unwrap()
        .into_iter()
        .map(|e| (e.seq, e.kind))
        .collect()
}

// 검증: C2, E3
#[test]
fn 규칙에_어긋나는_이름은_거부하고_번호를_쓰지_않는다() {
    let (_dir, _path, store) = temp_store();
    let mut want = Vec::new();
    for (i, good) in GOOD.iter().enumerate() {
        // 맞는 이름 사이사이에 어긋나는 이름을 모두 넣어 본다.
        for bad in BAD {
            assert_invalid(store.append(bad, &json!({ "bad": true })), bad);
        }
        let seq = store.append(good, &json!({ "n": i })).unwrap();
        assert_eq!(seq, i as u64 + 1, "{good:?}");
        want.push((seq, good.to_string()));
    }
    // 거부된 것은 하나도 쌓이지 않았고 번호가 비지 않는다.
    assert_eq!(kinds(&store), want);
}

// 검증: C2, E3
#[test]
fn 시각과_함께_쌓을_때도_이름_규칙이_같다() {
    let (_dir, _path, store) = temp_store();
    let mut want = Vec::new();
    for (i, good) in GOOD.iter().enumerate() {
        for bad in BAD {
            assert_invalid(
                store.append_at(bad, &json!({ "bad": true }), t(i as i64)),
                bad,
            );
        }
        let seq = store
            .append_at(good, &json!({ "n": i }), t(i as i64))
            .unwrap();
        assert_eq!(seq, i as u64 + 1, "{good:?}");
        want.push((seq, good.to_string()));
    }
    assert_eq!(kinds(&store), want);
}

// 검증: C2
#[test]
fn 이름은_64자까지이고_65자부터_거부한다() {
    assert_eq!(EVENT_KIND_MAX_CHARS, 64);
    let (_dir, _path, store) = temp_store();

    let longest = "a".repeat(64);
    let dotted = format!("room.{}", "x".repeat(59));
    assert_eq!(dotted.len(), 64);
    assert_eq!(store.append(&longest, &json!(1)).unwrap(), 1);
    assert_eq!(store.append_at(&dotted, &json!(2), t(2)).unwrap(), 2);

    for too_long in ["a".repeat(65), "a".repeat(200), format!("{dotted}.")] {
        assert_invalid(store.append(&too_long, &json!(3)), &too_long);
        assert_invalid(store.append_at(&too_long, &json!(3), t(3)), &too_long);
    }
    assert_eq!(kinds(&store), vec![(1, longest), (2, dotted)]);
    assert_eq!(store.append("chat", &json!(4)).unwrap(), 3);
}

// 검증: C2, E3
#[test]
fn 거부된_이름은_다시_열어도_쌓여_있지_않고_번호는_1부터다() {
    let (_dir, path, store) = temp_store();
    for bad in BAD {
        assert_invalid(store.append(bad, &json!({ "bad": true })), bad);
        assert_invalid(store.append_at(bad, &json!({ "bad": true }), t(1)), bad);
    }
    assert!(store.read_after(0, 100).unwrap().is_empty());
    drop(store);

    let store = Store::open(&path).unwrap();
    assert!(store.read_after(0, 100).unwrap().is_empty());
    assert_eq!(store.append("chat", &json!({ "n": 1 })).unwrap(), 1);
    assert_eq!(kinds(&store), vec![(1, "chat".to_string())]);
}

// 검증: C2, M6
#[test]
fn 이름이_거부돼도_접속_기록과_다른_자료는_그대로다() {
    let (_dir, _path, store) = temp_store();
    store.append("chat", &json!({ "n": 1 })).unwrap();
    store
        .record_access(pi_store::NewAccess {
            at: t(1),
            origin: "127.0.0.1".to_string(),
            action: "들어옴".to_string(),
        })
        .unwrap();
    let events = store.read_after(0, 10).unwrap();
    let access = store.access_log().unwrap();

    assert_invalid(store.append("Chat", &json!({ "n": 2 })), "Chat");
    assert_eq!(store.read_after(0, 10).unwrap(), events);
    assert_eq!(store.access_log().unwrap(), access);
    assert_eq!(store.append("chat", &json!({ "n": 2 })).unwrap(), 2);
}
