#![cfg(test)]
//! 3차 규칙 3 (대조표 C8): 이벤트 시각은 칸이다.
//!
//! 시각을 함께 받아 쌓고(`append_at`), 읽은 이벤트에 시각이 나오고(`read_events`),
//! 번호와 시각으로 범위를 정해 읽는다. 기존 `append` · `read_after`는 그대로다.
//!
//! 원본에서 확인한 것: Hermes 팀 논의 방 로그는 이벤트 줄에 `created_at` 칸을 둔다
//! (`gateway/hosted_rooms.py:110`), 읽은 이벤트에 그 시각이 함께 나온다(`:548`).

mod common;

use std::thread;

use common::{t, temp_store};
use pi_store::{Event, EventRange, Store, TimedEvent, UnixMillis};
use serde_json::{Value, json};

fn timed(seq: u64, kind: &str, at: Option<UnixMillis>, payload: Value) -> TimedEvent {
    TimedEvent {
        seq,
        kind: kind.to_string(),
        at,
        payload,
    }
}

fn all() -> EventRange {
    EventRange::default()
}

/// 범위에 든 이벤트의 번호.
fn seqs(store: &Store, range: EventRange, max: usize) -> Vec<u64> {
    store
        .read_events(range, max)
        .unwrap()
        .into_iter()
        .map(|e| e.seq)
        .collect()
}

/// 시각이 섞인 이벤트 8개를 쌓는다. 시각은 번호 순서와 다르고, 시각 없는 것이 섞여 있다.
///
/// 번호: 시각 = 1: 100 · 2: 없음 · 3: 300 · 4: 200 · 5: 없음 · 6: 300 · 7: 50 · 8: 400
fn mixed(store: &Store) {
    let rows: [Option<i64>; 8] = [
        Some(100),
        None,
        Some(300),
        Some(200),
        None,
        Some(300),
        Some(50),
        Some(400),
    ];
    for (i, at) in rows.into_iter().enumerate() {
        let want = i as u64 + 1;
        let payload = json!({ "n": want });
        let seq = match at {
            Some(at) => store.append_at("tick", &payload, t(at)).unwrap(),
            None => store.append("tick", &payload).unwrap(),
        };
        assert_eq!(seq, want);
    }
}

// 검증: C2, E3
#[test]
fn 시각과_함께_쌓은_이벤트는_시각이_함께_읽힌다() {
    let (_dir, _path, store) = temp_store();
    let payload = json!({ "who": "팀장", "text": "오늘은 쉰다", "tags": ["알림", 1, null] });
    assert_eq!(store.append_at("chat", &payload, t(5)).unwrap(), 1);

    assert_eq!(
        store.read_events(all(), 10).unwrap(),
        vec![timed(1, "chat", Some(t(5)), payload.clone())]
    );
    // 내용은 넣은 그대로다(시각을 내용에 싸 넣지 않는다). 기존 읽기에서도 같다.
    assert_eq!(
        store.read_after(0, 10).unwrap(),
        vec![Event {
            seq: 1,
            kind: "chat".to_string(),
            payload,
        }]
    );
}

// 검증: C2, E3
#[test]
fn 시각_없이_쌓은_이벤트는_시각이_없음으로_읽힌다() {
    let (_dir, _path, store) = temp_store();
    // 접속 관문이 쓰던 봉투 모양의 내용이어도 `append`로 쌓으면 시각 칸은 비어 있다.
    let envelope = json!({ "at": t(9).0, "payload": { "ok": true } });
    assert_eq!(store.append("chat", &json!({ "n": 1 })).unwrap(), 1);
    assert_eq!(store.append("feed_status", &envelope).unwrap(), 2);

    assert_eq!(
        store.read_events(all(), 10).unwrap(),
        vec![
            timed(1, "chat", None, json!({ "n": 1 })),
            timed(2, "feed_status", None, envelope),
        ]
    );
}

// 검증: C2
#[test]
fn 시각이_있든_없든_번호는_함께_1부터_빠짐없이_센다() {
    let (_dir, _path, store) = temp_store();
    mixed(&store);
    assert_eq!(seqs(&store, all(), 100), (1..=8).collect::<Vec<u64>>());
    assert_eq!(
        store
            .read_after(0, 100)
            .unwrap()
            .iter()
            .map(|e| e.seq)
            .collect::<Vec<_>>(),
        (1..=8).collect::<Vec<u64>>()
    );
    assert_eq!(store.append("tick", &json!({ "n": 9 })).unwrap(), 9);
    assert_eq!(
        store.append_at("tick", &json!({ "n": 10 }), t(1)).unwrap(),
        10
    );
}

// 검증: C2, E3
#[test]
fn 조건_없는_범위는_기존_읽기와_같은_이벤트를_같은_순서로_준다() {
    let (_dir, _path, store) = temp_store();
    assert!(store.read_events(all(), 10).unwrap().is_empty());
    mixed(&store);

    let plain = store.read_after(0, 100).unwrap();
    let with_time = store.read_events(all(), 100).unwrap();
    assert_eq!(
        with_time
            .iter()
            .map(|e| Event {
                seq: e.seq,
                kind: e.kind.clone(),
                payload: e.payload.clone(),
            })
            .collect::<Vec<_>>(),
        plain
    );
    assert_eq!(
        with_time.iter().map(|e| e.at).collect::<Vec<_>>(),
        vec![
            Some(t(100)),
            None,
            Some(t(300)),
            Some(t(200)),
            None,
            Some(t(300)),
            Some(t(50)),
            Some(t(400)),
        ]
    );
}

// 검증: C2, E3
#[test]
fn 번호_뒤부터_최대_개수만큼_이어_읽으면_겹치거나_빠지지_않는다() {
    let (_dir, _path, store) = temp_store();
    mixed(&store);

    assert!(store.read_events(all(), 0).unwrap().is_empty());
    let mut got = Vec::new();
    let mut after_seq = 0;
    loop {
        let page = seqs(&store, EventRange { after_seq, ..all() }, 3);
        if page.is_empty() {
            break;
        }
        assert!(page.len() <= 3);
        after_seq = page[page.len() - 1];
        got.extend(page);
    }
    assert_eq!(got, (1..=8).collect::<Vec<u64>>());

    for (after_seq, want) in [(7, vec![8]), (8, vec![]), (9, vec![]), (u64::MAX, vec![])] {
        assert_eq!(
            seqs(&store, EventRange { after_seq, ..all() }, 100),
            want,
            "{after_seq}"
        );
    }
}

// 검증: C2, E3
#[test]
fn 어느_시각_뒤의_이벤트만_번호_순서로_읽는다() {
    let (_dir, _path, store) = temp_store();
    mixed(&store);
    let after = |at: i64| EventRange {
        after_at: Some(t(at)),
        ..all()
    };
    // 같은 시각은 빠지고, 시각 없는 이벤트(2 · 5)는 나오지 않는다. 순서는 번호 순서다.
    assert_eq!(seqs(&store, after(0), 100), vec![1, 3, 4, 6, 7, 8]);
    assert_eq!(seqs(&store, after(50), 100), vec![1, 3, 4, 6, 8]);
    assert_eq!(seqs(&store, after(100), 100), vec![3, 4, 6, 8]);
    assert_eq!(seqs(&store, after(299), 100), vec![3, 6, 8]);
    assert_eq!(seqs(&store, after(300), 100), vec![8]);
    assert_eq!(seqs(&store, after(400), 100), Vec::<u64>::new());
}

// 검증: C2, E3
#[test]
fn 어느_시각_앞의_이벤트만_번호_순서로_읽는다() {
    let (_dir, _path, store) = temp_store();
    mixed(&store);
    let before = |at: i64| EventRange {
        before_at: Some(t(at)),
        ..all()
    };
    assert_eq!(seqs(&store, before(50), 100), Vec::<u64>::new());
    assert_eq!(seqs(&store, before(51), 100), vec![7]);
    assert_eq!(seqs(&store, before(300), 100), vec![1, 4, 7]);
    assert_eq!(seqs(&store, before(301), 100), vec![1, 3, 4, 6, 7]);
    assert_eq!(seqs(&store, before(1_000), 100), vec![1, 3, 4, 6, 7, 8]);
}

// 검증: C2, E3
#[test]
fn 번호와_시각_조건을_함께_주면_모두_맞는_것만_읽는다() {
    let (_dir, _path, store) = temp_store();
    mixed(&store);
    let range = EventRange {
        after_seq: 1,
        after_at: Some(t(50)),
        before_at: Some(t(400)),
    };
    // 번호 2부터, 시각이 50보다 늦고 400보다 이른 것: 3(300) · 4(200) · 6(300).
    assert_eq!(seqs(&store, range, 100), vec![3, 4, 6]);
    // 최대 개수는 조건에 맞는 것만 센다.
    assert_eq!(seqs(&store, range, 2), vec![3, 4]);
    assert_eq!(
        seqs(
            &store,
            EventRange {
                after_seq: 4,
                ..range
            },
            2
        ),
        vec![6]
    );
    // 비는 범위.
    assert_eq!(
        seqs(
            &store,
            EventRange {
                after_seq: 0,
                after_at: Some(t(300)),
                before_at: Some(t(300)),
            },
            100
        ),
        Vec::<u64>::new()
    );
    // 읽은 줄의 내용과 시각이 맞다.
    assert_eq!(
        store.read_events(range, 1).unwrap(),
        vec![timed(3, "tick", Some(t(300)), json!({ "n": 3 }))]
    );
}

// 검증: C2, E3, D6
#[test]
fn 이벤트_시각은_다시_열어도_닫지_못하고_꺼져도_남는다() {
    let (_dir, path, store) = temp_store();
    mixed(&store);
    let before = store.read_events(all(), 100).unwrap();
    assert_eq!(before.len(), 8);
    drop(store);

    let store = Store::open(&path).unwrap();
    assert_eq!(store.read_events(all(), 100).unwrap(), before);
    assert_eq!(
        store.append_at("tick", &json!({ "n": 9 }), t(500)).unwrap(),
        9
    );
    std::mem::forget(store);

    let store = Store::open(&path).unwrap();
    let after = store.read_events(all(), 100).unwrap();
    assert_eq!(after[..8], before[..]);
    assert_eq!(after[8], timed(9, "tick", Some(t(500)), json!({ "n": 9 })));
}

// 검증: C2
#[test]
fn 시각은_정수_전체_범위가_틀리지_않고_저장된다() {
    let (_dir, _path, store) = temp_store();
    let times = [i64::MIN, -1, 0, 1, 9_007_199_254_740_993, i64::MAX];
    for at in times {
        store
            .append_at("tick", &json!(null), UnixMillis(at))
            .unwrap();
    }
    assert_eq!(
        store
            .read_events(all(), 100)
            .unwrap()
            .into_iter()
            .map(|e| e.at)
            .collect::<Vec<_>>(),
        times
            .into_iter()
            .map(|at| Some(UnixMillis(at)))
            .collect::<Vec<_>>()
    );
    // 2^53 + 1 바로 앞뒤를 가른다(부동소수점을 거치면 못 가른다).
    assert_eq!(
        seqs(
            &store,
            EventRange {
                after_at: Some(UnixMillis(9_007_199_254_740_992)),
                before_at: Some(UnixMillis(9_007_199_254_740_994)),
                ..all()
            },
            100
        ),
        vec![5]
    );
}

// 검증: C2
#[test]
fn 여러_스레드가_시각과_함께_쌓아도_번호가_겹치거나_비지_않는다() {
    let (_dir, _path, store) = temp_store();
    let workers: Vec<_> = (0..4_i64)
        .map(|w| {
            let store = store.clone();
            thread::spawn(move || {
                (0..25_i64)
                    .map(|i| {
                        let at = t(w * 1_000 + i);
                        let seq = store
                            .append_at("work", &json!({ "w": w, "i": i }), at)
                            .unwrap();
                        (seq, at)
                    })
                    .collect::<Vec<_>>()
            })
        })
        .collect();
    let mut given: Vec<(u64, UnixMillis)> = workers
        .into_iter()
        .flat_map(|h| h.join().unwrap())
        .collect();
    given.sort();
    assert_eq!(
        given.iter().map(|x| x.0).collect::<Vec<_>>(),
        (1..=100).collect::<Vec<u64>>()
    );
    // 받은 번호의 줄에 그때 넘긴 시각이 적혀 있다.
    let read: Vec<(u64, UnixMillis)> = store
        .read_events(all(), 1_000)
        .unwrap()
        .into_iter()
        .map(|e| (e.seq, e.at.unwrap()))
        .collect();
    assert_eq!(read, given);
}
